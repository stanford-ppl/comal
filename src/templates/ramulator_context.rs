use std::collections::HashMap;

use dam::channel::PeekResult;
use dam::context_tools::*;
use derive_more::Constructor;
use num::BigUint;

use crate::templates::access::Access;
use crate::templates::request_manager::RequestManager;

use super::access::{AccessLike, MemoryData, SimpleRead, SimpleWrite};
use super::address::ByteAddress;

static ADDR_OFFSET: u64 = 4;

type Recv<'a, T> = dyn dam::channel::adapters::RecvAdapter<T> + Sync + Send + 'a;
type Snd<'a, T> = dyn dam::channel::adapters::SendAdapter<T> + Sync + Send + 'a;

#[derive(Clone, Debug)]
pub struct CoordPayload {
    pub seg_base: u64,
    pub crd_base: u64,
    // Seg size provides the address offset for accessing the coords
    // In the case of a dense tensor, we could set it to 0
    pub seg_size: usize,
}

#[derive(Clone, Debug)]
pub enum Payload {
    Coord(CoordPayload),
    Value(u64),
}

pub struct Memory {
    // TODO: Might need mapping from address to logical index
    // TODO: Might need mapping from context id to base addr?
    storage: HashMap<u64, MemoryData>,
    // Change to enum
    // seg_crd_pairs: HashMap<usize, CoordPayload>,
    // value_arrays: HashMap<usize, u64>,
    payload: HashMap<usize, Payload>,
    // Store mapping from context id to base iddr
    id_to_base_addr: HashMap<usize, u64>,
    current_base: u64,
}

/// A WriteBundle consists of:
///   1. A Data channel, which contains things that can be decomposed into 'chunks' of size T.
///   2. An address channel containing the base address (in bytes)
///   3. An acknowledgement channel which is written at the end of the access
#[derive(Constructor)]
pub struct WriteBundle<'a> {
    pub data: Box<Recv<'a, MemoryData>>,
    pub addr: Box<Recv<'a, u64>>,
    pub ack: Box<Snd<'a, bool>>,
    // Probably don't care to actually store the data to write somewhere
}

/// A ReadBundle consists of:
///   1. An address channel containing the base address (in bytes)
///   2. A size channel containing the read size in bytes
///   3. A response channel containing the data read out.
#[derive(Constructor)]
pub struct ReadBundle<'a> {
    pub addr: Box<Recv<'a, u64>>,
    // size: Box<Recv<'a, u64>>,
    pub resp: Box<Snd<'a, MemoryData>>,
    pub resp_addr: Box<Snd<'a, u64>>,
}

#[context_macro]
pub struct RamulatorContext<'a> {
    ramulator: ramulator_wrapper::RamulatorWrapper,
    datastore: Memory,
    writers: Vec<WriteBundle<'a>>,
    readers: Vec<ReadBundle<'a>>,
    // Elapsed cycles is measured w.r.t. the memory clock, which isn't necessarily the same as the global 'tick'
    cycles_per_tick: (num_bigint::BigUint, num_bigint::BigUint),
    elapsed_cycles: num_bigint::BigUint,
}

impl Context for RamulatorContext<'_> {
    fn run(&mut self) {
        // Might need a backlog similar to dam-ramulator?
        // let mut request_manager = RequestManager::default();
        let mut request_manager = RequestManager::default();

        while self.continue_running(&request_manager) {
            // std::print!("Inside");
            while self.ramulator.ret_available() {
                let resp_loc = ByteAddress(self.ramulator.pop());

                // std::println!("{:?}", resp_loc);

                let read = request_manager.register_recv(resp_loc);

                // Handle read request
                let data = self.read_data(resp_loc.into()).clone();
                self.readers[read.bundle_index()]
                    .resp
                    .enqueue(
                        &self.time,
                        ChannelElement {
                            time: self.time.tick() + 1,
                            data,
                        },
                    )
                    .unwrap();
                self.readers[read.bundle_index()]
                    .resp_addr
                    .enqueue(
                        &self.time,
                        ChannelElement {
                            time: self.time.tick() + 1,
                            data: resp_loc.0,
                        },
                    )
                    .unwrap();
                break;
            }

            // println!("Reader size: {}", self.readers.len());
            // println!("Writer size: {}", self.writers.len());

            self.update_ticks();
            self.update_read_requests(&mut request_manager);
            self.update_write_requests(&mut request_manager);

            self.time.incr_cycles(1);
            self.update_ticks();
        }

        // Need select to pick next thing to process

        // Process appropriately as read or write to return time and data
    }
}

