use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use dam::{context_tools::*, dam_macros::context_macro};

use super::{primitive::Token, utils::write_outputs};
// HBM timing interface
use crate::templates::ramulator::hbm_context::ParAddrs;

#[context_macro]
pub struct CompressedWrScan<ValType: Clone, StopType: Clone> {
    pub input: Receiver<Token<ValType, StopType>>,
    pub seg_arr: Arc<Mutex<Vec<ValType>>>,
    pub crd_arr: Arc<Mutex<Vec<ValType>>>,
    // Optional HBM-backed write interfaces (crd/seg)
    hbm_crd_addr_snd: Option<Sender<ParAddrs>>,
    hbm_crd_resp_rcv: Option<Receiver<u64>>,
    hbm_seg_addr_snd: Option<Sender<ParAddrs>>,
    hbm_seg_resp_rcv: Option<Receiver<u64>>,
    // Address mapping config: physical address = base + index * stride
    hbm_crd_base: u64,
    hbm_seg_base: u64,
    hbm_crd_stride: u64,
    hbm_seg_stride: u64,
}

impl<ValType: DAMType, StopType: DAMType> CompressedWrScan<ValType, StopType>
where
    CompressedWrScan<ValType, StopType>: Context,
{
    pub fn new(input: Receiver<Token<ValType, StopType>>) -> Self {
        let cwr = CompressedWrScan {
            input,
            seg_arr: Default::default(),
            crd_arr: Default::default(),
            hbm_crd_addr_snd: None,
            hbm_crd_resp_rcv: None,
            hbm_seg_addr_snd: None,
            hbm_seg_resp_rcv: None,
            hbm_crd_base: 0,
            hbm_seg_base: 0,
            hbm_crd_stride: 4,
            hbm_seg_stride: 4,
            context_info: Default::default(),
        };
        (cwr).input.attach_receiver(&cwr);

        cwr
    }

    // Enable HBM-driven timing for writes to crd_arr and seg_arr
    pub fn enable_hbm_writes(
        &mut self,
        crd_addr_snd: Sender<ParAddrs>,
        crd_resp_rcv: Receiver<u64>,
        seg_addr_snd: Sender<ParAddrs>,
        seg_resp_rcv: Receiver<u64>,
        crd_base: u64,
        seg_base: u64,
        crd_stride: u64,
        seg_stride: u64,
    ) {
        crd_addr_snd.attach_sender(self);
        seg_addr_snd.attach_sender(self);
        crd_resp_rcv.attach_receiver(self);
        seg_resp_rcv.attach_receiver(self);

        self.hbm_crd_addr_snd = Some(crd_addr_snd);
        self.hbm_crd_resp_rcv = Some(crd_resp_rcv);
        self.hbm_seg_addr_snd = Some(seg_addr_snd);
        self.hbm_seg_resp_rcv = Some(seg_resp_rcv);

        self.hbm_crd_base = crd_base;
        self.hbm_seg_base = seg_base;
        self.hbm_crd_stride = crd_stride.max(1);
        self.hbm_seg_stride = seg_stride.max(1);
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

        let mut crd_arr = self.crd_arr.lock().unwrap();
        let mut seg_arr = self.seg_arr.lock().unwrap();
        let use_crd_hbm = self.hbm_crd_addr_snd.is_some() && self.hbm_crd_resp_rcv.is_some();
        let use_seg_hbm = self.hbm_seg_addr_snd.is_some() && self.hbm_seg_resp_rcv.is_some();
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        if use_crd_hbm {
                            let idx = crd_arr.len();
                            let addr = self.hbm_crd_base + (idx as u64) * self.hbm_crd_stride;
                            if let Some(snd) = &self.hbm_crd_addr_snd {
                                snd.enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        self.time.tick(),
                                        ParAddrs::new(vec![addr]),
                                    ),
                                )
                                .unwrap();
                            }
                            if let Some(rcv) = &self.hbm_crd_resp_rcv {
                                loop {
                                    match rcv.dequeue(&self.time) {
                                        Ok(_) => break,
                                        Err(_) => self.time.incr_cycles(1),
                                    }
                                }
                            }
                        }
                        crd_arr.push(val.clone());
                        curr_crd_cnt += 1;
                        end_fiber = false;
                        // println!("{:?}", val.clone());
                        crd_write_count += 1;
                    }
                    Token::Stop(_) if !end_fiber => {
                        if use_seg_hbm {
                            let idx = seg_arr.len();
                            let addr = self.hbm_seg_base + (idx as u64) * self.hbm_seg_stride;
                            if let Some(snd) = &self.hbm_seg_addr_snd {
                                snd.enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        self.time.tick(),
                                        ParAddrs::new(vec![addr]),
                                    ),
                                )
                                .unwrap();
                            }
                            if let Some(rcv) = &self.hbm_seg_resp_rcv {
                                loop {
                                    match rcv.dequeue(&self.time) {
                                        Ok(_) => break,
                                        Err(_) => self.time.incr_cycles(1),
                                    }
                                }
                            }
                        }
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
    // Optional HBM-backed write interface for values
    hbm_wr_addr_snd: Option<Sender<ParAddrs>>,
    hbm_wr_resp_rcv: Option<Receiver<u64>>,
    hbm_wr_base: u64,
    hbm_wr_stride: u64,
}

