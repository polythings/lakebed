/// Random numbers: snapshot ids, file names, and the jitter between commit attempts.
pub trait Random {
    /// A random number.
    fn next_u64(&self) -> u64;
}

/// The operating system's random number generator (feature `getrandom`, native targets only).
#[cfg(all(feature = "getrandom", not(target_arch = "wasm32")))]
#[derive(Clone, Copy, Debug, Default)]
pub struct OsRandom;

#[cfg(all(feature = "getrandom", not(target_arch = "wasm32")))]
impl Random for OsRandom {
    fn next_u64(&self) -> u64 {
        // The operating system's generator only fails when it isn't available at all.
        getrandom::u64().expect("the operating system's random number generator")
    }
}

#[cfg(all(test, feature = "getrandom", not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn os_random_numbers_differ() {
        assert_ne!(OsRandom.next_u64(), OsRandom.next_u64());
    }
}
