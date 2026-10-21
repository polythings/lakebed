use uuid::Builder;

use crate::Env;

/// A random UUID (version 4) from `env`'s randomness.
pub fn uuid_v4(env: &impl Env) -> String {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&env.random().to_be_bytes());
    bytes[8..].copy_from_slice(&env.random().to_be_bytes());
    Builder::from_random_bytes(bytes).into_uuid().to_string()
}

/// A time-ordered UUID (version 7) from `env`'s clock and randomness: names made with it sort by
/// creation time.
pub fn uuid_v7(env: &impl Env) -> String {
    let millis = u64::try_from(env.now_ms()).unwrap_or(0);
    let mut random = [0u8; 10];
    random[..8].copy_from_slice(&env.random().to_be_bytes());
    random[8..].copy_from_slice(&env.random().to_be_bytes()[..2]);
    Builder::from_unix_timestamp_millis(millis, &random).into_uuid().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryStore;

    #[test]
    fn uuids_are_well_formed() {
        let env = MemoryStore::new(1_767_225_600_000);
        let v4 = uuid_v4(&env);
        let v7 = uuid_v7(&env);
        for (u, version) in [(&v4, '4'), (&v7, '7')] {
            assert_eq!(u.len(), 36);
            assert_eq!(u.chars().nth(14), Some(version));
            assert!(matches!(u.chars().nth(19), Some('8' | '9' | 'a' | 'b')));
        }
        // Version 7 starts with the time, so later ones sort later.
        env.advance(1);
        assert!(uuid_v7(&env) > v7);
    }
}
