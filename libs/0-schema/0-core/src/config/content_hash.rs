use std::hash::{Hash, Hasher};

use crate::traits::ContentHasher;

/// FNV-1a offset basis.
const FNV_OFFSET: u32 = 2_166_136_261;
/// FNV-1a prime.
const FNV_PRIME: u32 = 16_777_619;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct ContentHash(u32);

impl ContentHash {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        let mut hasher = Fnv1aHasher::default();
        hasher.write(data);
        Self(hasher.0)
    }

    /// The content hash of any [`Hash`] value, via FNV-1a. Deterministic for a
    /// given structure — the same parsed value always hashes the same — which
    /// is what makes it usable as a stable, structure-derived identifier.
    pub fn of<T: Hash>(value: &T) -> Self {
        Fnv1aContentHasher.hash(value)
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08x}", self.0)
    }
}

/// An FNV-1a [`Hasher`], so any [`Hash`] value yields a stable [`ContentHash`]
/// independent of the platform's default (randomized) hasher.
struct Fnv1aHasher(u32);

impl Default for Fnv1aHasher {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl Hasher for Fnv1aHasher {
    fn finish(&self) -> u64 {
        u64::from(self.0)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u32::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }
}

/// Hashes values with FNV-1a (see [`ContentHash::of`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct Fnv1aContentHasher;

impl ContentHasher for Fnv1aContentHasher {
    fn hash<T: Hash>(&self, data: &T) -> ContentHash {
        let mut hasher = Fnv1aHasher::default();
        data.hash(&mut hasher);
        ContentHash::new(hasher.0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_for_equal_values() {
        let a = ContentHash::of(&("users", 1u8, vec!["id", "email"]));
        let b = ContentHash::of(&("users", 1u8, vec!["id", "email"]));
        assert_eq!(a, b);
    }

    #[test]
    fn hash_changes_when_structure_changes() {
        let before = ContentHash::of(&vec!["id", "email"]);
        let after = ContentHash::of(&vec!["id", "email", "name"]);
        assert_ne!(before, after);
    }

    #[test]
    fn display_is_eight_hex_digits() {
        assert_eq!(format!("{}", ContentHash::new(0xABCD)), "0000abcd");
    }
}