impl<'a> RamulatorContext<'a> {
    pub fn new<A, B>(
        ramulator: ramulator_wrapper::RamulatorWrapper,
        cycles_per_tick: (A, B),
        datastore: Memory,
    ) -> Self
    where
        A: Into<BigUint>,
        B: Into<BigUint>,
    {
        Self {
            ramulator,
            datastore,
            writers: vec![],
            readers: vec![],
            cycles_per_tick: (cycles_per_tick.0.into(), cycles_per_tick.1.into()),
            elapsed_cycles: 0u32.into(),
            context_info: Default::default(),
        }
    }

    pub fn add_reader(
        &mut self,
        ReadBundle {
            addr,
            resp,
            resp_addr,
        }: ReadBundle<'a>,
    ) {
        addr.attach_receiver(self);
        resp.attach_sender(self);
        resp_addr.attach_sender(self);
        self.readers.push(ReadBundle {
            addr,
            resp,
            resp_addr,
        });
    }

    pub fn add_writer(&mut self, WriteBundle { data, addr, ack }: WriteBundle<'a>) {
        data.attach_receiver(self);
        addr.attach_receiver(self);
        ack.attach_sender(self);
        self.writers.push(WriteBundle { data, addr, ack })
    }

    // Requires a special handler for converting global "tick" count to local cycle count.
    fn update_ticks(&mut self) {
        let cur_ticks = self.time.tick().time();
        let expected_cycles = (cur_ticks * &self.cycles_per_tick.0) / &self.cycles_per_tick.1;
        while self.elapsed_cycles < expected_cycles {
            self.elapsed_cycles += 1u32;
            self.ramulator.cycle();
        }
    }

    fn update_write_requests(&mut self, request_manager: &mut RequestManager) {
        let cur_time = self.time.tick();
        let mut accesses = vec![];
        for (ind, writer) in self.writers.iter().enumerate() {
            match (writer.addr.peek(), writer.data.peek()) {
                (
                    PeekResult::Something(ChannelElement {
                        time: addr_time,
                        data: addr_data,
                    }),
                    PeekResult::Something(ChannelElement {
                        time: data_time,
                        data,
                    }),
                ) => {
                    if addr_time <= cur_time && data_time <= cur_time {
                        // Pop the peeked values
                        writer.addr.dequeue(&self.time).unwrap();
                        writer.data.dequeue(&self.time).unwrap();
                        let base_address = addr_data.try_into().unwrap();
                        let access = SimpleWrite::new(base_address, data, ind).into();
                        accesses.push(access);
                        // let data_size_in_bytes = data.dam_size() as u64 / 8;
                    }
                }
                _ => {}
            }
        }

        for access in accesses {
            self.enqueue_payload(access, request_manager)
        }
    }

