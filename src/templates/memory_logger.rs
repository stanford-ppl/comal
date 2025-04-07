use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{self, BufWriter, Write};

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

pub fn dump_access_bundles<T>(btree_map: &BTreeMap<T, AccessBundle>, filename: &str) -> io::Result<()> {
    // Create a file with a buffered writer - much more efficient for many writes
    let file = File::create(filename)?;
    let mut writer = BufWriter::with_capacity(8 * 1024 * 1024, file); // 8MB buffer
    
    for (_time, access_bundle) in btree_map {
        let access_type_str = match access_bundle.access_type {
            AccessType::Read => "LD",
            AccessType::Write => "ST",
        };
        
        // Write directly to the buffered writer
        writeln!(writer, "{} 0x{:08x}", access_type_str, access_bundle.addr)?;
    }
    
    // Ensure all data is written by flushing the buffer
    writer.flush()?;
    
    Ok(())
}

impl Context for MemoryLogger {
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut final_mems: BTreeMap<Time, AccessBundle> = BTreeMap::new();
        for chan in self.scanners.iter() {
            let mut data = chan.dequeue(&self.time).unwrap().data.map;
            final_mems.append(&mut data);
        }

        // Debug print
        // for (key, value) in final_mems.iter().take(10) {
        //     println!("{}: {:?}", key, value);
        // }

        dump_access_bundles(&final_mems, "memory_log.txt").unwrap();

        self.time.incr_cycles(1);
    }
}
