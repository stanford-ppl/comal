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
        let mut num_reads: u64 = 0;
        let use_hbm = self.hbm_rd_addr_snd.is_some() && self.hbm_rd_resp_rcv.is_some();
        let mut pending_idx: Vec<usize> = Vec::new();
        // Block sparse timing: block_size * block_size cycles per access
        let block_latency: u64 = (self.array_data.block_size * self.array_data.block_size) as u64;
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
                                    if let Some(snd) = &self.hbm_rd_addr_snd {
                                        let addrs: Vec<u64> = pending_idx
                                            .iter()
                                            .map(|i| {
                                                self.hbm_rd_base + (*i as u64) * self.hbm_rd_stride
                                            })
                                            .collect();
                                        snd.enqueue(
                                            &self.time,
                                            ChannelElement::new(
                                                self.time.tick(),
                                                ParAddrs::new(addrs),
                                            ),
                                        )
                                        .unwrap();
                                    }
                                    if let Some(rcv) = &self.hbm_rd_resp_rcv {
                                        let mut acks = 0usize;
                                        while acks < pending_idx.len() {
                                            match rcv.dequeue(&self.time) {
                                                Ok(_) => acks += 1,
                                                Err(_) => self.time.incr_cycles(1),
                                            }
                                        }
                                    }
                                    // Emit now for all in batch
                                    for i in pending_idx.drain(..) {
                                        let channel_elem = ChannelElement::new(
                                            self.time.tick() + block_latency,
                                            Token::Val(self.val_arr[i].clone()),
                                        );
                                        num_reads += 1;
                                        self.array_data
                                            .out_val
                                            .enqueue(&self.time, channel_elem)
                                            .unwrap();
                                        // Logging disabled for block sparse compatibility
                                    }
                                }
                            } else {
                                let channel_elem = ChannelElement::new(
                                    self.time.tick() + block_latency,
                                    Token::Val(self.val_arr[idx].clone()),
                                );
                                num_reads += 1;
                                self.array_data
                                    .out_val
                                    .enqueue(&self.time, channel_elem)
                                    .unwrap();
                                // Logging disabled for block sparse compatibility
                            }
                        }
                        Token::Stop(stkn) => {
                            if use_hbm && !pending_idx.is_empty() {
                                if let Some(snd) = &self.hbm_rd_addr_snd {
                                    let addrs: Vec<u64> = pending_idx
                                        .iter()
                                        .map(|i| {
                                            self.hbm_rd_base + (*i as u64) * self.hbm_rd_stride
                                        })
                                        .collect();
                                    snd.enqueue(
                                        &self.time,
                                        ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                    )
                                    .unwrap();
                                }
                                if let Some(rcv) = &self.hbm_rd_resp_rcv {
                                    let mut acks = 0usize;
                                    while acks < pending_idx.len() {
                                        match rcv.dequeue(&self.time) {
                                            Ok(_) => acks += 1,
                                            Err(_) => self.time.incr_cycles(1),
                                        }
                                    }
                                }
                                for i in pending_idx.drain(..) {
                                    let channel_elem = ChannelElement::new(
                                        self.time.tick() + 1,
                                        Token::Val(self.val_arr[i].clone()),
                                    );
                                    num_reads += 1;
                                    self.array_data
                                        .out_val
                                        .enqueue(&self.time, channel_elem)
                                        .unwrap();
                                }
                            }
                            let channel_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::Stop(stkn.clone()),
                            );
                            self.array_data
                                .out_val
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            // Logging disabled for block sparse compatibility
                        }
                        Token::Empty => {
                            if use_hbm && !pending_idx.is_empty() {
                                if let Some(snd) = &self.hbm_rd_addr_snd {
                                    let addrs: Vec<u64> = pending_idx
                                        .iter()
                                        .map(|i| {
                                            self.hbm_rd_base + (*i as u64) * self.hbm_rd_stride
                                        })
                                        .collect();
                                    snd.enqueue(
                                        &self.time,
                                        ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                    )
                                    .unwrap();
                                }
                                if let Some(rcv) = &self.hbm_rd_resp_rcv {
                                    let mut acks = 0usize;
                                    while acks < pending_idx.len() {
                                        match rcv.dequeue(&self.time) {
                                            Ok(_) => acks += 1,
                                            Err(_) => self.time.incr_cycles(1),
                                        }
                                    }
                                }
                                for i in pending_idx.drain(..) {
                                    let channel_elem = ChannelElement::new(
                                        self.time.tick() + 1,
                                        Token::Val(self.val_arr[i].clone()),
                                    );
                                    num_reads += 1;
                                    self.array_data
                                        .out_val
                                        .enqueue(&self.time, channel_elem)
                                        .unwrap();
                                }
                            }
                            let channel_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::Val(ValType::default()),
                            );

                            self.array_data
                                .out_val
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            // Logging disabled for block sparse compatibility
                        }
                        Token::Done => {
                            if use_hbm && !pending_idx.is_empty() {
                                if let Some(snd) = &self.hbm_rd_addr_snd {
                                    let addrs: Vec<u64> = pending_idx
                                        .iter()
                                        .map(|i| {
                                            self.hbm_rd_base + (*i as u64) * self.hbm_rd_stride
                                        })
                                        .collect();
                                    snd.enqueue(
                                        &self.time,
                                        ChannelElement::new(self.time.tick(), ParAddrs::new(addrs)),
                                    )
                                    .unwrap();
                                }
                                if let Some(rcv) = &self.hbm_rd_resp_rcv {
                                    let mut acks = 0usize;
                                    while acks < pending_idx.len() {
                                        match rcv.dequeue(&self.time) {
                                            Ok(_) => acks += 1,
                                            Err(_) => self.time.incr_cycles(1),
                                        }
                                    }
                                }
                                for i in pending_idx.drain(..) {
                                    let channel_elem = ChannelElement::new(
                                        self.time.tick() + 1,
                                        Token::Val(self.val_arr[i].clone()),
                                    );
                                    num_reads += 1;
                                    self.array_data
                                        .out_val
                                        .enqueue(&self.time, channel_elem)
                                        .unwrap();
                                }
                            }
                            let channel_elem =
                                ChannelElement::new(self.time.tick() + 1, Token::Done);
                            self.array_data
                                .out_val
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            println!("Num reads: {}", num_reads);
                            // Logging disabled for block sparse compatibility
                            return;
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
