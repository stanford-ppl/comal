use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use dam::{context_tools::*, dam_macros::context_macro, structures::Time};

use super::memory_logger::MemoryWrapper;
use super::primitive::{AccessBundle, AccessType};
use super::{primitive::Token, utils::write_outputs};

#[context_macro]
pub struct CompressedWrScan<ValType: Clone, StopType: Clone> {
    pub input: Receiver<Token<ValType, StopType>>,
    pub seg_arr: Arc<Mutex<Vec<ValType>>>,
    pub crd_arr: Arc<Mutex<Vec<ValType>>>,
    pub dump_chan: Sender<MemoryWrapper>,
    pub base_addr: u64,
    pub wr_latency: u64,
    pub log_memory: bool,
    pub access_map: BTreeMap<Time, AccessBundle>,
}

impl<ValType: DAMType, StopType: DAMType> CompressedWrScan<ValType, StopType>
where
    CompressedWrScan<ValType, StopType>: Context,
{
    pub fn new(
        input: Receiver<Token<ValType, StopType>>,
        dump_chan: Sender<MemoryWrapper>,
    ) -> Self {
        let cwr = CompressedWrScan {
            input,
            seg_arr: Default::default(),
            crd_arr: Default::default(),
            dump_chan,
            base_addr: 0,
            context_info: Default::default(),
            access_map: BTreeMap::new(),
            log_memory: false,
            wr_latency: 1,
        };
        (cwr).input.attach_receiver(&cwr);
        (cwr).dump_chan.attach_sender(&cwr);

        cwr
    }

    pub fn set_base_addr(&mut self, base_addr: u64) {
        self.base_addr = base_addr;
    }

    pub fn set_wr_latency(&mut self, latency: u64) {
        self.wr_latency = latency;
    }

    pub fn log_mem(&mut self, log: bool) {
        self.log_memory = log;
    }
}

impl<ValType, StopType> Context for CompressedWrScan<ValType, StopType>
where
    ValType: DAMType
        + std::ops::AddAssign<u32>
        + std::ops::AddAssign<ValType>
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {
        // default is 0
        self.seg_arr.lock().unwrap().push(ValType::default());
    }

    fn run(&mut self) {
        let mut curr_crd_cnt: ValType = ValType::default();
        let mut end_fiber = false;
        let initiation_interval = 1;
        let mut crd_write_count: u64 = 0;
        let mut seg_write_count: u64 = 0;

        let addr_offset = 4;

        let mut crd_arr = self.crd_arr.lock().unwrap();
        let mut seg_arr = self.seg_arr.lock().unwrap();
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        crd_arr.push(val.clone());
                        if self.log_memory {
                            self.access_map.insert(
                                self.time.tick(),
                                AccessBundle {
                                    addr: self.base_addr,
                                    access_type: AccessType::Write,
                                },
                            );

                            self.base_addr += addr_offset;
                        }
                        curr_crd_cnt += 1;
                        end_fiber = false;
                        // println!("{:?}", val.clone());
                        crd_write_count += 1;
                        self.time.incr_cycles(self.wr_latency);
                    }
                    Token::Stop(_) if !end_fiber => {
                        seg_arr.push(curr_crd_cnt.clone());
                        end_fiber = true;
                        seg_write_count += 1;
                    }
                    Token::Empty | Token::Stop(_) => {
                        // TODO: Maybe needs to be processed too

                        continue;
                    }
                    Token::Done => {
                        println!("Crd write count (crd): {}", crd_write_count);
                        println!("Crd write count (seg): {}", seg_write_count);
                        if self.log_memory {
                            let data = MemoryWrapper {
                                map: std::mem::take(&mut self.access_map),
                            };
                            self.dump_chan
                                .enqueue(&self.time, ChannelElement::new(self.time.tick(), data))
                                .unwrap();
                        }
                        return;
                    }
                },
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            self.time.incr_cycles(initiation_interval);
        }
    }
}

#[context_macro]
pub struct ValsWrScan<ValType: Clone, StopType: Clone> {
    pub input: Receiver<Token<ValType, StopType>>,
    pub out_val: Arc<Mutex<Vec<ValType>>>,
    pub wr_latency: u64,
    pub log_memory: bool,
    pub dump_chan: Sender<MemoryWrapper>,
    pub access_map: BTreeMap<Time, AccessBundle>,
    pub base_addr: u64,
}

impl<ValType: DAMType, StopType: DAMType> ValsWrScan<ValType, StopType>
where
    ValsWrScan<ValType, StopType>: Context,
{
    pub fn new(
        input: Receiver<Token<ValType, StopType>>,
        dump_chan: Sender<MemoryWrapper>,
    ) -> Self {
        let vals = ValsWrScan {
            input,
            out_val: Default::default(),
            wr_latency: 1,
            context_info: Default::default(),
            log_memory: false,
            dump_chan,
            access_map: BTreeMap::new(),
            base_addr: 0,
        };
        (vals.input).attach_receiver(&vals);
        (vals).dump_chan.attach_sender(&vals);

        vals
    }

    pub fn set_wr_latency(&mut self, latency: u64) {
        self.wr_latency = latency;
    }

    pub fn set_base_addr(&mut self, base_addr: u64) {
        self.base_addr = base_addr;
    }

    pub fn log_mem(&mut self, log: bool) {
        self.log_memory = log;
    }
}

impl<ValType, StopType> Context for ValsWrScan<ValType, StopType>
where
    ValType: DAMType + std::fmt::Display,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let latency = 1;
        let initiation_interval = 1;
        let addr_offset = 4;
        let mut locked = self.out_val.lock().unwrap();
        let mut write_count: u64 = 0;
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        // println!("Value: {:?}", Token::<ValType, StopType>::Val(val.clone()));
                        locked.push(val.clone());
                        if self.log_memory {
                            self.access_map.insert(
                                self.time.tick(),
                                AccessBundle {
                                    addr: self.base_addr,
                                    access_type: AccessType::Write,
                                },
                            );
                            self.base_addr += addr_offset;
                        }
                        // println!("{:?}", val.clone());
                        // self.time.incr_cycles(self.wr_latency);
                        write_count += 1;
                    }
                    Token::Empty | Token::Stop(_) => {
                        continue;
                    }
                    Token::Done => {
                        // let filename: String = "/tmp/tmp_result.txt".to_string();
                        // write_outputs(filename.into(), locked.to_vec());
                        // println!("res: {:?}", locked);
                        if self.log_memory {
                            let data = MemoryWrapper {
                                map: std::mem::take(&mut self.access_map),
                            };
                            self.dump_chan
                                .enqueue(&self.time, ChannelElement::new(self.time.tick(), data))
                                .unwrap();
                        }
                        println!("Write count: {}", write_count);
                        break;
                    }
                },
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            self.time.incr_cycles(self.wr_latency);
        }
        self.time.incr_cycles(latency);
    }
}
