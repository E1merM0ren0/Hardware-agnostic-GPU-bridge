# GPU synchronization foundation

This crate is the host-side synchronization foundation for the distributed GPU
orchestrator. It is intentionally independent of Vulkan and any GPU vendor.

## Current scope

- Opaque index + generation resource handles.
- Centralized timeline registry.
- Monotonic signal reservation and completion tracking.
- Terminal failure propagation.
- Timeout-aware blocking waits.
- Stale-handle rejection and slot recycling.
- No unsafe Rust and no third-party dependencies.

## Build and test

```sh
cargo test
cargo test --release
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

## Semantics

`reserve_signal` allocates a monotonically increasing value. It does not submit
GPU work and does not indicate completion.

`complete` must only be called after the owning backend has observed actual
completion from a valid device synchronization primitive.

`fail` makes a timeline terminal. It cannot be reset; create a new timeline for
a new execution lifetime.

Destroying a timeline invalidates its handle. A recycled slot receives a new
generation. A generation is never wrapped; a slot at generation exhaustion is
retired permanently.

## Deliberate limitations in this milestone

- The registry uses one mutex and one condition variable. This is easy to audit
  and appropriate until profiling demonstrates contention.
- `wait` blocks an OS thread. Async integration should use a dedicated blocking
  pool or a later event-driven adapter.
- There is no Vulkan integration yet. Vulkan fences, semaphores, queue submission,
  pipeline barriers, and memory visibility rules belong in the next milestone.
- There is no cross-process or network synchronization here. Remote completion
  must be authenticated and correlated with request IDs in the transport layer.

## Next milestone

Add a Vulkan adapter that owns all Vulkan objects privately, probes timeline
semaphore and synchronization2 support, and translates observed GPU completion
into coordinator events. The adapter must preserve device/queue ownership and
must not assume cross-device or cross-machine semaphore sharing.
