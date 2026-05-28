pub trait ParseFrom<T>: Sized {
    type Error;

    fn try_parse(value: T) -> Result<Self, Self::Error>;
}

pub trait ContentHasher {
    fn hash<T: std::hash::Hash>(&self, data: &T) -> crate::ContentHash;
}
