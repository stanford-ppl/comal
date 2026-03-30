use dam::structures::Identifiable;
use dam::{
    context_tools::*,
    dam_macros::{context_macro, event_type},
    structures::Identifier,
};
use serde::{Deserialize, Serialize};

use super::primitive::Token;
// HBM timing interface
use crate::templates::ramulator::hbm_context::ParAddrs;

pub struct ArrayData<RefType: Clone, ValType: Clone, StopType: Clone> {
    pub in_ref: Receiver<Token<RefType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    /// Block size for block sparse mode (1 = scalar mode)
    pub block_size: usize,
}

#[context_macro]
pub struct Array<RefType: Clone, ValType: Clone, StopType: Clone> {
    array_data: ArrayData<RefType, ValType, StopType>,
    val_arr: Vec<ValType>,
    // Optional HBM-backed read interface for val_arr
    hbm_rd_addr_snd: Option<Sender<ParAddrs>>,
    hbm_rd_resp_rcv: Option<Receiver<u64>>,
    hbm_rd_base: u64,
    hbm_rd_stride: u64,
    // Batch size for HBM reads
    hbm_rd_batch: usize,
    // Number of HBM batches that can be in-flight simultaneously.
    // 1 = original blocking behavior, 2+ = double-buffering/prefetch.
    hbm_prefetch_depth: usize,
}

