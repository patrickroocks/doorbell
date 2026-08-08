use std::sync::Arc;
use tokio::sync::Semaphore;

/// A notification primitive that remembers its state.
///
/// Once signaled, all current and future waiters will pass through immediately.
/// This solves race conditions where tasks start waiting after the signal was sent.
pub struct NotifyWithState {
    sem: Semaphore,
}

impl NotifyWithState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { sem: Semaphore::new(0) })
    }

    /// Signals all current and future waiters.
    ///
    /// After calling this, all `notified()` calls will return immediately.
    /// Closing the semaphore is what makes the signal permanent, and it is idempotent - two tasks
    /// signaling at the same time (e.g. two subsystems failing at startup) is safe.
    pub fn signal(&self) {
        self.sem.close();
    }

    /// Waits for the signal to be sent.
    ///
    /// If the signal has already been sent, this returns immediately.
    /// Otherwise, it waits until `signal()` is called.
    pub async fn notified(&self) {
        // Acquiring only ever fails because the semaphore was closed, which is exactly the signal.
        if let Ok(permit) = self.sem.acquire().await {
            permit.forget();
        }
    }

    /// Non-awaitable check if the signal has been sent.
    ///
    pub fn isSignaled(&self) -> bool {
        self.sem.is_closed()
    }
}
