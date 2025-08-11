use super::address::ByteAddress;
use dam::types::DAMType;
use derive_more::Constructor;
use enum_dispatch::enum_dispatch;
use half::f16;

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub enum MemoryData {
    // U32([u32; 16]),  // 16 elements × 4 bytes = 64 bytes
    // F32([f32; 16]),  // 16 elements × 4 bytes = 64 bytes
    F16([f16; 32]), // 32 elements × 2 bytes = 64 bytes
}

impl DAMType for MemoryData {
    fn dam_size(&self) -> usize {
        64
    }
}

impl Default for MemoryData {
    fn default() -> Self {
        MemoryData::F16([f16::from_f32(0.0); 32])
    }
}

#[enum_dispatch]
pub trait AccessLike {
    fn mark_resolved(&mut self) {}
    fn get_addr(&self) -> ByteAddress;
    fn is_write(&self) -> bool;

    fn bundle_index(&self) -> usize;
    fn num_chunks(&self) -> u64;
}

#[enum_dispatch(AccessLike)]
#[derive(Clone, Debug)]
pub enum Access {
    SimpleRead(SimpleRead),
    SimpleWrite(SimpleWrite),
}

#[enum_dispatch(AccessLike)]
#[derive(Clone, Debug)]
pub enum Read {
    SimpleRead,
}

#[derive(Clone, Copy, Constructor, Debug)]
pub struct SimpleRead {
    base: ByteAddress,
    bundle_index: usize,
}

impl AccessLike for SimpleRead {
    fn get_addr(&self) -> ByteAddress {
        self.base
    }

    fn is_write(&self) -> bool {
        false
    }

    fn bundle_index(&self) -> usize {
        self.bundle_index
    }

    fn num_chunks(&self) -> u64 {
        1
    }
}

#[derive(Clone, Constructor, Debug)]
pub struct SimpleWrite {
    base: ByteAddress,
    pub payload: MemoryData,
    bundle_index: usize,
}

impl AccessLike for SimpleWrite {
    fn get_addr(&self) -> ByteAddress {
        self.base
    }

    fn is_write(&self) -> bool {
        true
    }

    fn bundle_index(&self) -> usize {
        self.bundle_index
    }

    fn num_chunks(&self) -> u64 {
        1
    }
}