impl<ValType: DAMType, StopType: DAMType> ValsWrScan<ValType, StopType>
where
    ValsWrScan<ValType, StopType>: Context,
{
    pub fn new(input: Receiver<Token<ValType, StopType>>) -> Self {
        let vals = ValsWrScan {
            input,
            out_val: Default::default(),
            hbm_wr_addr_snd: None,
            hbm_wr_resp_rcv: None,
            hbm_wr_base: 0,
            hbm_wr_stride: 4,
            context_info: Default::default(),
        };
        (vals.input).attach_receiver(&vals);

        vals
    }

    // Enable HBM-driven timing for writes into out_val
    pub fn enable_hbm_writes(
        &mut self,
        wr_addr_snd: Sender<ParAddrs>,
        wr_resp_rcv: Receiver<u64>,
        base: u64,
        stride: u64,
    ) {
        wr_addr_snd.attach_sender(self);
        wr_resp_rcv.attach_receiver(self);
        self.hbm_wr_addr_snd = Some(wr_addr_snd);
        self.hbm_wr_resp_rcv = Some(wr_resp_rcv);
        self.hbm_wr_base = base;
        self.hbm_wr_stride = stride.max(1);
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
        let mut locked = self.out_val.lock().unwrap();
        let mut write_count: u64 = 0;
        let use_hbm = self.hbm_wr_addr_snd.is_some() && self.hbm_wr_resp_rcv.is_some();
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        if use_hbm {
                            let idx = locked.len();
                            let addr = self.hbm_wr_base + (idx as u64) * self.hbm_wr_stride;
                            if let Some(snd) = &self.hbm_wr_addr_snd {
                                snd.enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        self.time.tick(),
                                        ParAddrs::new(vec![addr]),
                                    ),
                                )
                                .unwrap();
                            }
                            if let Some(rcv) = &self.hbm_wr_resp_rcv {
                                loop {
                                    match rcv.dequeue(&self.time) {
                                        Ok(_) => break,
                                        Err(_) => self.time.incr_cycles(1),
                                    }
                                }
                            }
                        }
                        // println!("Value: {:?}", Token::<ValType, StopType>::Val(val.clone()));
                        locked.push(val.clone());
                        // println!("{:?}", val.clone());
                        write_count += 1;
                    }
                    Token::Empty | Token::Stop(_) => {
                        continue;
                    }
                    Token::Done => {
                        let filename: String = "/tmp/tmp_result.txt".to_string();
                        // write_outputs(filename.into(), locked.to_vec());
                        // println!("res: {:?}", locked);
                        println!("Write count: {}", write_count);
                        break;
                    }
                },
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            self.time.incr_cycles(initiation_interval);
        }
        self.time.incr_cycles(latency);
    }
}

#[cfg(test)]
mod tests {
    use dam::simulation::{InitializationOptions, ProgramBuilder, RunOptions};
    use dam::utility_contexts::GeneratorContext;

    use crate::templates::primitive::Token;
    use crate::templates::ramulator::hbm_context::{HBMConfig, HBMContext, ParAddrs, WriteBundle};
    use crate::token_vec;

