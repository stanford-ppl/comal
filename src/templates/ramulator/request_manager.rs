use std::collections::VecDeque;

use super::{
    access::{AccessLike, SimpleRead},
    address::ByteAddress,
};

#[derive(Default, Debug, Clone)]
pub struct RequestManager {
    addr_to_access_map: fxhash::FxHashMap<ByteAddress, VecDeque<SimpleRead>>,
}

impl RequestManager {
    pub fn add_request(&mut self, access: SimpleRead) {
        self.addr_to_access_map
            .entry(access.get_addr())
            .or_default()
            .push_back(access);
    }

    /// Possibly returns an access if it is ready.
    pub fn register_recv(&mut self, addr: ByteAddress) -> SimpleRead {
        match self.addr_to_access_map.get_mut(&addr) {
            Some(queue) if !queue.is_empty() => {
                let mut acc = queue.pop_front().unwrap();
                acc.mark_resolved();
                acc
            }
            _ => {
                panic!("We didn't have a request registered at this address!");
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.addr_to_access_map
            .values()
            .all(|queue| queue.is_empty())
    }
}
