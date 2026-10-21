use loko_clock::{Clock, HasClock, TestClock};
use loko_random::{HasRandom, Random, SeededRandom};
use loko_store::{HasStore, MemoryStore, Store};

/// A store, a clock, and a random source as one value, for APIs that need all three (commits):
/// `Services::new(ObjectStoreAdapter::new(…), TokioClock, OsRandom)`. Any type that is all three
/// works as well: a JavaScript host, or an application's own `#[loko::bundle]` struct.
#[derive(Clone, Debug, Default)]
pub struct Services<S, C, R> {
    /// The store.
    pub store: S,
    /// The clock.
    pub clock: C,
    /// The random source.
    pub random: R,
}

// The three impls `#[loko::bundle]` would write, by hand, so lakekit needs no macros.
impl<S: Store, C, R> HasStore for Services<S, C, R> {
    type Store = S;

    fn store(&self) -> &S {
        &self.store
    }
}

impl<S, C: Clock, R> HasClock for Services<S, C, R> {
    type Clock = C;

    fn clock(&self) -> &C {
        &self.clock
    }
}

impl<S, C, R: Random> HasRandom for Services<S, C, R> {
    type Random = R;

    fn random(&self) -> &R {
        &self.random
    }
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