    use super::{CompressedWrScan, ValsWrScan};

    #[test]
    fn vals_wr_scan_hbm_mode_smoke() {
        const USE_HBM: bool = true;
        let mut parent = ProgramBuilder::default();
        let (in_snd, in_rcv) = parent.unbounded::<Token<u32, u32>>();

        let mut vals = ValsWrScan::new(in_rcv);
        let out_arc = vals.out_val.clone();

        if USE_HBM {
            // HBM for value writes
            let (wr_addr_snd, wr_addr_rcv) = parent.unbounded::<ParAddrs>();
            let (wr_resp_snd, wr_resp_rcv) = parent.unbounded::<u64>();
            let mut mem = HBMContext::new(
                &mut parent,
                HBMConfig {
                    addr_offset: 64,
                    channel_num: 8,
                    per_channel_latency: 4,
                    per_channel_init_interval: 2,
                    per_channel_outstanding: 1,
                    per_channel_start_up_time: 10,
                },
            );
            mem.add_writer(WriteBundle {
                addr: wr_addr_rcv,
                resp: wr_resp_snd,
            });
            vals.enable_hbm_writes(wr_addr_snd, wr_resp_rcv, 0x3000_0000, 4);
            parent.add_child(mem);
        }

        let in_ref = || token_vec!(u32; u32; 10, 20, 30, "D").into_iter();
        parent.add_child(GeneratorContext::new(in_ref, in_snd));
        parent.add_child(vals);

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());

        let out_vals = out_arc.lock().unwrap().clone();
        assert_eq!(out_vals, vec![10u32, 20, 30]);
        println!(
            "ValsWrScan elapsed (HBM={}): {:?}",
            USE_HBM,
            executed.elapsed_cycles()
        );
    }

    #[test]
    fn compressed_wr_scan_hbm_mode_smoke() {
        const USE_HBM: bool = true;
        let mut parent = ProgramBuilder::default();
        let (in_snd, in_rcv) = parent.unbounded::<Token<u32, u32>>();

        let mut wr = CompressedWrScan::new(in_rcv);
        let crd_arc = wr.crd_arr.clone();
        let seg_arc = wr.seg_arr.clone();

        if USE_HBM {
            // HBM for writes to crd and seg arrays
            let (crd_addr_snd, crd_addr_rcv) = parent.unbounded::<ParAddrs>();
            let (crd_resp_snd, crd_resp_rcv) = parent.unbounded::<u64>();
            let (seg_addr_snd, seg_addr_rcv) = parent.unbounded::<ParAddrs>();
            let (seg_resp_snd, seg_resp_rcv) = parent.unbounded::<u64>();

            let mut mem = HBMContext::new(
                &mut parent,
                HBMConfig {
                    addr_offset: 64,
                    channel_num: 8,
                    per_channel_latency: 4,
                    per_channel_init_interval: 2,
                    per_channel_outstanding: 1,
                    per_channel_start_up_time: 10,
                },
            );
            mem.add_writer(WriteBundle {
                addr: crd_addr_rcv,
                resp: crd_resp_snd,
            });
            mem.add_writer(WriteBundle {
                addr: seg_addr_rcv,
                resp: seg_resp_snd,
            });
            wr.enable_hbm_writes(
                crd_addr_snd,
                crd_resp_rcv,
                seg_addr_snd,
                seg_resp_rcv,
                0x4000_0000,
                0x5000_0000,
                4,
                4,
            );
            parent.add_child(mem);
        }

        // Two fibers: [1,2] and [3]
        let in_ref = || token_vec!(u32; u32; 1, 2, "S0", 3, "S0", "D").into_iter();
        parent.add_child(GeneratorContext::new(in_ref, in_snd));
        parent.add_child(wr);

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());

        let crd = crd_arc.lock().unwrap().clone();
        let seg = seg_arc.lock().unwrap().clone();
        assert_eq!(crd, vec![1u32, 2, 3]);
        assert_eq!(seg, vec![0u32, 2, 3]);
        println!(
            "CompressedWrScan elapsed (HBM={}): {:?}",
            USE_HBM,
            executed.elapsed_cycles()
        );
    }
}
