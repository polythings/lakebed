use std::future::Future;

use crate::{Clock, Listed, Random, Result, Store};

/// A store, a clock, and a random source as one value, for APIs that need all three (commits):
/// `Services::new(ObjectStoreAdapter::new(…), TokioClock, OsRandom)`. A type that is all three (a
/// JavaScript host, [`MemoryStore`](crate::MemoryStore)) needs no wrapper.
#[derive(Clone, Debug, Default)]
pub struct Services<S, C, R> {
    /// The store.
    pub store: S,
    /// The clock.
    pub clock: C,
    /// The random source.
    pub random: R,
}

impl<S, C, R> Services<S, C, R> {
    /// `store` with `clock` and `random`.
    pub fn new(store: S, clock: C, random: R) -> Self {
        Self { store, clock, random }
    }
}

impl<S: Store, C, R> Store for Services<S, C, R> {
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

impl<S, C: Clock, R> Clock for Services<S, C, R> {
    fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }

    fn sleep(&self, ms: u64) -> impl Future<Output = ()> {
        self.clock.sleep(ms)
    }
}

impl<S, C, R: Random> Random for Services<S, C, R> {
    fn next_u64(&self) -> u64 {
        self.random.next_u64()
    }
}
