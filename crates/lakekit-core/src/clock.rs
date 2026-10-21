use std::future::Future;

/// The time and a timer: commit timestamps, time-ordered file names, and the wait between commit
/// attempts.
///
/// The future isn't required to be `Send`, so a clock can wrap JavaScript timers.
pub trait Clock {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
    /// Waits `ms` milliseconds.
    fn sleep(&self, ms: u64) -> impl Future<Output = ()>;
}

/// The system clock and tokio's timer (feature `tokio`, native targets only).
#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
#[derive(Clone, Copy, Debug, Default)]
pub struct TokioClock;

#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
impl Clock for TokioClock {
    fn now_ms(&self) -> i64 {
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
    }

    fn sleep(&self, ms: u64) -> impl Future<Output = ()> {
        tokio::time::sleep(std::time::Duration::from_millis(ms))
    }
}

#[cfg(all(test, feature = "tokio", not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_tokio_clock_tells_time_and_waits() {
        let before = TokioClock.now_ms();
        assert!(before > 1_700_000_000_000, "after November 2023");
        TokioClock.sleep(5).await;
        assert!(TokioClock.now_ms() >= before + 5);
    }
}
