use std::sync::{Arc, Mutex};

use dam::{context_tools::*, dam_macros::context_macro};

use super::primitive::Token;
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
    // Batch sizes (default 1 = no batching)
    hbm_crd_batch: usize,
    hbm_seg_batch: usize,
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
            hbm_crd_batch: 8,
            hbm_seg_batch: 8,
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

    pub fn set_hbm_batch_sizes(&mut self, crd_batch: usize, seg_batch: usize) {
        self.hbm_crd_batch = crd_batch.max(1);
        self.hbm_seg_batch = seg_batch.max(1);
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

        let use_crd_hbm = self.hbm_crd_addr_snd.is_some() && self.hbm_crd_resp_rcv.is_some();
        let use_seg_hbm = self.hbm_seg_addr_snd.is_some() && self.hbm_seg_resp_rcv.is_some();
        // Track logical lengths without holding locks
        let mut _crd_len = { self.crd_arr.lock().unwrap().len() };
        let mut _seg_len = { self.seg_arr.lock().unwrap().len() };

        // Pending batches for writes
        let mut pending_crd_idx: Vec<usize> = Vec::new();
        let mut pending_crd_vals: Vec<ValType> = Vec::new();
        let mut pending_seg_idx: Vec<usize> = Vec::new();
        let mut pending_seg_vals: Vec<ValType> = Vec::new();

        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        if use_crd_hbm {
                            // Defer commit until batch flush
                            let idx = _crd_len + pending_crd_vals.len();
                            pending_crd_idx.push(idx);
                            pending_crd_vals.push(val.clone());
                            if pending_crd_idx.len() >= self.hbm_crd_batch {
                                // Flush CRD batch: send addresses, wait acks, then commit
                                if let Some(snd) = &self.hbm_crd_addr_snd {
                                    let addrs: Vec<u64> = pending_crd_idx
                                        .iter()
                                        .map(|i| {
                                            self.hbm_crd_base + (*i as u64) * self.hbm_crd_stride
                                        })
                                        .collect();
                                    snd.enqueue(
                                        &self.time,
                                        ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                    )
                                    .unwrap();
                                }
                                if let Some(rcv) = &self.hbm_crd_resp_rcv {
                                    let mut acks = 0usize;
                                    while acks < pending_crd_idx.len() {
                                        match rcv.dequeue(&self.time) {
                                            Ok(_) => acks += 1,
                                            Err(_) => self.time.incr_cycles(1),
                                        }
                                    }
                                }
                                // Commit
                                {
                                    let mut lock = self.crd_arr.lock().unwrap();
                                    for v in pending_crd_vals.drain(..) {
                                        lock.push(v);
                                    }
                                }
                                _crd_len += pending_crd_idx.len();
                                pending_crd_idx.clear();
                            }
                        } else {
                            let mut lock = self.crd_arr.lock().unwrap();
                            lock.push(val.clone());
                            _crd_len += 1;
                        }
                        curr_crd_cnt += 1;
                        end_fiber = false;
                        // println!("{:?}", val.clone());
                        crd_write_count += 1;
                    }
                    Token::Stop(_) if !end_fiber => {
                        if use_seg_hbm {
                            // Defer commit until batch flush
                            let idx = _seg_len + pending_seg_vals.len();
                            pending_seg_idx.push(idx);
                            pending_seg_vals.push(curr_crd_cnt.clone());
                            if pending_seg_idx.len() >= self.hbm_seg_batch {
                                // Flush SEG batch
                                if let Some(snd) = &self.hbm_seg_addr_snd {
                                    let addrs: Vec<u64> = pending_seg_idx
                                        .iter()
                                        .map(|i| {
                                            self.hbm_seg_base + (*i as u64) * self.hbm_seg_stride
                                        })
                                        .collect();
                                    snd.enqueue(
                                        &self.time,
                                        ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                    )
                                    .unwrap();
                                }
                                if let Some(rcv) = &self.hbm_seg_resp_rcv {
                                    let mut acks = 0usize;
                                    while acks < pending_seg_idx.len() {
                                        match rcv.dequeue(&self.time) {
                                            Ok(_) => acks += 1,
                                            Err(_) => self.time.incr_cycles(1),
                                        }
                                    }
                                }
                                // Commit
                                {
                                    let mut lock = self.seg_arr.lock().unwrap();
                                    for v in pending_seg_vals.drain(..) {
                                        lock.push(v);
                                    }
                                }
                                _seg_len += pending_seg_idx.len();
                                pending_seg_idx.clear();
                            }
                        } else {
                            let mut lock = self.seg_arr.lock().unwrap();
                            lock.push(curr_crd_cnt.clone());
                            _seg_len += 1;
                        }
                        end_fiber = true;
                        seg_write_count += 1;
                    }
                    Token::Empty | Token::Stop(_) => {
                        // Flush pending batches on control tokens
                        if use_crd_hbm && !pending_crd_idx.is_empty() {
                            if let Some(snd) = &self.hbm_crd_addr_snd {
                                let addrs: Vec<u64> = pending_crd_idx
                                    .iter()
                                    .map(|i| self.hbm_crd_base + (*i as u64) * self.hbm_crd_stride)
                                    .collect();
                                snd.enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                )
                                .unwrap();
                            }
                            if let Some(rcv) = &self.hbm_crd_resp_rcv {
                                let mut acks = 0usize;
                                while acks < pending_crd_idx.len() {
                                    match rcv.dequeue(&self.time) {
                                        Ok(_) => acks += 1,
                                        Err(_) => self.time.incr_cycles(1),
                                    }
                                }
                            }
                            {
                                let mut lock = self.crd_arr.lock().unwrap();
                                for v in pending_crd_vals.drain(..) {
                                    lock.push(v);
                                }
                            }
                            _crd_len += pending_crd_idx.len();
                            pending_crd_idx.clear();
                        }
                        if use_seg_hbm && !pending_seg_idx.is_empty() {
                            if let Some(snd) = &self.hbm_seg_addr_snd {
                                let addrs: Vec<u64> = pending_seg_idx
                                    .iter()
                                    .map(|i| self.hbm_seg_base + (*i as u64) * self.hbm_seg_stride)
                                    .collect();
                                snd.enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                )
                                .unwrap();
                            }
                            if let Some(rcv) = &self.hbm_seg_resp_rcv {
                                let mut acks = 0usize;
                                while acks < pending_seg_idx.len() {
                                    match rcv.dequeue(&self.time) {
                                        Ok(_) => acks += 1,
                                        Err(_) => self.time.incr_cycles(1),
                                    }
                                }
                            }
                            {
                                let mut lock = self.seg_arr.lock().unwrap();
                                for v in pending_seg_vals.drain(..) {
                                    lock.push(v);
                                }
                            }
                            _seg_len += pending_seg_idx.len();
                            pending_seg_idx.clear();
                        }
                        continue;
                    }
                    Token::Done => {
                        if use_crd_hbm && !pending_crd_idx.is_empty() {
                            if let Some(snd) = &self.hbm_crd_addr_snd {
                                let addrs: Vec<u64> = pending_crd_idx
                                    .iter()
                                    .map(|i| self.hbm_crd_base + (*i as u64) * self.hbm_crd_stride)
                                    .collect();
                                snd.enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                )
                                .unwrap();
                            }
                            if let Some(rcv) = &self.hbm_crd_resp_rcv {
                                let mut acks = 0usize;
                                while acks < pending_crd_idx.len() {
                                    match rcv.dequeue(&self.time) {
                                        Ok(_) => acks += 1,
                                        Err(_) => self.time.incr_cycles(1),
                                    }
                                }
                            }
                            let mut lock = self.crd_arr.lock().unwrap();
                            for v in pending_crd_vals.drain(..) {
                                lock.push(v);
                            }
                            _crd_len += pending_crd_idx.len();
                            pending_crd_idx.clear();
                        }
                        if use_seg_hbm && !pending_seg_idx.is_empty() {
                            if let Some(snd) = &self.hbm_seg_addr_snd {
                                let addrs: Vec<u64> = pending_seg_idx
                                    .iter()
                                    .map(|i| self.hbm_seg_base + (*i as u64) * self.hbm_seg_stride)
                                    .collect();
                                snd.enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                )
                                .unwrap();
                            }
                            if let Some(rcv) = &self.hbm_seg_resp_rcv {
                                let mut acks = 0usize;
                                while acks < pending_seg_idx.len() {
                                    match rcv.dequeue(&self.time) {
                                        Ok(_) => acks += 1,
                                        Err(_) => self.time.incr_cycles(1),
                                    }
                                }
                            }
                            let mut lock = self.seg_arr.lock().unwrap();
                            for v in pending_seg_vals.drain(..) {
                                lock.push(v);
                            }
                            _seg_len += pending_seg_idx.len();
                            pending_seg_idx.clear();
                        }
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
    // Batch size for value writes
    hbm_wr_batch: usize,
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
            hbm_wr_batch: 1,
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

    pub fn set_hbm_batch_size(&mut self, batch: usize) {
        self.hbm_wr_batch = batch.max(1);
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
        let mut pending_idx: Vec<usize> = Vec::new();
        let mut pending_vals: Vec<ValType> = Vec::new();
        let mut out_len = { self.out_val.lock().unwrap().len() };
        let use_hbm = self.hbm_wr_addr_snd.is_some() && self.hbm_wr_resp_rcv.is_some();
        let mut write_count: u64 = 0;
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        if use_hbm {
                            let idx = out_len + pending_vals.len();
                            pending_idx.push(idx);
                            pending_vals.push(val.clone());
                            if pending_idx.len() >= self.hbm_wr_batch {
                                if let Some(snd) = &self.hbm_wr_addr_snd {
                                    let addrs: Vec<u64> = pending_idx
                                        .iter()
                                        .map(|i| {
                                            self.hbm_wr_base + (*i as u64) * self.hbm_wr_stride
                                        })
                                        .collect();
                                    snd.enqueue(
                                        &self.time,
                                        ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                    )
                                    .unwrap();
                                }
                                if let Some(rcv) = &self.hbm_wr_resp_rcv {
                                    let mut acks = 0usize;
                                    while acks < pending_idx.len() {
                                        match rcv.dequeue(&self.time) {
                                            Ok(_) => acks += 1,
                                            Err(_) => self.time.incr_cycles(1),
                                        }
                                    }
                                }
                                {
                                    let mut lock = self.out_val.lock().unwrap();
                                    for v in pending_vals.drain(..) {
                                        lock.push(v);
                                    }
                                }
                                out_len += pending_idx.len();
                                pending_idx.clear();
                            }
                        }
                        // println!("Value: {:?}", Token::<ValType, StopType>::Val(val.clone()));
                        if !use_hbm {
                            let mut lock = self.out_val.lock().unwrap();
                            lock.push(val.clone());
                            out_len += 1;
                        }
                        // println!("{:?}", val.clone());
                        write_count += 1;
                    }
                    Token::Empty | Token::Stop(_) => {
                        if use_hbm && !pending_idx.is_empty() {
                            if let Some(snd) = &self.hbm_wr_addr_snd {
                                let addrs: Vec<u64> = pending_idx
                                    .iter()
                                    .map(|i| self.hbm_wr_base + (*i as u64) * self.hbm_wr_stride)
                                    .collect();
                                snd.enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                )
                                .unwrap();
                            }
                            if let Some(rcv) = &self.hbm_wr_resp_rcv {
                                let mut acks = 0usize;
                                while acks < pending_idx.len() {
                                    match rcv.dequeue(&self.time) {
                                        Ok(_) => acks += 1,
                                        Err(_) => self.time.incr_cycles(1),
                                    }
                                }
                            }
                            let mut lock = self.out_val.lock().unwrap();
                            for v in pending_vals.drain(..) {
                                lock.push(v);
                            }
                            out_len += pending_idx.len();
                            pending_idx.clear();
                        }
                        continue;
                    }
                    Token::Done => {
                        if use_hbm && !pending_idx.is_empty() {
                            if let Some(snd) = &self.hbm_wr_addr_snd {
                                let addrs: Vec<u64> = pending_idx
                                    .iter()
                                    .map(|i| self.hbm_wr_base + (*i as u64) * self.hbm_wr_stride)
                                    .collect();
                                snd.enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                )
                                .unwrap();
                            }
                            if let Some(rcv) = &self.hbm_wr_resp_rcv {
                                let mut acks = 0usize;
                                while acks < pending_idx.len() {
                                    match rcv.dequeue(&self.time) {
                                        Ok(_) => acks += 1,
                                        Err(_) => self.time.incr_cycles(1),
                                    }
                                }
                            }
                            let mut lock = self.out_val.lock().unwrap();
                            for v in pending_vals.drain(..) {
                                lock.push(v);
                            }
                            out_len += pending_idx.len();
                            pending_idx.clear();
                        }
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
