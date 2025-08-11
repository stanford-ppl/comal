#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct ChunkAddress(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Hash)]
pub struct ByteAddress(pub u64);

impl ByteAddress {
    pub fn to_chunk(&self, bytes_per_chunk: u64) -> ChunkAddress {
        ChunkAddress(self.0 / bytes_per_chunk)
    }
}

impl From<u64> for ByteAddress {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<u64> for ChunkAddress {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl std::ops::Add<u64> for ByteAddress {
    type Output = ByteAddress;

    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl std::ops::Add<u64> for ChunkAddress {
    type Output = ChunkAddress;

    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl From<ChunkAddress> for u64 {
    fn from(value: ChunkAddress) -> Self {
        value.0
    }
}

impl From<ByteAddress> for u64 {
    fn from(value: ByteAddress) -> Self {
        value.0
    }
}