    fn update_read_requests(&mut self, request_manager: &mut RequestManager) {
        let cur_time = self.time.tick();
        let mut accesses: Vec<Access> = vec![];

        // Iterate over readers list and service the earliest request based on time address was sent
        loop {
            let mut earliest_time = None;
            let mut earliest_index = None;

            // Select the earliest arriving request
            for (ind, reader) in self.readers.iter().enumerate() {
                if let PeekResult::Something(ChannelElement { time, .. }) = reader.addr.peek() {
                    if time <= cur_time {
                        match earliest_time {
                            Some(t) if time < t => {
                                earliest_time = Some(time);
                                earliest_index = Some(ind);
                            }
                            None => {
                                earliest_time = Some(time);
                                earliest_index = Some(ind);
                            }
                            _ => {}
                        }
                    }
                }
            }

            // If no valid request is found, break the loop
            if earliest_index.is_none() {
                break;
            }

            // Process the reader with the earliest request
            let ind = earliest_index.unwrap();
            let reader = &self.readers[ind];

            if let PeekResult::Something(ChannelElement { time, data: addr }) = reader.addr.peek() {
                if time <= cur_time {
                    // Pop the peeked value and create a new access request
                    reader.addr.dequeue(&self.time).unwrap();
                    let access = SimpleRead::new(ByteAddress(addr), ind).into();
                    accesses.push(access);
                }
            }
        }

        // for (ind, reader) in self.readers.iter().enumerate() {
        //     match reader.addr.peek() {
        //         PeekResult::Something(ChannelElement { time, data: addr }) => {
        //             if time <= cur_time {
        //                 {
        //                     // Pop the peeked values
        //                     reader.addr.dequeue(&self.time).unwrap();
        //                     // reader.size.dequeue(&self.time).unwrap();
        //                     let access = SimpleRead::new(ByteAddress(addr), ind).into();
        //                     accesses.push(access);
        //                     // self.enqueue_or_backlog(access, request_manager, backlog);
        //                 }
        //             }
        //         }
        //         _ => {}
        //     }
        // }
        for access in accesses {
            self.enqueue_payload(access, request_manager)
        }
    }

    fn enqueue_payload(&mut self, access: Access, manager: &mut RequestManager) {
        if self
            .ramulator
            .available(access.get_addr().into(), access.is_write())
        {
            self.ramulator
                .send(access.get_addr().into(), access.is_write());
            match access {
                Access::SimpleRead(rd) => manager.add_request(rd.into()),
                Access::SimpleWrite(write) => {
                    let index = write.bundle_index();
                    let data = write.payload;

                    self.write_data(write.get_addr().into(), data);
                    self.writers[index]
                        .ack
                        .enqueue(
                            &self.time,
                            ChannelElement {
                                time: self.time.tick() + 1,
                                data: true,
                            },
                        )
                        .unwrap();
                }
            }
        }
    }

    fn read_data(&self, addr: u64) -> MemoryData {
        match self.datastore.read(addr) {
            Some(data) => data.clone(),
            None => panic!("Expected value in memory"),
        }
    }

    fn write_data(&mut self, _addr: u64, _data: MemoryData) {
        // TODO: Might need to be changed to actually write back
        // Memory writes are only acknowledged but not stored
    }

    fn continue_running(&mut self, request_manager: &RequestManager) -> bool {
        // TODO: Might not need anymore
        // if !backlog.is_empty() {
        // return true;
        // }

        if !request_manager.is_empty() {
            return true;
        }

        // check all of the writers
        let mut writers_done =
            self.writers
                .iter()
                .all(
                    |WriteBundle { data, addr, ack: _ }| match (data.peek(), addr.peek()) {
                        (PeekResult::Closed, _) | (_, PeekResult::Closed) => true,
                        _ => false,
                    },
                );

        if self.writers.is_empty() {
            writers_done = true;
        }

        if !writers_done {
            return true;
        }

        let readers_done = self.readers.iter().all(
            |ReadBundle {
                 addr,
                 resp: _,
                 resp_addr: _,
             }| {
                match addr.peek() {
                    PeekResult::Closed => true,
                    _ => false,
                }
            },
        );

        if !readers_done {
            // println!("Readers Nonempty");
            return true;
        }

        false
    }

    pub fn add_seg_crd_pair(&mut self, context_id: usize, seg: Vec<u32>, crd: Vec<u32>) {
        // Map id to current base to help with addr calculation
        self.datastore
            .id_to_base_addr
            .insert(context_id, self.datastore.current_base);
        self.datastore.allocate_seg_crd_pair(context_id, seg, crd);
    }

    pub fn add_value_array(&mut self, context_id: usize, values: Vec<f32>) {
        // Map id to current base to help with addr calculation
        self.datastore
            .id_to_base_addr
            .insert(context_id, self.datastore.current_base);
        self.datastore.allocate_values(context_id, values);
    }

