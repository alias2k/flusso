#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct ContentHash(u32);

impl ContentHash {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        let mut hash: u32 = 2_166_136_261;
        for byte in data {
            hash ^= *byte as u32;
            hash = hash.wrapping_mul(16_777_619);
        }
        Self(hash)
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08x}", self.0)
    }
}
