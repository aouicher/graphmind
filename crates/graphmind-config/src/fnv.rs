//! FNV-1a, for fingerprints and file names that must stay the same across builds and Rust
//! versions (`DefaultHasher` doesn't promise that).

/// 64-bit FNV-1a of `bytes`. Hashing two byte sequences one after the other gives the hash of
/// their concatenation.
pub fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published FNV-1a test vectors: stamps and fingerprints written by earlier versions
    /// keep matching.
    #[test]
    fn matches_the_reference_values() {
        assert_eq!(fnv1a(*b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(*b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(*b"foobar"), 0x8594_4171_f739_67e8);
    }
}