    pub fn get_seg_addr(&mut self, context_id: usize, seg_idx: usize) -> u64 {
        self.datastore
            .id_to_base_addr
            .get(&context_id)
            .unwrap()
            .clone()
            + seg_idx as u64 * ADDR_OFFSET
    }

    pub fn get_crd_addr(&mut self, context_id: usize, crd_idx: usize) -> u64 {
        let seg_offset = self.datastore.get_seg_crd_pair(context_id).seg_size;
        self.datastore
            .id_to_base_addr
            .get(&context_id)
            .unwrap()
            .clone()
            + (seg_offset as u64) * ADDR_OFFSET
            + (crd_idx as u64) * ADDR_OFFSET
    }

    pub fn get_base_addr(&mut self, context_id: usize) -> u64 {
        self.datastore
            .id_to_base_addr
            .get(&context_id)
            .unwrap()
            .clone()
    }

    pub fn set_next_addr(&mut self) -> u64 {
        self.datastore.current_base
    }
}

pub fn get_seg_addr(base_addr: u64, seg_idx: usize) -> u64 {
    base_addr + seg_idx as u64 * ADDR_OFFSET
}

pub fn get_crd_addr(base_addr: u64, crd_idx: usize, seg_offset: usize) -> u64 {
    base_addr + (seg_offset as u64) * ADDR_OFFSET + (crd_idx as u64) * ADDR_OFFSET
}

pub fn get_val_addr(base_addr: u64, val_idx: usize) -> u64 {
    base_addr + (val_idx as u64) * ADDR_OFFSET
}

impl Memory {
    pub fn new() -> Self {
        Memory {
            storage: HashMap::new(),
            payload: HashMap::new(),
            id_to_base_addr: HashMap::new(),
            current_base: 0x0000_0000,
        }
    }

    // TODO: Using context id to map storage for now
    pub fn allocate_seg_crd_pair(
        &mut self,
        context_id: usize,
        seg_arr: Vec<u32>,
        crd_arr: Vec<u32>,
    ) {
        // Storing seg and coord arrays as pairs
        let seg_base = self.current_base;
        let crd_base = seg_base + (seg_arr.len() as u64 * ADDR_OFFSET);

        for (i, &val) in seg_arr.iter().enumerate() {
            self.storage
                .insert(seg_base + (i as u64 * ADDR_OFFSET), MemoryData::U32(val));
        }
        for (i, &val) in crd_arr.iter().enumerate() {
            self.storage
                .insert(crd_base + (i as u64 * ADDR_OFFSET), MemoryData::U32(val));
        }

        self.payload.insert(
            context_id,
            Payload::Coord(CoordPayload {
                seg_base,
                crd_base,
                seg_size: seg_arr.len(),
            }),
        );

        self.current_base = crd_base + (crd_arr.len() as u64 * ADDR_OFFSET);
    }

    pub fn allocate_values(&mut self, context_id: usize, value_arr: Vec<f32>) {
        let base_address = self.current_base;

        for (i, &val) in value_arr.iter().enumerate() {
            self.storage.insert(
                base_address + (i as u64 * ADDR_OFFSET),
                MemoryData::F32(val),
            );
        }

        self.payload
            .insert(context_id, Payload::Value(base_address));
        self.current_base = base_address + (value_arr.len() as u64 * ADDR_OFFSET);
    }

    pub fn read(&self, address: u64) -> Option<&MemoryData> {
        self.storage.get(&address)
    }

    pub fn get_seg_crd_pair(&self, context_id: usize) -> CoordPayload {
        match self.payload.get(&context_id) {
            Some(payload) => match payload {
                Payload::Coord(coord_payload) => coord_payload.clone(),
                Payload::Value(_) => panic!("Expected coord payload but got value"),
            },
            None => panic!("Expected stored coordinate but got none"),
        }
    }

    pub fn get_value_base(&self, context_id: usize) -> u64 {
        match self.payload.get(&context_id) {
            Some(payload) => match payload {
                Payload::Coord(_) => todo!(),
                Payload::Value(val_payload) => val_payload.clone(),
            },
            None => todo!(),
        }
    }
}

