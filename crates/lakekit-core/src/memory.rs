use std::collections::BTreeMap;
use std::future::{Future, ready};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{Clock, Error, Listed, Random, Result, Store, Upload};

/// A [`Store`] in memory that is also a [`Clock`] that moves only when told (or when `sleep` is
/// called) and a [`Random`] with a seeded sequence: for tests and examples. Clones share the same
/// objects.
#[derive(Clone, Debug, Default)]
pub struct MemoryStore {
    objects: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    clock: Arc<Mutex<i64>>,
    seed: Arc<Mutex<u64>>,
}

impl MemoryStore {
    /// An empty store at time `now_ms`.
    pub fn new(now_ms: i64) -> Self {
        let store = Self::default();
        *lock(&store.clock) = now_ms;
        store
    }

    /// Moves the clock forward.
    pub fn advance(&self, ms: i64) {
        *lock(&self.clock) += ms;
    }

    /// The URIs stored, sorted.
    pub fn uris(&self) -> Vec<String> {
        self.objects().keys().cloned().collect()
    }

    fn objects(&self) -> MutexGuard<'_, BTreeMap<String, Vec<u8>>> {
        lock(&self.objects)
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An upload to a [`MemoryStore`].
#[derive(Debug)]
pub struct MemoryUpload {
    store: MemoryStore,
    uri: String,
    bytes: Vec<u8>,
}

impl Upload for MemoryUpload {
    fn put_part(&mut self, bytes: Vec<u8>) -> impl Future<Output = Result<()>> {
        self.bytes.extend(bytes);
        ready(Ok(()))
    }

    fn complete(self) -> impl Future<Output = Result<()>> {
        self.store.objects().insert(self.uri, self.bytes);
        ready(Ok(()))
    }

    fn abort(self) -> impl Future<Output = ()> {
        ready(())
    }
}

impl Store for MemoryStore {
    type Upload = MemoryUpload;

    fn get(&self, uri: &str) -> impl Future<Output = Result<Option<Vec<u8>>>> {
        ready(Ok(self.objects().get(uri).cloned()))
    }

    fn get_range(&self, uri: &str, start: u64, len: u64) -> impl Future<Output = Result<Vec<u8>>> {
        let objects = self.objects();
        ready(objects.get(uri).ok_or_else(|| Error::NotFound(uri.to_string())).map(|bytes| {
            let start = usize::try_from(start).unwrap_or(usize::MAX).min(bytes.len());
            let end = start.saturating_add(usize::try_from(len).unwrap_or(usize::MAX)).min(bytes.len());
            bytes[start..end].to_vec()
        }))
    }

    fn exists(&self, uri: &str) -> impl Future<Output = Result<bool>> {
        ready(Ok(self.objects().contains_key(uri)))
    }

    fn put(&self, uri: &str, bytes: Vec<u8>) -> impl Future<Output = Result<()>> {
        self.objects().insert(uri.to_string(), bytes);
        ready(Ok(()))
    }

    fn put_if_absent(&self, uri: &str, bytes: Vec<u8>) -> impl Future<Output = Result<bool>> {
        let mut objects = self.objects();
        let absent = !objects.contains_key(uri);
        if absent {
            objects.insert(uri.to_string(), bytes);
        }
        ready(Ok(absent))
    }

    fn upload(&self, uri: &str) -> impl Future<Output = Result<MemoryUpload>> {
        ready(Ok(MemoryUpload { store: self.clone(), uri: uri.to_string(), bytes: vec![] }))
    }

    fn list(&self, prefix: &str) -> impl Future<Output = Result<Vec<Listed>>> {
        let listed = self
            .objects()
            .range(prefix.to_string()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| Listed { uri: k.clone(), size: v.len() as u64 })
            .collect();
        ready(Ok(listed))
    }

    fn delete(&self, uris: &[String]) -> impl Future<Output = Result<()>> {
        let mut objects = self.objects();
        for uri in uris {
            objects.remove(uri);
        }
        ready(Ok(()))
    }
}

impl Clock for MemoryStore {
    fn now_ms(&self) -> i64 {
        *lock(&self.clock)
    }

    fn sleep(&self, ms: u64) -> impl Future<Output = ()> {
        self.advance(i64::try_from(ms).unwrap_or(i64::MAX));
        ready(())
    }
}

impl Random for MemoryStore {
    fn next_u64(&self) -> u64 {
        // A linear congruential generator (Knuth's MMIX constants), with the high bits folded in.
        let mut seed = lock(&self.seed);
        *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        *seed ^ (*seed >> 29)
    }
}
