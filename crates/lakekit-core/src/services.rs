use loko_clock::{Clock, HasClock, TestClock};
use loko_macros::Services;
use loko_random::{HasRandom, Random, SeededRandom};
use loko_store::{HasStore, MemoryStore, Store};

/// A store, a clock, and a random source as one value, for APIs that need all three (commits):
/// `Services::new(ObjectStoreAdapter::new(…), TokioClock, OsRandom)`. Any type that is all three
/// works as well: a JavaScript host, or an application's own `#[derive(loko::Services)]` struct.
#[derive(Clone, Debug, Default, Services)]
pub struct Services<S, C, R> {
    /// The store.
    #[service(Store)]
    pub store: S,
    /// The clock.
    #[service(Clock)]
    pub clock: C,
    /// The random source.
    #[service(Random)]
    pub random: R,
}

impl<S, C, R> Services<S, C, R> {
    /// `store` with `clock` and `random`.
    pub fn new(store: S, clock: C, random: R) -> Self {
        Self { store, clock, random }
    }
}

/// Everything in memory, for tests: a [`MemoryStore`], a [`TestClock`], and a [`SeededRandom`].
pub type MemoryServices = Services<MemoryStore, TestClock, SeededRandom>;

impl MemoryServices {
    /// An empty store, the clock at `now_ms` milliseconds after the Unix epoch, and seed 0.
    pub fn at_ms(now_ms: i64) -> Self {
        Self::new(MemoryStore::new(), TestClock::at_ms(now_ms), SeededRandom::new(0))
    }
}
