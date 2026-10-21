use std::future::Future;

use crate::{Listed, Result, Store};

/// What a writer needs from where it runs besides its store: the time, randomness (snapshot ids and
/// file names), and a way to wait (between commit attempts).
///
/// The future isn't required to be `Send`, so an environment can wrap JavaScript timers.
pub trait Env {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
    /// A random number.
    fn random(&self) -> u64;
    /// Waits `ms` milliseconds.
    fn sleep(&self, ms: u64) -> impl Future<Output = ()>;
}

/// A [`Store`] and an [`Env`] as one value, for APIs that need both (commits):
/// `WithEnv::new(ObjectStoreAdapter::new(…), TokioEnv)`.
#[derive(Clone, Debug, Default)]
pub struct WithEnv<S, E> {
    /// The store.
    pub store: S,
    /// The environment.
    pub env: E,
}

impl<S, E> WithEnv<S, E> {
    /// `store` with `env`.
    pub fn new(store: S, env: E) -> Self {
        Self { store, env }
    }
}

impl<S: Store, E> Store for WithEnv<S, E> {
    type Upload = S::Upload;

    fn get(&self, uri: &str) -> impl Future<Output = Result<Option<Vec<u8>>>> {
        self.store.get(uri)
    }

    fn get_range(&self, uri: &str, start: u64, len: u64) -> impl Future<Output = Result<Vec<u8>>> {
        self.store.get_range(uri, start, len)
    }

    fn exists(&self, uri: &str) -> impl Future<Output = Result<bool>> {
        self.store.exists(uri)
    }

    fn put(&self, uri: &str, bytes: Vec<u8>) -> impl Future<Output = Result<()>> {
        self.store.put(uri, bytes)
    }

    fn put_if_absent(&self, uri: &str, bytes: Vec<u8>) -> impl Future<Output = Result<bool>> {
        self.store.put_if_absent(uri, bytes)
    }

    fn upload(&self, uri: &str) -> impl Future<Output = Result<Self::Upload>> {
        self.store.upload(uri)
    }

    fn list(&self, prefix: &str) -> impl Future<Output = Result<Vec<Listed>>> {
        self.store.list(prefix)
    }

    fn delete(&self, uris: &[String]) -> impl Future<Output = Result<()>> {
        self.store.delete(uris)
    }
}

impl<S, E: Env> Env for WithEnv<S, E> {
    fn now_ms(&self) -> i64 {
        self.env.now_ms()
    }

    fn random(&self) -> u64 {
        self.env.random()
    }

    fn sleep(&self, ms: u64) -> impl Future<Output = ()> {
        self.env.sleep(ms)
    }
}

/// The environment of a native program on tokio (feature `tokio`): the system clock, the operating
/// system's randomness, and tokio's timer.
#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
#[derive(Clone, Copy, Debug, Default)]
pub struct TokioEnv;

#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
impl Env for TokioEnv {
    fn now_ms(&self) -> i64 {
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
    }

    fn random(&self) -> u64 {
        // The operating system's generator only fails when it isn't available at all.
        getrandom::u64().expect("the operating system's random number generator")
    }

    fn sleep(&self, ms: u64) -> impl Future<Output = ()> {
        tokio::time::sleep(std::time::Duration::from_millis(ms))
    }
}

#[cfg(all(test, feature = "tokio", not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_tokio_env_tells_time_and_waits() {
        let env = TokioEnv;
        let before = env.now_ms();
        assert!(before > 1_700_000_000_000, "after November 2023");
        env.sleep(5).await;
        assert!(env.now_ms() >= before + 5);
        assert_ne!(env.random(), env.random());
    }
}