impl<RefType: DAMType, ValType: DAMType, StopType: DAMType> Array<RefType, ValType, StopType>
where
    Array<RefType, ValType, StopType>: Context,
{
    pub fn new(array_data: ArrayData<RefType, ValType, StopType>, val_arr: Vec<ValType>) -> Self {
        let arr = Array {
            array_data,
            val_arr,
            hbm_rd_addr_snd: None,
            hbm_rd_resp_rcv: None,
            hbm_rd_base: 0,
            hbm_rd_stride: 4,
            hbm_rd_batch: 8,
            hbm_prefetch_depth: 1,
            context_info: Default::default(),
        };
        (arr.array_data.in_ref).attach_receiver(&arr);
        (arr.array_data.out_val).attach_sender(&arr);

        arr
    }

    // Enable HBM-driven timing for reading from val_arr by reference index
    pub fn enable_hbm_reads(
        &mut self,
        rd_addr_snd: Sender<ParAddrs>,
        rd_resp_rcv: Receiver<u64>,
        base: u64,
        stride: u64,
    ) {
        rd_addr_snd.attach_sender(self);
        rd_resp_rcv.attach_receiver(self);
        self.hbm_rd_addr_snd = Some(rd_addr_snd);
        self.hbm_rd_resp_rcv = Some(rd_resp_rcv);
        self.hbm_rd_base = base;
        self.hbm_rd_stride = stride.max(1);
    }

    pub fn set_hbm_batch_size(&mut self, batch: usize) {
        self.hbm_rd_batch = batch.max(1);
    }

    pub fn set_prefetch_depth(&mut self, depth: usize) {
        self.hbm_prefetch_depth = depth.max(1);
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[event_type]
pub struct ArrayLog {
    in_ref: Token<u32, u32>,
    val: Token<f32, u32>,
}

impl<RefType, ValType, StopType> Context for Array<RefType, ValType, StopType>
where
    RefType: DAMType
        + std::ops::Mul<RefType, Output = RefType>
        + std::ops::Add<RefType, Output = RefType>,
    RefType: TryInto<usize>,
    <RefType as TryInto<usize>>::Error: std::fmt::Debug,
    ValType: DAMType,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
    // Token<u32, u32>: From<Token<RefType, StopType>>,  // Disabled for block sparse mode
    // Token<f32, u32>: From<Token<ValType, StopType>>,  // Disabled for block sparse mode
{
    fn init(&mut self) {}

    fn run(&mut self) {
        use std::collections::VecDeque;

        let mut num_reads: u64 = 0;
        let use_hbm = self.hbm_rd_addr_snd.is_some() && self.hbm_rd_resp_rcv.is_some();
        let mut pending_idx: Vec<usize> = Vec::new();
        let block_latency: u64 = (self.array_data.block_size * self.array_data.block_size) as u64;

        // Double-buffer: in-flight HBM batches awaiting responses
        let prefetch_depth = if use_hbm { self.hbm_prefetch_depth } else { 1 };
        let mut in_flight: VecDeque<Vec<usize>> = VecDeque::with_capacity(prefetch_depth + 1);

        loop {
            match self.array_data.in_ref.dequeue(&self.time) {
                Ok(curr_in) => {
                    let data = curr_in.data;
                    match data.clone() {
                        Token::Val(val) => {
                            let idx: usize = val.try_into().unwrap();
                            if use_hbm {
                                pending_idx.push(idx);
                                if pending_idx.len() >= self.hbm_rd_batch {
                                    // Issue HBM read for this batch
                                    if let Some(snd) = &self.hbm_rd_addr_snd {
                                        let addrs: Vec<u64> = pending_idx.iter()
                                            .map(|i| self.hbm_rd_base + (*i as u64) * self.hbm_rd_stride)
                                            .collect();
                                        snd.enqueue(&self.time,
                                            ChannelElement::new(self.time.tick(), ParAddrs::new(addrs))).unwrap();
                                    }
                                    in_flight.push_back(std::mem::take(&mut pending_idx));

                                    // If at prefetch capacity, drain oldest batch
                                    if in_flight.len() >= prefetch_depth {
                                        let oldest = in_flight.pop_front().unwrap();
                                        if let Some(rcv) = &self.hbm_rd_resp_rcv {
                                            let mut acks = 0usize;
                                            while acks < oldest.len() {
                                                match rcv.dequeue(&self.time) {
                                                    Ok(_) => acks += 1,
                                                    Err(_) => { self.time.incr_cycles(1); }
                                                }
                                            }
                                        }
                                        for i in oldest {
                                            num_reads += 1;
                                            self.array_data.out_val.enqueue(&self.time,
                                                ChannelElement::new(self.time.tick() + block_latency,
                                                    Token::Val(self.val_arr[i].clone()))).unwrap();
                                        }
                                    }
                                }
                            } else {
                                num_reads += 1;
                                self.array_data.out_val.enqueue(&self.time,
                                    ChannelElement::new(self.time.tick() + block_latency,
                                        Token::Val(self.val_arr[idx].clone()))).unwrap();
                            }
                        }
                        Token::Stop(_) | Token::Empty | Token::Done => {
                            // Flush: issue any remaining pending refs
                            if use_hbm && !pending_idx.is_empty() {
                                if let Some(snd) = &self.hbm_rd_addr_snd {
                                    let addrs: Vec<u64> = pending_idx.iter()
                                        .map(|i| self.hbm_rd_base + (*i as u64) * self.hbm_rd_stride)
                                        .collect();
                                    snd.enqueue(&self.time,
                                        ChannelElement::new(self.time.tick(), ParAddrs::new(addrs))).unwrap();
                                }
                                in_flight.push_back(std::mem::take(&mut pending_idx));
                            }
                            // Drain all in-flight batches
                            if use_hbm {
                                while let Some(batch) = in_flight.pop_front() {
                                    if let Some(rcv) = &self.hbm_rd_resp_rcv {
                                        let mut acks = 0usize;
                                        while acks < batch.len() {
                                            match rcv.dequeue(&self.time) {
                                                Ok(_) => acks += 1,
                                                Err(_) => { self.time.incr_cycles(1); }
                                            }
                                        }
                                    }
                                    for i in batch {
                                        num_reads += 1;
                                        self.array_data.out_val.enqueue(&self.time,
                                            ChannelElement::new(self.time.tick() + 1,
                                                Token::Val(self.val_arr[i].clone()))).unwrap();
                                    }
                                }
                            }
                            // Emit the control token
                            match data {
                                Token::Stop(stkn) => {
                                    self.array_data.out_val.enqueue(&self.time,
                                        ChannelElement::new(self.time.tick() + 1,
                                            Token::Stop(stkn.clone()))).unwrap();
                                }
                                Token::Empty => {
                                    self.array_data.out_val.enqueue(&self.time,
                                        ChannelElement::new(self.time.tick() + 1,
                                            Token::Val(ValType::default()))).unwrap();
                                }
                                Token::Done => {
                                    self.array_data.out_val.enqueue(&self.time,
                                        ChannelElement::new(self.time.tick() + 1, Token::Done)).unwrap();
                                    println!("Num reads: {}", num_reads);
                                    return;
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                }
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Software-pipelined sub-contexts: ArrayIssuer + ArrayConsumer
// ---------------------------------------------------------------------------
// Following the DAM PMU composite-context pattern (cf. HBMContext spawning
// HBMChannelContext children).  The issuer fires HBM address requests without
// blocking; the consumer blocks on HBM responses while the issuer runs ahead.

#[context_macro]
pub struct ArrayIssuer<RefType: Clone, StopType: Clone> {
    in_ref: Receiver<Token<RefType, StopType>>,
    hbm_addr_snd: Sender<ParAddrs>,
    idx_snd: Sender<Token<RefType, StopType>>,
    hbm_base: u64,
    hbm_stride: u64,
    batch_size: usize, // number of addresses per HBM request (MLP = memory-level parallelism)
}

impl<RefType: DAMType, StopType: DAMType> ArrayIssuer<RefType, StopType>
where
    ArrayIssuer<RefType, StopType>: Context,
{
    pub fn new(
        in_ref: Receiver<Token<RefType, StopType>>,
        hbm_addr_snd: Sender<ParAddrs>,
        idx_snd: Sender<Token<RefType, StopType>>,
        hbm_base: u64,
        hbm_stride: u64,
        batch_size: usize,
    ) -> Self {
        let ctx = Self {
            in_ref,
            hbm_addr_snd,
            idx_snd,
            hbm_base,
            hbm_stride,
            batch_size: batch_size.max(1),
            context_info: Default::default(),
        };
        ctx.in_ref.attach_receiver(&ctx);
        ctx.hbm_addr_snd.attach_sender(&ctx);
        ctx.idx_snd.attach_sender(&ctx);
        ctx
    }
}

impl<RefType, StopType> Context for ArrayIssuer<RefType, StopType>
where
    RefType: DAMType + TryInto<usize>,
    <RefType as TryInto<usize>>::Error: std::fmt::Debug,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut pending_addrs: Vec<u64> = Vec::with_capacity(self.batch_size);

        loop {
            match self.in_ref.dequeue(&self.time) {
                Ok(elem) => {
                    match elem.data {
                        Token::Val(ref val) => {
                            let idx: usize = val.clone().try_into().unwrap();
                            let addr = self.hbm_base + (idx as u64) * self.hbm_stride;
                            pending_addrs.push(addr);

                            // Forward token to consumer immediately
                            self.idx_snd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick(), elem.data),
                                )
                                .unwrap();

                            // When batch is full, fire all addresses to HBM at once
                            // This dispatches to batch_size channels simultaneously
                            if pending_addrs.len() >= self.batch_size {
                                self.hbm_addr_snd
                                    .enqueue(
                                        &self.time,
                                        ChannelElement::new(
                                            self.time.tick(),
                                            ParAddrs::new(std::mem::take(&mut pending_addrs)),
                                        ),
                                    )
                                    .unwrap();
                            }
                        }
                        Token::Stop(_) | Token::Empty => {
                            // Flush any partial batch before control token
                            if !pending_addrs.is_empty() {
                                self.hbm_addr_snd
                                    .enqueue(
                                        &self.time,
                                        ChannelElement::new(
                                            self.time.tick(),
                                            ParAddrs::new(std::mem::take(&mut pending_addrs)),
                                        ),
                                    )
                                    .unwrap();
                            }
                            self.idx_snd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick(), elem.data),
                                )
                                .unwrap();
                        }
                        Token::Done => {
                            if !pending_addrs.is_empty() {
                                self.hbm_addr_snd
                                    .enqueue(
                                        &self.time,
                                        ChannelElement::new(
                                            self.time.tick(),
                                            ParAddrs::new(std::mem::take(&mut pending_addrs)),
                                        ),
                                    )
                                    .unwrap();
                            }
                            self.idx_snd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick(), Token::Done),
                                )
                                .unwrap();
                            return;
                        }
                    }
                }
                Err(_) => {
                    panic!("ArrayIssuer: unexpected end of stream");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

#[context_macro]
pub struct ArrayConsumer<RefType: Clone, ValType: Clone, StopType: Clone> {
    idx_rcv: Receiver<Token<RefType, StopType>>,
    hbm_resp_rcv: Receiver<u64>,
    out_val: Sender<Token<ValType, StopType>>,
    val_arr: Vec<ValType>,
    block_size: usize,
}

impl<RefType: DAMType, ValType: DAMType, StopType: DAMType> ArrayConsumer<RefType, ValType, StopType>
where
    ArrayConsumer<RefType, ValType, StopType>: Context,
{
    pub fn new(
        idx_rcv: Receiver<Token<RefType, StopType>>,
        hbm_resp_rcv: Receiver<u64>,
        out_val: Sender<Token<ValType, StopType>>,
        val_arr: Vec<ValType>,
        block_size: usize,
    ) -> Self {
        let ctx = Self {
            idx_rcv,
            hbm_resp_rcv,
            out_val,
            val_arr,
            block_size,
            context_info: Default::default(),
        };
        ctx.idx_rcv.attach_receiver(&ctx);
        ctx.hbm_resp_rcv.attach_receiver(&ctx);
        ctx.out_val.attach_sender(&ctx);
        ctx
    }
}

impl<RefType, ValType, StopType> Context for ArrayConsumer<RefType, ValType, StopType>
where
    RefType: DAMType + TryInto<usize>,
    <RefType as TryInto<usize>>::Error: std::fmt::Debug,
    ValType: DAMType,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut num_reads: u64 = 0;
        let block_latency: u64 = (self.block_size * self.block_size) as u64;
        loop {
            match self.idx_rcv.dequeue(&self.time) {
                Ok(elem) => {
                    match elem.data {
                        Token::Val(val) => {
                            let idx: usize = val.try_into().unwrap();
                            // Wait for HBM response using peek + incr_cycles
                            // to advance time gradually (1 cycle at a time).
                            // This preserves the 1-token-per-cycle cadence that
                            // downstream spacc/reduce nodes expect.
                            // Using dequeue() would atomically jump time to T+100,
                            // breaking timing alignment with crd streams.
                            loop {
                                match self.hbm_resp_rcv.peek() {
                                    dam::channel::PeekResult::Something(ref ce)
                                        if ce.time <= self.time.tick() =>
                                    {
                                        self.hbm_resp_rcv.dequeue(&self.time).unwrap();
                                        break;
                                    }
                                    dam::channel::PeekResult::Something(_) => {
                                        // Response exists but in the future — advance time
                                        self.time.incr_cycles(1);
                                    }
                                    dam::channel::PeekResult::Nothing(_) => {
                                        self.time.incr_cycles(1);
                                    }
                                    dam::channel::PeekResult::Closed => break,
                                }
                            }
                            num_reads += 1;
                            self.out_val
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        self.time.tick() + block_latency,
                                        Token::Val(self.val_arr[idx].clone()),
                                    ),
                                )
                                .unwrap();
                        }
                        Token::Stop(stkn) => {
                            self.out_val
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        self.time.tick() + 1,
                                        Token::Stop(stkn),
                                    ),
                                )
                                .unwrap();
                        }
                        Token::Empty => {
                            self.out_val
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        self.time.tick() + 1,
                                        Token::Val(ValType::default()),
                                    ),
                                )
                                .unwrap();
                        }
                        Token::Done => {
                            self.out_val
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(self.time.tick() + 1, Token::Done),
                                )
                                .unwrap();
                            println!("Num reads: {}", num_reads);
                            return;
                        }
                    }
                }
                Err(_) => {
                    panic!("ArrayConsumer: unexpected end of stream");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

impl<RefType, ValType, StopType> Array<RefType, ValType, StopType>
where
    RefType: DAMType + TryInto<usize> + std::ops::Mul<RefType, Output = RefType> + std::ops::Add<RefType, Output = RefType>,
    <RefType as TryInto<usize>>::Error: std::fmt::Debug,
    ValType: DAMType,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    /// Consume `self` and register two pipelined sub-contexts (issuer + consumer)
    /// with `builder`, connected by an internal channel.  The issuer fires HBM
    /// address requests without ever blocking on the response; the consumer
    /// blocks on the response channel, but DAM's coroutine scheduler yields to
    /// the issuer while it waits, achieving full latency overlap.
    pub fn enable_hbm_pipelined<'a>(
        self,
        builder: &mut dam::simulation::ProgramBuilder<'a>,
        hbm_addr_snd: Sender<ParAddrs>,
        hbm_resp_rcv: Receiver<u64>,
        hbm_base: u64,
        hbm_stride: u64,
    ) where
        RefType: 'a,
        ValType: 'a,
        StopType: 'a,
    {
        // Internal channel connecting issuer -> consumer
        let (idx_snd, idx_rcv) = builder.unbounded::<Token<RefType, StopType>>();

        // batch_size=32 matches U280's 32 HBM channels — one address per channel
        // per dispatch cycle, maximizing memory-level parallelism.
        let issuer = ArrayIssuer::new(
            self.array_data.in_ref,
            hbm_addr_snd,
            idx_snd,
            hbm_base,
            hbm_stride,
            32,
        );

        let consumer = ArrayConsumer::new(
            idx_rcv,
            hbm_resp_rcv,
            self.array_data.out_val,
            self.val_arr,
            self.array_data.block_size,
        );

        builder.add_child(issuer);
        builder.add_child(consumer);
        // `self` is consumed -- NOT added to builder
    }
}

#[cfg(test)]
mod tests {
    use dam::simulation::*;
    use dam::utility_contexts::*;

    use crate::templates::primitive::Token;
    use crate::templates::ramulator::hbm_context::{HBMConfig, HBMContext, ParAddrs, ReadBundle};
    use crate::token_vec;

    use super::Array;
    use super::ArrayData;

    #[test]
    fn array_2d_test() {
        let in_ref = || {
            token_vec![u32; u32; "N", 0, 1, 2, "S0", "N", "N", "S0", 2, 3, 4, "S0", "N", "N", "S1", "D"].into_iter()
        };
        let out_val = || {
            token_vec!(u32; u32; 0, 1, 2, 3, "S0", 0, 0, "S0", 3, 4, 5, "S0", 0, 0, "S1", "D")
                .into_iter()
        };
        let val_arr = vec![1u32, 2, 3, 4, 5];
        array_test(in_ref, out_val, val_arr);
    }

    #[test]
    fn array_hbm_mode_smoke() {
        const USE_HBM: bool = true;
        let mut parent = ProgramBuilder::default();
        let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_val_sender, out_val_receiver) = parent.unbounded::<Token<u32, u32>>();
        let data = ArrayData::<u32, u32, u32> {
            in_ref: in_ref_receiver,
            out_val: out_val_sender,
            block_size: 1,
        };
        let val_arr = vec![10u32, 20, 30, 40];
        let mut arr = Array::new(data, val_arr);

        if USE_HBM {
            let (rd_addr_snd, rd_addr_rcv) = parent.unbounded::<ParAddrs>();
            let (rd_resp_snd, rd_resp_rcv) = parent.unbounded::<u64>();
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
            mem.add_reader(ReadBundle {
                addr: rd_addr_rcv,
                resp: rd_resp_snd,
            });
            arr.enable_hbm_reads(rd_addr_snd, rd_resp_rcv, 0x6000_0000, 4);
            parent.add_child(mem);
        }

        let in_ref = || token_vec!(u32; u32; 0, 2, 1, "D").into_iter();
        let expected = || token_vec!(u32; u32; 10, 30, 20, "D").into_iter();
        parent.add_child(GeneratorContext::new(in_ref, in_ref_sender));
        parent.add_child(CheckerContext::new(expected, out_val_receiver));
        parent.add_child(arr);

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        println!(
            "Array elapsed (HBM={}): {:?}",
            USE_HBM,
            executed.elapsed_cycles()
        );
    }

    #[test]
    fn array_hbm_pipelined_smoke() {
        let mut parent = ProgramBuilder::default();
        let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_val_sender, out_val_receiver) = parent.unbounded::<Token<u32, u32>>();
        let data = ArrayData::<u32, u32, u32> {
            in_ref: in_ref_receiver,
            out_val: out_val_sender,
            block_size: 1,
        };
        let val_arr = vec![10u32, 20, 30, 40];
        let arr = Array::new(data, val_arr);

        let (rd_addr_snd, rd_addr_rcv) = parent.unbounded::<ParAddrs>();
        let (rd_resp_snd, rd_resp_rcv) = parent.unbounded::<u64>();
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
        mem.add_reader(ReadBundle {
            addr: rd_addr_rcv,
            resp: rd_resp_snd,
        });
        // Use pipelined mode: arr is consumed, issuer + consumer added to builder
        arr.enable_hbm_pipelined(&mut parent, rd_addr_snd, rd_resp_rcv, 0x6000_0000, 4);
        parent.add_child(mem);

        let in_ref = || token_vec!(u32; u32; 0, 2, 1, "D").into_iter();
        let expected = || token_vec!(u32; u32; 10, 30, 20, "D").into_iter();
        parent.add_child(GeneratorContext::new(in_ref, in_ref_sender));
        parent.add_child(CheckerContext::new(expected, out_val_receiver));

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        println!(
            "Array pipelined elapsed: {:?}",
            executed.elapsed_cycles()
        );
    }

    fn array_test<IRT, ORT>(in_ref: fn() -> IRT, out_val: fn() -> ORT, val_arr: Vec<u32>)
    where
        IRT: Iterator<Item = Token<u32, u32>> + 'static,
        ORT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_val_sender, out_val_receiver) = parent.unbounded::<Token<u32, u32>>();
        let data = ArrayData::<u32, u32, u32> {
            in_ref: in_ref_receiver,
            out_val: out_val_sender,
            block_size: 1,
        };
        let arr = Array::new(data, val_arr);
        let gen1 = GeneratorContext::new(in_ref, in_ref_sender);
        let out_val_checker = CheckerContext::new(out_val, out_val_receiver);
        parent.add_child(gen1);
        parent.add_child(out_val_checker);
        parent.add_child(arr);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }
}
