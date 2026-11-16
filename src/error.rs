/// Stable failure categories reported by a synchronization timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureCode {
    /// The device was lost or became unusable.
    DeviceLost,
    /// The underlying queue submission failed.
    SubmissionFailed,
    /// The owning worker was stopped or cancelled.
    WorkerStopped,
    /// An upstream dependency failed.
    DependencyFailed,
    /// An implementation-specific failure occurred.
    InternalFailure,
}

/// Errors returned by the synchronization coordinator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncError {
    /// A handle is stale, unknown, or already destroyed.
    InvalidHandle,
    /// A requested value has not been submitted.
    ValueNotSubmitted,
    /// Completion would move the timeline backwards.
    CompletionRegression,
    /// The timeline has entered a terminal failure state.
    TimelineFailed(FailureCode),
    /// The requested completion was not observed before the timeout.
    Timeout,
    /// A monotonic timeline counter cannot be incremented further.
    CounterExhausted,
    /// The registry cannot represent another resource.
    CapacityExhausted,
    /// A synchronization lock was poisoned by a panicking thread.
    LockPoisoned,
}