#[cfg(test)]
mod test {
    use dam::context_tools::*;
    use dam::simulation::{InitializationOptions, ProgramBuilder, RunOptions};
    use dam::utility_contexts::*;
    use ramulator_wrapper::RamulatorWrapper;

    use crate::templates::access::MemoryData;
    use crate::templates::ramulator_context::{Memory, ReadBundle, WriteBundle};
    use crate::templates::ramulator_context::{RamulatorContext, ADDR_OFFSET};
    #[test]
    fn ramulator_e2e_small() {
        const MEM_SIZE: usize = 32;

        let mut parent = ProgramBuilder::default();
        let ramulator =
            RamulatorWrapper::new_with_preset(ramulator_wrapper::PresetConfigs::HBM, "test.txt");

        // let ramulator = RamulatorWrapper::new("/home/rubensl/comal/src/templates/configs/DDR4-config.cfg", "test.txt");

        let seg: Vec<u32> = vec![0, MEM_SIZE as u32];
        let crd: Vec<u32> = Vec::from_iter(0..MEM_SIZE as u32);

        let mut mem_context = RamulatorContext::new(ramulator, (1u32, 1u32), Memory::new());

        mem_context.add_seg_crd_pair(0, seg, crd);

        let (addr_snd, addr_rcv) = parent.unbounded();
        let (data_snd, data_rcv) = parent.unbounded::<MemoryData>();
        let (ack_snd, ack_rcv) = parent.unbounded::<bool>();
        let addrs = || (0..(MEM_SIZE as u64)).map(|x| x * ADDR_OFFSET);
        parent.add_child(GeneratorContext::new(addrs, addr_snd));
        parent.add_child(GeneratorContext::new(
            || (0..(MEM_SIZE as u32)).map(|x| MemoryData::U32(x)),
            data_snd,
        ));

        mem_context.add_writer(WriteBundle {
            data: Box::new(data_rcv),
            addr: Box::new(addr_rcv),
            ack: Box::new(ack_snd),
        });

        let (raddr_snd, raddr_rcv) = parent.unbounded();
        let (rdata_snd, rdata_rcv) = parent.unbounded::<MemoryData>();
        // let (size_snd, size_rcv) = parent.unbounded();

        let mut read_ctx = FunctionContext::new();
        raddr_snd.attach_sender(&read_ctx);
        // size_snd.attach_sender(&read_ctx);
        ack_rcv.attach_receiver(&read_ctx);
        read_ctx.set_run(move |time| {
            for iter in 0..MEM_SIZE {
                // Wait for an ack to be received
                ack_rcv.dequeue(time).unwrap();

                raddr_snd
                    .enqueue(
                        time,
                        ChannelElement {
                            time: time.tick() + 1,
                            data: ADDR_OFFSET * (iter as u64),
                        },
                    )
                    .unwrap();
                // size_snd
                // .enqueue(
                // time,
                // ChannelElement {
                // time: time.tick() + 1,
                // Going to be super inefficient and only use 8 bytes (64 bits) instead of the full 64 byte access
                // data: 8u64,
                // },
                // )
                // .unwrap();
            }
        });
        parent.add_child(read_ctx);

        let (resp_addr_snd, resp_addr_rcv) = parent.unbounded::<u64>();
        mem_context.add_reader(ReadBundle {
            addr: Box::new(raddr_rcv),
            resp: Box::new(rdata_snd),
            resp_addr: Box::new(resp_addr_snd),
        });

        parent.add_child(mem_context);
        let mut verif_context = FunctionContext::new();
        resp_addr_rcv.attach_receiver(&verif_context);
        rdata_rcv.attach_receiver(&verif_context);
        verif_context.set_run(move |time| {
            let mut received: fxhash::FxHashMap<u64, MemoryData> = Default::default();
            for _ in 0..MEM_SIZE {
                let addr = resp_addr_rcv.dequeue(time).unwrap().data;
                let data = rdata_rcv.dequeue(time).unwrap().data;
                received.insert(addr, data);
                time.incr_cycles(1);
            }
            println!("Received: {:?}", received);
        });

        parent.add_child(verif_context);

        println!("Finished building");

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());

        println!("Elapsed: {:?}", executed.elapsed_cycles());
    }
}
