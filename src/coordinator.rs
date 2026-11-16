use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::{FailureCode, ResourceHandle, SyncError};
use crate::timeline::Timeline;

#[derive(Debug)]
struct Slot {
    generation: u32,
    timeline: Option<Timeline>,
    retired: bool,
}

#[derive(Debug, Default)]
struct Registry {
    slots: Vec<Slot>,
    free_slots: Vec<u32>,
}

impl Registry {
    fn insert(&mut self) -> Result<ResourceHandle, SyncError> {
        while let Some(index) = self.free_slots.pop() {
            let slot = self
                .slots
                .get_mut(index as usize)
                .ok_or(SyncError::InvalidHandle)?;

            if slot.retired || slot.timeline.is_some() {
                continue;
            }

            slot.timeline = Some(Timeline::new());
            return Ok(ResourceHandle::new(index, slot.generation));
        }

        let index = u32::try_from(self.slots.len())
            .map_err(|_| SyncError::CapacityExhausted)?;

        self.slots.push(Slot {
            generation: 1,
            timeline: Some(Timeline::new()),
            retired: false,
        });

        Ok(ResourceHandle::new(index, 1))
    }

    fn get(&self, handle: ResourceHandle) -> Result<&Timeline, SyncError> {
        let slot = self
            .slots
            .get(handle.index() as usize)
            .ok_or(SyncError::InvalidHandle)?;

        if slot.generation != handle.generation() || slot.retired {
            return Err(SyncError::InvalidHandle);
        }

        slot.timeline.as_ref().ok_or(SyncError::InvalidHandle)
    }

    fn get_mut(&mut self, handle: ResourceHandle) -> Result<&mut Timeline, SyncError> {
        let slot = self
            .slots
            .get_mut(handle.index() as usize)
            .ok_or(SyncError::InvalidHandle)?;

        if slot.generation != handle.generation() || slot.retired {
            return Err(SyncError::InvalidHandle);
        }

        slot.timeline.as_mut().ok_or(SyncError::InvalidHandle)
    }

    fn remove(&mut self, handle: ResourceHandle) -> Result<(), SyncError> {
        let slot = self
            .slots
            .get_mut(handle.index() as usize)
            .ok_or(SyncError::InvalidHandle)?;

        if slot.generation != handle.generation()
            || slot.retired
            || slot.timeline.is_none()
        {
            return Err(SyncError::InvalidHandle);
        }

        slot.timeline = None;

        match slot.generation.checked_add(1) {
            Some(next_generation) => {
                slot.generation = next_generation;
                self.free_slots.push(handle.index());
            }
            None => {
                // Never wrap a generation: wrapping could make an ancient
                // handle valid again (the ABA problem).
                slot.retired = true;
            }
        }

        Ok(())
    }
}

/// Immutable view of a synchronization timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineSnapshot {
    /// Highest signal value reserved for submission.
    pub submitted: u64,
    /// Highest value known to have completed.
    pub completed: u64,
    /// Terminal failure, if the timeline has failed.
    pub failure: Option<FailureCode>,
}

/// Thread-safe owner of host-side synchronization timelines.
///
/// All registry mutation is serialized by one mutex. This is intentionally
/// simple for the first milestone; per-slot locking or sharding should only be
/// introduced after contention is measured.
pub struct SynchronizationCoordinator {
    registry: Mutex<Registry>,
    changed: Condvar,
}

impl Default for SynchronizationCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl SynchronizationCoordinator {
    /// Creates an empty coordinator.
    pub fn new() -> Self {
        Self {
            registry: Mutex::new(Registry::default()),
            changed: Condvar::new(),
        }
    }

    /// Creates a new timeline and returns its opaque handle.
    pub fn create_timeline(&self) -> Result<ResourceHandle, SyncError> {
        let mut registry = self.registry.lock().map_err(|_| SyncError::LockPoisoned)?;
        registry.insert()
    }

    /// Destroys a timeline. All subsequent uses of its handle fail.
    pub fn destroy_timeline(&self, handle: ResourceHandle) -> Result<(), SyncError> {
        let mut registry = self.registry.lock().map_err(|_| SyncError::LockPoisoned)?;
        registry.remove(handle)?;
        drop(registry);
        self.changed.notify_all();
        Ok(())
    }

    /// Reserves the next monotonically increasing signal value.
    ///
    /// The caller should associate this value with a real queue submission.
    /// Reservation alone does not mean the GPU work has completed.
    pub fn reserve_signal(&self, handle: ResourceHandle) -> Result<u64, SyncError> {
        let mut registry = self.registry.lock().map_err(|_| SyncError::LockPoisoned)?;
        registry.get_mut(handle)?.reserve_signal()
    }

    /// Publishes completion of a previously reserved value.
    ///
    /// A Vulkan adapter must call this only after observing the corresponding
    /// fence, semaphore, or other valid completion primitive.
    pub fn complete(&self, handle: ResourceHandle, value: u64) -> Result<(), SyncError> {
        let mut registry = self.registry.lock().map_err(|_| SyncError::LockPoisoned)?;
        registry.get_mut(handle)?.complete(value)?;
        drop(registry);
        self.changed.notify_all();
        Ok(())
    }

