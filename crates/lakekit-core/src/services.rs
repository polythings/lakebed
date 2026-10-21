use std::fmt;

/// The store, clock, and random source an operation such as a commit uses, each borrowed from
/// wherever the caller keeps it: `Services::new(&s3, &TokioClock, &OsRandom)`. One object that is
/// all three (a JavaScript host) goes in all three places: `Services::new(&host, &host, &host)`.
pub struct Services<'a, S, C, R> {
    /// Where objects are read and written.
    pub store: &'a S,
    /// The time, and waiting between attempts.
    pub clock: &'a C,
    /// Random numbers: snapshot ids, file names, and jitter.
    pub random: &'a R,
}

impl<'a, S, C, R> Services<'a, S, C, R> {
    /// `store`, `clock`, and `random`.
    pub fn new(store: &'a S, clock: &'a C, random: &'a R) -> Self {
        Self { store, clock, random }
    }
}

// By hand, so `Services` is `Copy` whatever the services are (a derive would ask them to be).
impl<S, C, R> Clone for Services<'_, S, C, R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, C, R> Copy for Services<'_, S, C, R> {}

impl<S, C, R> fmt::Debug for Services<'_, S, C, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Services").finish_non_exhaustive()
    }
}
