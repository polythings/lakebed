use std::fmt::Write as _;

use crate::Host;

/// A random UUID (version 4) from `host`.
pub fn uuid_v4(host: &impl Host) -> String {
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&host.random().to_be_bytes());
    b[8..].copy_from_slice(&host.random().to_be_bytes());
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format_uuid(&b)
}

/// A time-ordered UUID (version 7) from `host`'s clock and randomness: names made with it sort by
/// creation time.
pub fn uuid_v7(host: &impl Host) -> String {
    let mut b = [0u8; 16];
    let ms = u64::try_from(host.now_ms()).unwrap_or(0);
    b[..6].copy_from_slice(&ms.to_be_bytes()[2..]);
    b[6..14].copy_from_slice(&host.random().to_be_bytes());
    b[14..].copy_from_slice(&host.random().to_be_bytes()[..2]);
    b[6] = (b[6] & 0x0f) | 0x70;
    b[8] = (b[8] & 0x3f) | 0x80;
    format_uuid(&b)
}

fn format_uuid(b: &[u8; 16]) -> String {
    let hex = b.iter().fold(String::with_capacity(32), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    });
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryHost;

    #[test]
    fn uuids_are_well_formed() {
        let host = MemoryHost::new(1_767_225_600_000);
        let v4 = uuid_v4(&host);
        let v7 = uuid_v7(&host);
        for (u, version) in [(&v4, '4'), (&v7, '7')] {
            assert_eq!(u.len(), 36);
            assert_eq!(u.chars().nth(14), Some(version));
            assert!(matches!(u.chars().nth(19), Some('8' | '9' | 'a' | 'b')));
        }
        // Version 7 starts with the time, so later ones sort later.
        host.advance(1);
        assert!(uuid_v7(&host) > v7);
    }
}
