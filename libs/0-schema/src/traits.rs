pub trait ParseFrom<T>: Sized {
    type Error;

    fn try_parse(value: T) -> Result<Self, Self::Error>;
}
