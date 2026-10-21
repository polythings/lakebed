use uuid::Builder;

use crate::{Clock, Random, Rng};

/// A random UUID (version 4) from `random`.
pub fn uuid_v4(random: &impl Random) -> String {
    let mut bytes = [0u8; 16];
    random.rng().fill_bytes(&mut bytes);
    Builder::from_random_bytes(bytes).into_uuid().to_string()
}

/// A time-ordered UUID (version 7) from `clock` and `random`: names made with it sort by creation
/// time.
pub fn uuid_v7(clock: &impl Clock, random: &impl Random) -> String {
    let millis = u64::try_from(clock.now_ms()).unwrap_or(0);
    let mut bytes = [0u8; 10];
    random.rng().fill_bytes(&mut bytes);
    Builder::from_unix_timestamp_millis(millis, &bytes).into_uuid().to_string()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{SeededRandom, TestClock};

    #[test]
    fn uuids_are_well_formed() {
        let (clock, random) = (TestClock::at_ms(1_767_225_600_000), SeededRandom::new(0));
        let v4 = uuid_v4(&random);
        let v7 = uuid_v7(&clock, &random);
        for (u, version) in [(&v4, '4'), (&v7, '7')] {
            assert_eq!(u.len(), 36);
            assert_eq!(u.chars().nth(14), Some(version));
            assert!(matches!(u.chars().nth(19), Some('8' | '9' | 'a' | 'b')));
        }
        // Version 7 starts with the time, so later ones sort later.
        clock.advance(Duration::from_millis(1));
        assert!(uuid_v7(&clock, &random) > v7);
    }
}