    /// Places a timeline into a terminal failure state and wakes its waiters.
    pub fn fail(
        &self,
        handle: ResourceHandle,
        code: FailureCode,
    ) -> Result<(), SyncError> {
        let mut registry = self.registry.lock().map_err(|_| SyncError::LockPoisoned)?;
        registry.get_mut(handle)?.fail(code);
        drop(registry);
        self.changed.notify_all();
        Ok(())
    }

    /// Returns a consistent snapshot of a timeline.
    pub fn snapshot(&self, handle: ResourceHandle) -> Result<TimelineSnapshot, SyncError> {
        let registry = self.registry.lock().map_err(|_| SyncError::LockPoisoned)?;
        let timeline = registry.get(handle)?;

        Ok(TimelineSnapshot {
            submitted: timeline.submitted(),
            completed: timeline.completed(),
            failure: timeline.failure(),
        })
    }

    /// Waits until `value` completes, the timeline fails, it is destroyed,
    /// or `timeout` expires.
    ///
    /// This method blocks the calling host thread. Do not call it from a
    /// latency-sensitive async executor thread; bridge it through a dedicated
    /// blocking worker or use snapshots and event-driven notification instead.
    pub fn wait(
        &self,
        handle: ResourceHandle,
        value: u64,
        timeout: Duration,
    ) -> Result<(), SyncError> {
        let start = Instant::now();
        let mut registry = self.registry.lock().map_err(|_| SyncError::LockPoisoned)?;

        loop {
            let timeline = registry.get(handle)?;

            if let Some(code) = timeline.failure() {
                return Err(SyncError::TimelineFailed(code));
            }
            if value > timeline.submitted() {
                return Err(SyncError::ValueNotSubmitted);
            }
            if timeline.completed() >= value {
                return Ok(());
            }

            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(SyncError::Timeout);
            }

            let (next_registry, wait_result) = self
                .changed
                .wait_timeout(registry, remaining)
                .map_err(|_| SyncError::LockPoisoned)?;

            registry = next_registry;

            if wait_result.timed_out() {
                // Re-check state while holding the lock before reporting timeout.
                let timeline = registry.get(handle)?;
                if let Some(code) = timeline.failure() {
                    return Err(SyncError::TimelineFailed(code));
                }
                if timeline.completed() >= value {
                    return Ok(());
                }
                return Err(SyncError::Timeout);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn signal_values_are_monotonic() {
        let coordinator = SynchronizationCoordinator::new();
        let handle = coordinator.create_timeline().unwrap();

        assert_eq!(coordinator.reserve_signal(handle).unwrap(), 1);
        assert_eq!(coordinator.reserve_signal(handle).unwrap(), 2);
        coordinator.complete(handle, 2).unwrap();

        let snapshot = coordinator.snapshot(handle).unwrap();
        assert_eq!(snapshot.submitted, 2);
        assert_eq!(snapshot.completed, 2);
    }

    #[test]
    fn rejects_completion_that_was_never_submitted() {
        let coordinator = SynchronizationCoordinator::new();
        let handle = coordinator.create_timeline().unwrap();

        assert_eq!(
            coordinator.complete(handle, 1),
            Err(SyncError::ValueNotSubmitted)
        );
    }

    #[test]
    fn recycled_slot_rejects_stale_handle() {
        let coordinator = SynchronizationCoordinator::new();
        let old = coordinator.create_timeline().unwrap();
        coordinator.destroy_timeline(old).unwrap();

        let new = coordinator.create_timeline().unwrap();
        assert_eq!(old.index(), new.index());
        assert_ne!(old.generation(), new.generation());
        assert_eq!(coordinator.snapshot(old), Err(SyncError::InvalidHandle));
        assert!(coordinator.snapshot(new).is_ok());
    }

    #[test]
    fn wait_observes_completion_from_another_thread() {
        let coordinator = std::sync::Arc::new(SynchronizationCoordinator::new());
        let handle = coordinator.create_timeline().unwrap();
        let value = coordinator.reserve_signal(handle).unwrap();

        let worker_coordinator = std::sync::Arc::clone(&coordinator);
        let worker = thread::spawn(move || {
            worker_coordinator.complete(handle, value).unwrap();
        });

        assert_eq!(
            coordinator.wait(handle, value, Duration::from_secs(2)),
            Ok(())
        );
        worker.join().unwrap();
    }

    #[test]
    fn wait_returns_terminal_failure() {
        let coordinator = SynchronizationCoordinator::new();
        let handle = coordinator.create_timeline().unwrap();
        let value = coordinator.reserve_signal(handle).unwrap();
        coordinator.fail(handle, FailureCode::DeviceLost).unwrap();

        assert_eq!(
            coordinator.wait(handle, value, Duration::from_millis(1)),
            Err(SyncError::TimelineFailed(FailureCode::DeviceLost))
        );
    }

    #[test]
    fn wait_times_out_deterministically() {
        let coordinator = SynchronizationCoordinator::new();
        let handle = coordinator.create_timeline().unwrap();
        let value = coordinator.reserve_signal(handle).unwrap();

        assert_eq!(
            coordinator.wait(handle, value, Duration::ZERO),
            Err(SyncError::Timeout)
        );
    }
}
