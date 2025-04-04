use std::collections::{BTreeMap, HashMap};

use dam::structures::{Identifiable, Time};
use dam::{
    context_tools::*,
    dam_macros::{context_macro, event_type},
    structures::Identifier,
};
use serde::{Deserialize, Serialize};

use super::primitive::{AccessBundle, AccessType, Token};

#[context_macro]
pub struct MemoryLogger {
    pub scanners: Vec<Receiver<MemoryWrapper>>,
}

#[derive(Clone, Debug, Default)]
pub struct MemoryWrapper {
    pub map: BTreeMap<Time, AccessBundle>,
}

impl DAMType for MemoryWrapper {
    fn dam_size(&self) -> usize {
        100
    }
}

impl MemoryLogger {
    pub fn new() -> Self {
        let mem = MemoryLogger {
            scanners: vec![],
            context_info: Default::default(),
        };

        mem
    }

    pub fn add_scanner(&mut self, scanner: Receiver<MemoryWrapper>) {
        scanner.attach_receiver(self);
        self.scanners.push(scanner);
    }
}

impl Context for MemoryLogger {
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut final_mems: BTreeMap<Time, AccessBundle> = BTreeMap::new();
        for chan in self.scanners.iter() {
            let mut data = chan.dequeue(&self.time).unwrap().data.map;
            final_mems.append(&mut data);
        }

        for (key, value) in final_mems.iter().take(500) {
            println!("{}: {:?}", key, value);
        }
        println!("Size: {}", final_mems.len());

        self.time.incr_cycles(1);
    }
}
