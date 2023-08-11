use super::proto_headers::tortilla::{CrdStream, RefStream, RepSigStream, ValStream};

pub fn get_crd_id(stream: &Option<CrdStream>) -> u64 {
    stream.try_conv()
}

pub fn get_ref_id(stream: &Option<RefStream>) -> u64 {
    stream.try_conv()
}

pub fn get_val_id(stream: &Option<ValStream>) -> u64 {
    stream.try_conv()
}

pub fn get_repsig_id(stream: &Option<RepSigStream>) -> u64 {
    stream.try_conv()
}

pub trait AsStreamID {
    fn try_conv(&self) -> u64;
}

impl AsStreamID for Option<ValStream> {
    fn try_conv(&self) -> u64 {
        match self {
            Some(stuff) => stuff.try_conv(),
            None => 0,
        }
    }
}

impl AsStreamID for Option<CrdStream> {
    fn try_conv(&self) -> u64 {
        match self {
            Some(stuff) => stuff.try_conv(),
            None => 0,
        }
    }
}

impl AsStreamID for Option<RefStream> {
    fn try_conv(&self) -> u64 {
        match self {
            Some(stuff) => stuff.try_conv(),
            None => 0,
        }
    }
}

impl AsStreamID for Option<RepSigStream> {
    fn try_conv(&self) -> u64 {
        match self {
            Some(stuff) => stuff.try_conv(),
            None => 0,
        }
    }
}

impl AsStreamID for ValStream {
    fn try_conv(&self) -> u64 {
        self.id.as_ref().expect("Error getting id").id
    }
}

impl AsStreamID for CrdStream {
    fn try_conv(&self) -> u64 {
        self.id.as_ref().expect("Error getting id").id
    }
}

impl AsStreamID for RefStream {
    fn try_conv(&self) -> u64 {
        self.id.as_ref().expect("Error getting id").id
    }
}

impl AsStreamID for RepSigStream {
    fn try_conv(&self) -> u64 {
        self.id.as_ref().expect("Error getting id").id
    }
}
