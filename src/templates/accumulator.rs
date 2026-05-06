use core::hash::Hash;
use std::{collections::BTreeMap, u32};

use dam::structures::{Identifiable, Identifier, Time};
use dam::{
    context_tools::*,
    dam_macros::{context_macro, event_type},
};
use serde::{Deserialize, Serialize};

use super::primitive::{FinalReduce, Token};

pub struct ReduceData<ValType: Clone, StopType: Clone, const N: usize> {
    pub in_val: Receiver<Token<ValType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    pub sum: bool,
    /// Stop depth at which this reduction's fiber ends.
    ///   Stops at depth <  reduction_depth → within reduction (consume, no emit)
    ///   Stops at depth >= reduction_depth → at-or-above reduction-fiber-end
    ///                                       (emit Val(sum), reset, decrement Stop)
    /// Default 0 = "1-rank reduction; flush at Stop(0)" — preserves untiled behavior.
    pub reduction_depth: u32,
}

#[context_macro]
pub struct Reduce<ValType: Clone, StopType: Clone, const N: usize> {
    reduce_data: ReduceData<ValType, StopType, N>,
}

impl<ValType: DAMType, StopType: DAMType, const N: usize> Reduce<ValType, StopType, N>
where
    Reduce<ValType, StopType, N>: Context,
{
    pub fn new(reduce_data: ReduceData<ValType, StopType, N>) -> Self {
        let red = Reduce {
            reduce_data,
            context_info: Default::default(),
        };
        (red.reduce_data.in_val).attach_receiver(&red);
        (red.reduce_data.out_val).attach_sender(&red);

        red
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[event_type]
pub struct ReduceLog {
    out_val: Token<f32, u32>,
    // val: Token<f32, u32>,
}

#[derive(Serialize, Deserialize, Debug)]
#[event_type]
pub struct SpaccLog {
    out_val: Token<f32, u32>,
    out_crd: Token<u32, u32>,
    // val: Token<f32, u32>,
}

// impl<ValType, StopType> Context for Reduce<ValType, StopType>
// where
//     ValType: DAMType + std::ops::AddAssign<ValType>,
//     StopType: DAMType
//         + std::ops::Add<u32, Output = StopType>
//         + std::ops::Sub<u32, Output = StopType>
//         + std::cmp::PartialEq,
// {
//     fn init(&mut self) {}

//     fn run(&mut self) {
//         let mut sum = ValType::default();
//         loop {
//             match self.reduce_data.in_val.dequeue(&self.time) {
//                 Ok(curr_in) => match curr_in.data {
//                     Token::Val(val) => {
//                         sum += val;
//                     }
//                     Token::Stop(stkn) => {
//                         let curr_time = self.time.tick();
//                         self.reduce_data
//                             .out_val
//                             .enqueue(
//                                 &self.time,
//                                 ChannelElement::new(curr_time + 1, Token::Val(sum)),
//                             )
//                             .unwrap();
//                         sum = ValType::default();
//                         if stkn != StopType::default() {
//                             self.reduce_data
//                                 .out_val
//                                 .enqueue(
//                                     &self.time,
//                                     ChannelElement::new(curr_time + 1, Token::Stop(stkn - 1)),
//                                 )
//                                 .unwrap();
//                         }
//                     }
//                     Token::Empty => {
//                         continue;
//                     }
//                     Token::Done => {
//                         let curr_time = self.time.tick();
//                         self.reduce_data
//                             .out_val
//                             .enqueue(&self.time, ChannelElement::new(curr_time + 1, Token::Done))
//                             .unwrap();
//                         return;
//                     }
//                 },
//                 Err(_) => {
//                     panic!("Unexpected end of stream");
//                 }
//             }
//             self.time.incr_cycles(1);
//         }
//     }
// }

impl<ValType, StopType, const N: usize> Context for Reduce<ValType, StopType, N>
where
    ValType: DAMType
        + std::ops::AddAssign<ValType>
        + std::cmp::PartialEq
        + std::ops::Add<Output = ValType>,
    StopType: DAMType
        + std::ops::Add<u32, Output = StopType>
        + std::ops::Sub<u32, Output = StopType>
        + std::cmp::PartialEq
        + std::cmp::PartialOrd
        + std::convert::From<u32>,
    // Token<f32, u32>: From<Token<ValType, StopType>>,  // Disabled for block sparse mode
    Token<ValType, StopType>: FinalReduce<StopType, N>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut sum = ValType::default();
        let id = self.id();
        let curr_id = Identifier { id: 0 };
        let mut reduce_count: u64 = 0;
        loop {
            match self.reduce_data.in_val.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data.clone() {
                    Token::Val(val) => {
                        sum = sum + val.clone();
                        reduce_count += 1;
                    }
                    Token::Stop(stkn) => {
                        let curr_time = self.time.tick();

                        // reduction_depth gates whether this Stop is at-or-above
                        // the reduction fiber (flush + emit + decrement) or within
                        // the reduction (consume, no emission, sum keeps growing).
                        // Default reduction_depth=0 → all Stops are at-or-above →
                        // current 1-rank-reduction behavior preserved byte-identically.
                        let red_depth = StopType::from(self.reduce_data.reduction_depth);
                        if stkn < red_depth {
                            // Within multi-rank reduction: consume Stop, keep accumulating.
                            // No Val/Stop emission; downstream sees the reduction
                            // collapse all input ranks below reduction_depth.
                            // (The rank-collapse means there's no corresponding output
                            // Stop level for input depths below reduction_depth.)
                            continue;
                        }

                        let final_reduce = if self.reduce_data.sum {
                            Token::Val(sum.clone()).sum_axis()
                        } else {
                            Token::Val(sum.clone())
                        };
                        self.reduce_data
                            .out_val
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    curr_time + Time::new((N * N).try_into().unwrap()),
                                    final_reduce,
                                ),
                            )
                            .unwrap();
                        if id == curr_id {
                            println!(
                                "In val: {:?}, Out val: {:?}",
                                curr_in.data.clone(),
                                Token::<ValType, StopType>::Val(sum.clone())
                            );
                        }
                        let _out_val = Token::<ValType, StopType>::Val(sum.clone());
                        sum = ValType::default();
                        if stkn != StopType::default() {
                            self.reduce_data
                                .out_val
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + 1,
                                        Token::Stop(stkn.clone() - 1),
                                    ),
                                )
                                .unwrap();
                            let _stk = Token::<ValType, StopType>::Stop(stkn.clone() - 1);
                            if id == curr_id {
                                println!(
                                    "In val: {:?}, Out val: {:?}",
                                    curr_in.data.clone(),
                                    Token::<ValType, StopType>::Stop(stkn.clone() - 1)
                                );
                            }
                        }
                    }
                    Token::Empty => {
                        continue;
                    }
                    Token::Done => {
                        let curr_time = self.time.tick();
                        self.reduce_data
                            .out_val
                            .enqueue(&self.time, ChannelElement::new(curr_time + 1, Token::Done))
                            .unwrap();
                        if id == curr_id {
                            println!(
                                "In val: {:?}, Out val: {:?}",
                                curr_in.data.clone(),
                                Token::<ValType, StopType>::Done
                            );
                        }
                        println!("Reduce count: {}", reduce_count);
                        return;
                    }
                },
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

pub struct Spacc1Data<CrdType: Clone, ValType: Clone, StopType: Clone> {
    pub in_val: Receiver<Token<ValType, StopType>>,
    pub in_crd_outer: Receiver<Token<CrdType, StopType>>,
    pub in_crd_inner: Receiver<Token<CrdType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    pub out_crd_inner: Sender<Token<CrdType, StopType>>,
    /// Stop depth at which this accumulator's reduction fiber ends.
    ///   Stops at depth <  reduction_depth → within reduction (forward, do NOT flush)
    ///   Stops at depth >= reduction_depth → flush state, emit (crd, val) pairs,
    ///                                       then forward Stop unchanged
    /// Default 0 preserves current behavior (every Stop flushes).
    pub reduction_depth: u32,
}

#[context_macro]
pub struct Spacc1<CrdType: Clone, ValType: Clone, StopType: Clone> {
    spacc1_data: Spacc1Data<CrdType, ValType, StopType>,
}

impl<CrdType: DAMType, ValType: DAMType, StopType: DAMType> Spacc1<CrdType, ValType, StopType>
where
    Spacc1<CrdType, ValType, StopType>: Context,
{
    pub fn new(spacc1_data: Spacc1Data<CrdType, ValType, StopType>) -> Self {
        let red = Spacc1 {
            spacc1_data,
            context_info: Default::default(),
        };
        (red.spacc1_data.in_crd_outer).attach_receiver(&red);
        (red.spacc1_data.in_crd_inner).attach_receiver(&red);
        (red.spacc1_data.in_val).attach_receiver(&red);
        (red.spacc1_data.out_crd_inner).attach_sender(&red);
        (red.spacc1_data.out_val).attach_sender(&red);

        red
    }
}

impl<CrdType, ValType, StopType> Context for Spacc1<CrdType, ValType, StopType>
where
    CrdType: DAMType + Hash + std::cmp::Eq + std::cmp::PartialEq + std::cmp::Ord,
    ValType: DAMType
        + std::ops::AddAssign<ValType>
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    StopType: DAMType
        + std::ops::Add<u32, Output = StopType>
        + std::ops::Sub<u32, Output = StopType>
        + std::cmp::PartialEq
        + std::cmp::PartialOrd
        + std::convert::From<u32>,
    // Token<f32, u32>: From<Token<ValType, StopType>>,  // Disabled for block sparse mode
    // Token<u32, u32>: From<Token<CrdType, StopType>>,  // Disabled for block sparse mode
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut accum_storage: BTreeMap<CrdType, ValType> = BTreeMap::new();
        let id = Identifier { id: 0 };
        let id1 = Identifier { id: 0 };
        let mut icrd_stkn_pop_cnt = 0;
        let mut ocrd_val_pop_cnt = 0;
        let mut reduce_count: u64 = 0;
        loop {
            let in_ocrd = self.spacc1_data.in_crd_outer.peek_next(&self.time).unwrap();
            let in_icrd = self.spacc1_data.in_crd_inner.peek_next(&self.time).unwrap();
            let in_val = self.spacc1_data.in_val.peek_next(&self.time).unwrap();

            let matches = match (in_icrd.data.clone(), in_val.data.clone()) {
                (Token::Val(_), Token::Val(_)) => true,
                (Token::Stop(_), Token::Stop(_)) => true,
                (Token::Empty, Token::Empty) => true,
                (Token::Done, Token::Done) => true,
                (_, _) => false,
            };

            if !matches {
                panic!("in_icrd and in_val don't match types");
            }

            match in_ocrd.data.clone() {
                Token::Val(_) => {
                    match in_val.data.clone() {
                        Token::Val(val) => match in_icrd.data.clone() {
                            Token::Val(crd) => {
                                *accum_storage.entry(crd).or_default() += val.clone();
                                reduce_count += 1;
                            }
                            _ => {
                                // self.spacc1_data.in_val.dequeue(&self.time).unwrap();
                                println!("Icrd: {:?}", in_icrd.data.clone());
                                println!("Val: {:?}", in_val.data.clone());
                                println!("Invalid token found in Spacc1");
                                panic!("Exiting spacc");
                                // std::process::exit(1);
                            }
                        },
                        Token::Stop(val_stkn) => match in_icrd.data {
                            Token::Stop(icrd_stkn) => {
                                assert_eq!(val_stkn, icrd_stkn);
                                self.spacc1_data.in_crd_outer.dequeue(&self.time).unwrap();
                                ocrd_val_pop_cnt += 1;
                                icrd_stkn_pop_cnt += 1;
                            }
                            _ => {
                                panic!("Stop tokens must match for inner crd");
                            }
                        },
                        Token::Done => {
                            panic!("Reached Done too soon");
                        }
                        _ => {
                            panic!("Invalid case reached");
                        }
                    }
                    self.spacc1_data.in_crd_inner.dequeue(&self.time).unwrap();
                    self.spacc1_data.in_val.dequeue(&self.time).unwrap();
                }
                Token::Stop(stkn) => {
                    // reduction_depth gates whether this Stop on outer-crd is the
                    // reduction-fiber-end (flush + forward) or within reduction
                    // (just forward, keep accumulating across multi-rank reduction).
                    // Default reduction_depth=0 → every Stop has stkn >= 0 → flushes
                    // (current behavior preserved byte-identically for untiled).
                    let red_depth = StopType::from(self.spacc1_data.reduction_depth);
                    let is_reduction_end = stkn >= red_depth;
                    if !is_reduction_end {
                        // Within multi-rank reduction: consume the outer-crd Stop
                        // silently and keep the accumulator alive for further tile
                        // contributions. Do NOT emit anything on the output channels
                        // — the Stop only marks an inner-tile boundary in the *input*
                        // stream; the output rank structure does not include this
                        // boundary. (Mirrors Reduce's within-reduction skip.)
                        self.spacc1_data.in_crd_outer.dequeue(&self.time).unwrap();
                        ocrd_val_pop_cnt += 1;
                        continue;
                    }
                    for (key, value) in &accum_storage {
                        let icrd_chan_elem = ChannelElement::new(
                            self.time.tick() + 1,
                            // Token::Val(accum_storage.keys().next().unwrap().clone()),
                            Token::Val(key.clone()),
                        );
                        self.spacc1_data
                            .out_crd_inner
                            .enqueue(&self.time, icrd_chan_elem)
                            .unwrap();
                        let val_chan_elem = ChannelElement::new(
                            self.time.tick() + 1,
                            Token::<ValType, StopType>::Val(value.clone()),
                        );
                        self.spacc1_data
                            .out_val
                            .enqueue(&self.time, val_chan_elem)
                            .unwrap();
                        // Logging disabled for block sparse mode
                        // let _ = dam::logging::log_event(&SpaccLog {
                        //     out_val: Token::Val(value.clone()).into(),
                        //     out_crd: Token::Val(key.clone()).into(),
                        // });
                        if self.id() == id.clone() || self.id() == id1.clone() {
                            // println!("Id: {:?}", self.id());
                            println!();
                            println!("Icrd: {:?}", in_icrd.data.clone());
                            println!("Ocrd: {:?}", in_ocrd.data.clone());
                            println!(
                                "Out Val: {:?}",
                                Token::<ValType, StopType>::Val(value.clone())
                            );
                            println!(
                                "Out crd: {:?}",
                                Token::<CrdType, StopType>::Val(key.clone())
                            );
                        }
                    }
                    let val_stkn_chan_elem =
                        ChannelElement::new(self.time.tick() + 1, Token::Stop(stkn.clone()));
                    self.spacc1_data
                        .out_val
                        .enqueue(&self.time, val_stkn_chan_elem.clone())
                        .unwrap();
                    let crd_stkn_chan_elem =
                        ChannelElement::new(self.time.tick() + 1, Token::Stop(stkn.clone()));
                    self.spacc1_data
                        .out_crd_inner
                        .enqueue(&self.time, crd_stkn_chan_elem)
                        .unwrap();
                    // Logging disabled for block sparse mode
                    // let _ = dam::logging::log_event(&SpaccLog {
                    //     out_val: Token::<ValType, StopType>::Stop(stkn.clone()).into(),
                    //     out_crd: Token::<CrdType, StopType>::Stop(stkn.clone()).into(),
                    // });
                    accum_storage.clear();
                    if self.id() == id.clone() || self.id() == id1.clone() {
                        // println!("Id: {:?}", self.id());
                        println!();
                        println!("Icrd: {:?}", in_icrd.data.clone());
                        println!("Ocrd: {:?}", in_ocrd.data.clone());
                        println!(
                            "Out Val: {:?}",
                            Token::<ValType, StopType>::Stop(stkn.clone())
                        );
                        println!(
                            "Out Crd: {:?}",
                            Token::<ValType, StopType>::Stop(stkn.clone())
                        );
                    }
                    self.spacc1_data.in_crd_outer.dequeue(&self.time).unwrap();
                    // Handle the case with back to back stop tokens

                    if let Token::Stop(inner_stkn) = in_icrd.data.clone() {
                        let next_ocrd =
                            self.spacc1_data.in_crd_outer.peek_next(&self.time).unwrap();
                        if let Token::Stop(ocrd_stkn) = next_ocrd.data.clone() {
                            if inner_stkn == ocrd_stkn.clone() + 1 {
                                self.spacc1_data.in_crd_inner.dequeue(&self.time).unwrap();
                                self.spacc1_data.in_val.dequeue(&self.time).unwrap();
                            } else {
                                println!(
                                    "Outer: {:?}, Inner: {:?}",
                                    ocrd_stkn.clone(),
                                    inner_stkn.clone()
                                );
                                println!("Inner and outer stop token types don't match");
                            }
                        } else {
                            assert_eq!(
                                inner_stkn,
                                StopType::default(),
                                "Inner stkn lvl should be 0"
                            );
                        }
                    }
                }
                Token::Done => {
                    match in_icrd.data.clone() {
                        Token::Done => {
                            let icrd_chan_elem =
                                ChannelElement::new(self.time.tick() + 1, Token::Done);
                            self.spacc1_data
                                .out_crd_inner
                                .enqueue(&self.time, icrd_chan_elem)
                                .unwrap();
                            let val_chan_elem =
                                ChannelElement::new(self.time.tick() + 1, Token::Done);
                            self.spacc1_data
                                .out_val
                                .enqueue(&self.time, val_chan_elem)
                                .unwrap();
                            // Logging disabled for block sparse mode
                            // let _ = dam::logging::log_event(&SpaccLog {
                            //     out_val: Token::<ValType, StopType>::Done.into(),
                            //     out_crd: Token::<CrdType, StopType>::Done.into(),
                            // });
                            println!("Reduce count: {}", reduce_count);
                            return;
                        }
                        _ => {
                            if self.id() == id.clone() {
                                println!("Icrd: {:?}", in_icrd.data.clone());
                                println!("Ival: {:?}", in_val.data.clone());
                            }
                            match in_icrd.data.clone() {
                                Token::Stop(_) => {
                                    icrd_stkn_pop_cnt += 1;
                                }
                                _ => {}
                            }
                            self.spacc1_data.in_crd_inner.dequeue(&self.time).unwrap();
                            self.spacc1_data.in_val.dequeue(&self.time).unwrap();
                        }
                    }

                    // if self.id() == id.clone() || self.id() == id1.clone() {
                    //     // println!("Id: {:?}", self.id());
                    //     println!();
                    //     println!("Icrd: {:?}", in_icrd.data.clone());
                    //     println!("Ocrd: {:?}", in_ocrd.data.clone());
                    //     println!("Out Val: {:?}", Token::<ValType, StopType>::Done);
                    //     println!("Out Crd: {:?}", Token::<ValType, StopType>::Done);
                    // }
                }
                _ => {
                    println!("Unexpected empty token found in spacc");
                    panic!();
                    // std::process::exit(1);
                }
            }
            // println!("icrd cnt: {}, ocrd cnt: {}", icrd_stkn_pop_cnt, ocrd_val_pop_cnt);
            self.time.incr_cycles(1);
        }
    }
}

pub struct Spacc2Data<CrdType: Clone, ValType: Clone, StopType: Clone> {
    pub in_val: Receiver<Token<ValType, StopType>>,
    pub in_crd0: Receiver<Token<CrdType, StopType>>,
    pub in_crd1: Receiver<Token<CrdType, StopType>>,
    pub in_crd2: Receiver<Token<CrdType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    pub out_crd0: Sender<Token<CrdType, StopType>>,
    pub out_crd1: Sender<Token<CrdType, StopType>>,
    /// See Spacc1Data::reduction_depth — same semantics.
    pub reduction_depth: u32,
}

#[context_macro]
pub struct Spacc2<CrdType: Clone, ValType: Clone, StopType: Clone> {
    spacc2_data: Spacc2Data<CrdType, ValType, StopType>,
}

impl<CrdType: DAMType, ValType: DAMType, StopType: DAMType> Spacc2<CrdType, ValType, StopType>
where
    Spacc2<CrdType, ValType, StopType>: Context,
{
    pub fn new(spacc2_data: Spacc2Data<CrdType, ValType, StopType>) -> Self {
        let red = Spacc2 {
            spacc2_data,
            context_info: Default::default(),
        };
        (red.spacc2_data.in_crd0).attach_receiver(&red);
        (red.spacc2_data.in_crd1).attach_receiver(&red);
        (red.spacc2_data.in_crd2).attach_receiver(&red);
        (red.spacc2_data.in_val).attach_receiver(&red);
        (red.spacc2_data.out_crd0).attach_sender(&red);
        (red.spacc2_data.out_crd1).attach_sender(&red);
        (red.spacc2_data.out_val).attach_sender(&red);

        red
    }
}

impl<CrdType, ValType, StopType> Context for Spacc2<CrdType, ValType, StopType>
where
    CrdType: DAMType + Hash + std::cmp::Eq + std::cmp::PartialEq + std::cmp::Ord,
    ValType: DAMType
        + std::ops::AddAssign<ValType>
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    StopType: DAMType
        + std::ops::Add<u32, Output = StopType>
        + std::ops::Sub<u32, Output = StopType>
        + std::cmp::PartialEq
        + std::cmp::PartialOrd
        + std::convert::From<u32>,
    // Token<f32, u32>: From<Token<ValType, StopType>>,  // Disabled for block sparse mode
    // Token<u32, u32>: From<Token<CrdType, StopType>>,  // Disabled for block sparse mode
    CrdType: PartialEq<CrdType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut accum_storage: BTreeMap<CrdType, BTreeMap<CrdType, ValType>> = BTreeMap::new();
        let id = Identifier { id: 0 };
        let id1 = Identifier { id: 0 };
        let mut icrd_stkn_pop_cnt = 0;
        let mut ocrd_val_pop_cnt = 0;
        loop {
            let in_crd2 = self.spacc2_data.in_crd2.peek_next(&self.time).unwrap();
            let in_crd1 = self.spacc2_data.in_crd1.peek_next(&self.time).unwrap();
            let in_crd0 = self.spacc2_data.in_crd0.peek_next(&self.time).unwrap();
            let in_val = self.spacc2_data.in_val.peek_next(&self.time).unwrap();

            let matches = match (in_crd0.data.clone(), in_val.data.clone()) {
                (Token::Val(_), Token::Val(_)) => true,
                (Token::Stop(_), Token::Stop(_)) => true,
                (Token::Empty, Token::Empty) => true,
                (Token::Done, Token::Done) => true,
                (_, _) => false,
            };

            if !matches {
                panic!("in_icrd and in_val don't match types");
            }

            // match self.spacc2_data.in_crd2.dequeue(&self.time).unwrap().data {
            match in_crd2.data.clone() {
                // Acc1 body
                Token::Val(_) => {
                    // let curr_in_crd1 = self.spacc2_data.in_crd1.dequeue(&self.time).unwrap().data;
                    match in_crd1.data.clone() {
                        Token::Val(crd1) => {
                            let in_crd0_deq =
                                self.spacc2_data.in_crd0.dequeue(&self.time).unwrap().data;
                            let in_val_deq =
                                self.spacc2_data.in_val.dequeue(&self.time).unwrap().data;
                            match in_val_deq.clone() {
                                Token::Val(val) => {
                                    // Needs assert to make sure crd0 is also a nc token
                                    if let Token::Val(crd0) = in_crd0_deq {
                                        // accum_storage.entry(crd1).entry(crd0).or_default() += val.clone();
                                        match accum_storage.entry(crd1) {
                                            std::collections::btree_map::Entry::Vacant(
                                                vacant_entry,
                                            ) => {
                                                let mut inner_map = BTreeMap::new();
                                                *inner_map.entry(crd0).or_default() += val;
                                                vacant_entry.insert(inner_map);
                                            }
                                            std::collections::btree_map::Entry::Occupied(
                                                mut occupied_entry,
                                            ) => {
                                                *occupied_entry
                                                    .get_mut()
                                                    .entry(crd0)
                                                    .or_default() += val;
                                            }
                                        }
                                    } else {
                                        panic!("Inner crd needs to match value stream type")
                                    }
                                }
                                Token::Stop(_) => {
                                    self.spacc2_data.in_crd1.dequeue(&self.time).unwrap();
                                }
                                Token::Done => panic!("Shouldn't have done for coords yet"),
                                _ => panic!("Should not reach any other case"),
                            }
                        }
                        Token::Stop(_) => {
                            self.spacc2_data.in_crd1.dequeue(&self.time).unwrap();
                            self.spacc2_data.in_crd2.dequeue(&self.time).unwrap();
                        }
                        Token::Empty => panic!("empty"),
                        Token::Done => panic!("Done on crd 1 match"),
                    }
                }
                Token::Stop(stkn) => {
                    for (key, crd0_map) in &accum_storage {
                        // Flush out crd1 in order
                        let icrd_chan_elem =
                            ChannelElement::new(self.time.tick() + 1, Token::Val(key.clone()));
                        self.spacc2_data
                            .out_crd1
                            .enqueue(&self.time, icrd_chan_elem)
                            .unwrap();
                        for (crd0, val) in crd0_map {
                            // For each crd1, flush out crd0 and value
                            let val_chan_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::<ValType, StopType>::Val(val.clone()),
                            );
                            self.spacc2_data
                                .out_val
                                .enqueue(&self.time, val_chan_elem)
                                .unwrap();
                            let crd0_chan_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::<CrdType, StopType>::Val(crd0.clone()),
                            );
                            self.spacc2_data
                                .out_crd0
                                .enqueue(&self.time, crd0_chan_elem)
                                .unwrap();
                        }

                        let mut emitted_stop_crd = Token::Stop(StopType::default());
                        let mut emitted_stop_val = Token::Stop(StopType::default());
                        let (last_key, _) = accum_storage.iter().next_back().unwrap();
                        if last_key == key {
                            emitted_stop_crd = Token::<CrdType, StopType>::Stop(stkn.clone() + 1);
                            emitted_stop_val = Token::<ValType, StopType>::Stop(stkn.clone() + 1);
                        }

                        let val_stkn_chan_elem =
                            ChannelElement::new(self.time.tick() + 1, emitted_stop_val.clone());
                        self.spacc2_data
                            .out_val
                            .enqueue(&self.time, val_stkn_chan_elem.clone())
                            .unwrap();
                        let crd0_stkn_chan_elem =
                            ChannelElement::new(self.time.tick() + 1, emitted_stop_crd.clone());
                        self.spacc2_data
                            .out_crd0
                            .enqueue(&self.time, crd0_stkn_chan_elem)
                            .unwrap();
                    }

                    let crd1_stkn_chan_elem =
                        ChannelElement::new(self.time.tick() + 1, Token::Stop(stkn.clone()));
                    self.spacc2_data
                        .out_crd1
                        .enqueue(&self.time, crd1_stkn_chan_elem)
                        .unwrap();

                    accum_storage.clear();
                    self.spacc2_data.in_crd2.dequeue(&self.time).unwrap();
                    // if self.id() == id.clone() || self.id() == id1.clone() {
                    //     // println!("Id: {:?}", self.id());
                    //     println!();
                    //     println!("Icrd: {:?}", in_icrd.data.clone());
                    //     println!("Ocrd: {:?}", in_ocrd.data.clone());
                    //     println!(
                    //         "Out Val: {:?}",
                    //         Token::<ValType, StopType>::Stop(stkn.clone())
                    //     );
                    //     println!(
                    //         "Out Crd: {:?}",
                    //         Token::<ValType, StopType>::Stop(stkn.clone())
                    //     );
                    // }
                    // self.spacc1_data.in_crd_outer.dequeue(&self.time).unwrap();

                    // Handle the case with back to back stop tokens
                    // if let Token::Stop(inner_stkn) = in_crd0.data.clone() {
                    //     let next_ocrd = self.spacc2_data.in_crd1.peek_next(&self.time).unwrap();
                    //     if let Token::Stop(ocrd_stkn) = next_ocrd.data.clone() {
                    //         if inner_stkn == ocrd_stkn.clone() + 1 {
                    //             self.spacc2_data.in_crd0.dequeue(&self.time).unwrap();
                    //             self.spacc2_data.in_val.dequeue(&self.time).unwrap();
                    //         } else {
                    //             println!(
                    //                 "Outer: {:?}, Inner: {:?}",
                    //                 ocrd_stkn.clone(),
                    //                 inner_stkn.clone()
                    //             );
                    //             println!("Inner and outer stop token types don't match");
                    //         }
                    //     } else {
                    //         assert_eq!(
                    //             inner_stkn,
                    //             StopType::default(),
                    //             "Inner stkn lvl should be 0"
                    //         );
                    //     }
                    // }
                }
                Token::Done => {
                    match in_crd1.data.clone() {
                        Token::Val(_) => panic!("Val crd1 in done"),
                        Token::Stop(_) => {
                            self.spacc2_data.in_crd1.dequeue(&self.time).unwrap();
                            println!("Reached stop crd1");
                        }
                        Token::Empty => panic!("Empty crd1 in done"),
                        Token::Done => match in_crd0.data.clone() {
                            Token::Done => {
                                println!(
                                    "{:?}, {:?}, {:?}",
                                    in_crd0.data.clone(),
                                    in_crd1.data.clone(),
                                    in_val.data.clone()
                                );
                                let icrd_chan_elem =
                                    ChannelElement::new(self.time.tick() + 1, Token::Done);
                                self.spacc2_data
                                    .out_crd0
                                    .enqueue(&self.time, icrd_chan_elem)
                                    .unwrap();
                                let crd1_chan_elem =
                                    ChannelElement::new(self.time.tick() + 1, Token::Done);
                                self.spacc2_data
                                    .out_crd1
                                    .enqueue(&self.time, crd1_chan_elem)
                                    .unwrap();
                                let val_chan_elem =
                                    ChannelElement::new(self.time.tick() + 1, Token::Done);
                                self.spacc2_data
                                    .out_val
                                    .enqueue(&self.time, val_chan_elem)
                                    .unwrap();
                                // Logging disabled for block sparse mode
                                // let _ = dam::logging::log_event(&SpaccLog {
                                //     out_val: Token::<ValType, StopType>::Done.into(),
                                //     out_crd: Token::<CrdType, StopType>::Done.into(),
                                // });
                                return;
                            }
                            _ => {
                                if self.id() == id.clone() {
                                    println!("Icrd: {:?}", in_crd0.data.clone());
                                    println!("Ival: {:?}", in_val.data.clone());
                                }
                                match in_crd0.data.clone() {
                                    Token::Stop(_) => {
                                        icrd_stkn_pop_cnt += 1;
                                    }
                                    _ => {}
                                }
                                panic!("Should not reach here");
                                self.spacc2_data.in_crd0.dequeue(&self.time).unwrap();
                                self.spacc2_data.in_val.dequeue(&self.time).unwrap();
                            }
                        },
                    }

                    // if self.id() == id.clone() || self.id() == id1.clone() {
                    //     // println!("Id: {:?}", self.id());
                    //     println!();
                    //     println!("Icrd: {:?}", in_icrd.data.clone());
                    //     println!("Ocrd: {:?}", in_ocrd.data.clone());
                    //     println!("Out Val: {:?}", Token::<ValType, StopType>::Done);
                    //     println!("Out Crd: {:?}", Token::<ValType, StopType>::Done);
                    // }
                }
                _ => {
                    println!("Unexpected empty token found in spacc");
                    panic!();
                    // std::process::exit(1);
                }
            }
            // println!("icrd cnt: {}, ocrd cnt: {}", icrd_stkn_pop_cnt, ocrd_val_pop_cnt);
            self.time.incr_cycles(1);
        }
    }
}

// MaxReduceData - separate struct for MaxReduce that doesn't need const N or sum
pub struct MaxReduceData<ValType: Clone, StopType: Clone> {
    pub in_val: Receiver<Token<ValType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
}

#[context_macro]
pub struct MaxReduce<ValType: Clone, StopType: Clone> {
    max_reduce_data: MaxReduceData<ValType, StopType>,
    min_val: ValType,
}

impl<ValType: DAMType, StopType: DAMType> MaxReduce<ValType, StopType>
where
    MaxReduce<ValType, StopType>: Context,
{
    pub fn new(max_reduce_data: MaxReduceData<ValType, StopType>, min_val: ValType) -> Self {
        let red = MaxReduce {
            max_reduce_data,
            min_val,
            context_info: Default::default(),
        };
        (red.max_reduce_data.in_val).attach_receiver(&red);
        (red.max_reduce_data.out_val).attach_sender(&red);

        red
    }
}

impl<ValType, StopType> Context for MaxReduce<ValType, StopType>
where
    ValType: DAMType
        + std::ops::AddAssign<ValType>
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    StopType: DAMType
        + std::ops::Add<u32, Output = StopType>
        + std::ops::Sub<u32, Output = StopType>
        + std::cmp::PartialEq,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut max_elem = self.min_val.clone();
        let mut reduce_count: u64 = 0;
        loop {
            match self.max_reduce_data.in_val.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        reduce_count += 1;
                        match val.lt(&max_elem) {
                            true => (),
                            false => max_elem = val,
                        }
                    }
                    Token::Stop(stkn) => {
                        let curr_time = self.time.tick();
                        self.max_reduce_data
                            .out_val
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + 1, Token::Val(max_elem)),
                            )
                            .unwrap();
                        max_elem = ValType::default();
                        if stkn != StopType::default() {
                            self.max_reduce_data
                                .out_val
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Stop(stkn - 1)),
                                )
                                .unwrap();
                        }
                    }
                    Token::Empty => {
                        continue;
                    }
                    Token::Done => {
                        let curr_time = self.time.tick();
                        self.max_reduce_data
                            .out_val
                            .enqueue(&self.time, ChannelElement::new(curr_time + 1, Token::Done))
                            .unwrap();
                        // println!("Reduce count: {}", reduce_count);
                        return;
                    }
                },
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

// ---------------------------------------------------------------------------
// FlashSoftmaxAccum: streaming online softmax + weighted V accumulation.
// Implements Flash Attention's single-pass algorithm:
//   For each (score, v_vec) token:
//     m' = max(m, score)
//     correction = exp(m - m')
//     p  = exp(score - m')
//     l  = l * correction + p
//     O  = O * correction + p * v_vec
//   On Stop (end of query row fiber):
//     emit O / l
//     reset state
//
// This fuses what would be 6 separate SAM ops (max_reduce, sub, exp, mul,
// reduce_sum, spacc) into a single streaming node with zero pipeline stalls.
// No intermediate buffering. O(1) memory. Full throughput.
// ---------------------------------------------------------------------------

pub struct FlashSoftmaxAccumData<ValType: Clone, StopType: Clone> {
    pub in_score: Receiver<Token<f32, StopType>>,    // scalar attention scores
    pub in_val: Receiver<Token<ValType, StopType>>,   // V elements (head_dim per score, with inner Stops)
    pub out_val: Sender<Token<ValType, StopType>>,    // output O elements (emitted on outer Stop)
}

#[context_macro]
pub struct FlashSoftmaxAccum<ValType: Clone, StopType: Clone> {
    data: FlashSoftmaxAccumData<ValType, StopType>,
    /// Number of V elements per score. 1 = scalar (1:1 score:V), >1 = vector (two-rate).
    /// When >1, V stream has inner Stop tokens between K positions.
    head_dim: usize,
}

impl<ValType: DAMType, StopType: DAMType> FlashSoftmaxAccum<ValType, StopType>
where
    FlashSoftmaxAccum<ValType, StopType>: Context,
{
    pub fn new(data: FlashSoftmaxAccumData<ValType, StopType>) -> Self {
        Self::with_head_dim(data, 1)
    }

    /// Create with explicit head_dim. When head_dim > 1, expects head_dim V elements
    /// per score, with inner Stop tokens in the V stream between K positions.
    pub fn with_head_dim(data: FlashSoftmaxAccumData<ValType, StopType>, head_dim: usize) -> Self {
        let ctx = FlashSoftmaxAccum {
            data,
            head_dim: head_dim.max(1),
            context_info: Default::default(),
        };
        ctx.data.in_score.attach_receiver(&ctx);
        ctx.data.in_val.attach_receiver(&ctx);
        ctx.data.out_val.attach_sender(&ctx);
        ctx
    }
}

impl<ValType, StopType> Context for FlashSoftmaxAccum<ValType, StopType>
where
    ValType: DAMType
        + std::ops::Mul<f32, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>,
    StopType: DAMType
        + std::ops::Add<u32, Output = StopType>
        + std::ops::Sub<u32, Output = StopType>
        + std::cmp::PartialEq,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut m: f32 = f32::NEG_INFINITY;
        let mut l: f32 = 0.0;
        let mut o: Option<ValType> = None;
        let mut token_count: u64 = 0;
        let mut score_count: u64 = 0;
        let hd = self.head_dim;

        loop {
            // Dequeue one score
            let score_elem = match self.data.in_score.dequeue(&self.time) {
                Ok(elem) => elem,
                Err(_) => panic!("FlashSoftmaxAccum: unexpected end of score stream"),
            };

            match score_elem.data {
                Token::Val(score) => {
                    // Online softmax update (once per K position)
                    let m_new = score.max(m);
                    let correction = if m == f32::NEG_INFINITY { 0.0_f32 } else { (m - m_new).exp() };
                    let p = (score - m_new).exp();
                    l = l * correction + p;

                    // Rescale O by correction (once per score)
                    o = o.map(|prev| prev * correction);
                    m = m_new;
                    score_count += 1;

                    // Consume head_dim V elements for this score
                    for _d in 0..hd {
                        let v_elem = self.data.in_val.dequeue(&self.time).unwrap();
                        match v_elem.data {
                            Token::Val(v_val) => {
                                let weighted_v = v_val * p;
                                o = Some(match o {
                                    Some(prev) => prev + weighted_v,
                                    None => weighted_v,
                                });
                                token_count += 1;
                            }
                            _ => panic!("FlashSoftmaxAccum: expected Val in V inner dim, got control token"),
                        }
                        self.time.incr_cycles(1);
                    }

                    // If head_dim > 1, try to consume the inner Stop(0) from V stream.
                    // But only consume Stop(0) (inner K-position boundary).
                    // Stop(≥1) is the row boundary — leave it for the outer logic.
                    if hd > 1 {
                        match self.data.in_val.peek() {
                            dam::channel::PeekResult::Something(ref ce) => {
                                match &ce.data {
                                    Token::Stop(stkn) if *stkn == StopType::default() => {
                                        // Inner Stop(0) — consume it
                                        self.data.in_val.dequeue(&self.time).unwrap();
                                    }
                                    _ => {
                                        // Not an inner Stop — either a Val (shouldn't happen)
                                        // or a higher-level Stop (row boundary). Don't consume.
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Token::Stop(stkn) => {
                    // End of query row — emit O / l
                    // Consume matching Stop from V stream
                    let v_stop = self.data.in_val.dequeue(&self.time).unwrap();
                    match v_stop.data {
                        Token::Stop(_) => {}
                        other => {
                            std::fs::write("/tmp/flash_debug.txt", format!(
                                "V got {:?}\nscores_seen={}, V_tokens={}, head_dim={}\nm={}, l={}\n",
                                other, score_count, token_count, hd, m, l)).ok();
                            panic!("FlashSoftmaxAccum: misaligned score/V streams");
                        }
                    }

                    if let Some(accum) = o.take() {
                        let inv_l = if l > 0.0 { 1.0 / l } else { 0.0 };
                        let result = accum * inv_l;
                        self.data.out_val.enqueue(
                            &self.time,
                            ChannelElement::new(self.time.tick() + 1, Token::Val(result)),
                        ).unwrap();
                    }

                    if stkn != StopType::default() {
                        self.data.out_val.enqueue(
                            &self.time,
                            ChannelElement::new(self.time.tick() + 1, Token::Stop(stkn - 1)),
                        ).unwrap();
                    }

                    m = f32::NEG_INFINITY;
                    l = 0.0;
                    o = None;
                }
                Token::Done => {
                    let _ = self.data.in_val.dequeue(&self.time);
                    self.data.out_val.enqueue(
                        &self.time,
                        ChannelElement::new(self.time.tick() + 1, Token::Done),
                    ).unwrap();
                    println!("FlashSoftmaxAccum: processed {} tokens (head_dim={})", token_count, hd);
                    return;
                }
                Token::Empty => {}
            }
            self.time.incr_cycles(1);
        }
    }
}

#[cfg(test)]
mod tests {

    use dam::simulation::*;
    use dam::utility_contexts::*;

    use crate::templates::accumulator::Spacc2;
    use crate::templates::accumulator::Spacc2Data;
    use crate::templates::primitive::Token;
    use crate::token_vec;

    use super::{FlashSoftmaxAccum, FlashSoftmaxAccumData, MaxReduce, MaxReduceData, Reduce, Spacc1};
    use super::{ReduceData, Spacc1Data};

    #[test]
    fn reduce_2d_test() {
        let in_val = || {
            token_vec!(u32; u32; 5, 5, "S0", 5, "S0", 4, 8, "S0", 4, 3, "S0", 4, 3, "S1", "D")
                .into_iter()
        };
        let out_val = || token_vec!(u32; u32; 10, 5, 12, 7, 7, "S0", "D").into_iter();
        reduce_test(in_val, out_val);
    }

    #[test]
    fn reduce_2d_test1() {
        let in_val = || {
            token_vec!(u32; u32; "S0", "S1", "S1", "S2", 2, "S0", "S1", "S1", "S2", "S0", 3, "S1", "S1", "S3", "D").into_iter()
        };
        let out_val = || token_vec!(u32; u32; 10, 5, 12, 7, 7, "S0", "D").into_iter();
        reduce_test(in_val, out_val);
    }

    #[test]
    fn spacc1_2d_test() {
        let in_ocrd = || token_vec!(u32; u32; 0, 2, "S0", 2, "S1", "D").into_iter();
        let in_icrd =
            || token_vec!(u32; u32; 0, 2, 3, "S0", 0, 2, 3, "S1", 0, 2, 3, "S2", "D").into_iter();
        let in_val = || {
            token_vec!(f32; u32; 50.0, 5.0, 10.0, "S0", 40.0, 4.0, 8.0, "S1", -40.0, 33.0, 36.0, "S2", "D")
                    .into_iter()
        };
        let out_icrd = || token_vec!(u32; u32; 0, 2, 3, "S0", 0, 2, 3, "S1", "D").into_iter();
        let out_val = || {
            token_vec!(f32; u32; 90.0, 9.0, 18.0, "S0", -40.0, 33.0, 36.0, "S1", "D").into_iter()
        };
        spacc1_test(in_ocrd, in_icrd, in_val, out_icrd, out_val);
    }

    #[test]
    fn spacc1_2d_test1() {
        let in_ocrd = || {
            token_vec!(u32; u32; 0,"S0",1,"S0",2,"S0",3,"S0",4,"S0",5,"S0",6,"S0",7,"S0",6,"S0",0,"S0",2,"S0",3,"S0",1,"S0",5,"S1","D").into_iter()
        };
        let in_icrd = || {
            token_vec!(u32; u32; 478,479,480,481,482,483,484,485,486,487,488,489,490,491,492,"S1",0,346,"S1",1,696,"S1",2,353,"S1",3,666,"S1",4,"S1",5,699,"S1",6,22,"S1",5,699,"S1",478,479,480,481,482,483,484,485,486,487,488,489,490,491,492,"S1",1,696,"S1",2,353,"S1",0,346,"S1",4,"S2","D").into_iter()
        };
        let in_val = || {
            token_vec!(f32; u32; 2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,"S1",2.0,2.0,"S1",2.0,2.0,"S1",2.0,2.0,"S1",2.0,2.0,"S1",2.0,"S1",2.0,2.0,"S1",2.0,2.0,"S1",2.0,2.0,"S1",2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,"S1",2.0,2.0,"S1",2.0,2.0,"S1",2.0,2.0,"S1",2.0,"S2","D")
                    .into_iter()
        };
        let out_icrd = || token_vec!(u32; u32; 0, 2, 3, "S0", 0, 2, 3, "S1", "D").into_iter();
        let out_val = || {
            token_vec!(f32; u32; 90.0, 9.0, 18.0, "S0", -40.0, 33.0, 36.0, "S1", "D").into_iter()
        };
        spacc1_test(in_ocrd, in_icrd, in_val, out_icrd, out_val);
    }

    #[test]
    fn max_reduce_2d_test() {
        let in_val = || {
            token_vec!(f32; u32; 5.0, 5.0, "S0", 5.0, "S0", 4.0, 8.0, "S0", 4.0, 3.0, "S0", 4.0, 3.0, "S1", "D")
                .into_iter()
        };
        let out_val = || token_vec!(f32; u32; 5.0, 5.0, 8.0, 4.0, 4.0, "S0", "D").into_iter();
        max_reduce_test(in_val, out_val);
    }

    fn reduce_test<IRT, ORT>(in_val: fn() -> IRT, out_val: fn() -> ORT)
    where
        IRT: Iterator<Item = Token<u32, u32>> + 'static,
        ORT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_val_sender, in_val_receiver) = parent.unbounded();
        let (out_val_sender, out_val_receiver) = parent.unbounded();
        let data = ReduceData::<u32, u32, 1> {
            in_val: in_val_receiver,
            out_val: out_val_sender,
            sum: false,
            reduction_depth: 0,
        };
        let red = Reduce::<u32, u32, 1>::new(data);
        let gen1 = GeneratorContext::new(in_val, in_val_sender);
        let val_checker = PrinterContext::new(out_val_receiver);
        // let val_checker = CheckerContext::new(out_val, out_val_receiver);
        parent.add_child(gen1);
        parent.add_child(val_checker);
        parent.add_child(red);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }

    #[test]
    fn spacc2_2d_test() {
        let in_crd2 = || token_vec!(u32; u32; 0, 1, "S0", "D").into_iter();
        let in_crd1 = || token_vec!(u32; u32; 0, 2, "S0", 2, "S1", "D").into_iter();
        let in_crd0 =
            || token_vec!(u32; u32; 0, 2, 3, "S0", 0, 2, 3, "S1", 0, 2, 3, "S2", "D").into_iter();
        let in_val = || {
            token_vec!(f32; u32; 50.0, 5.0, 10.0, "S0", 40.0, 4.0, 8.0, "S1", -40.0, 33.0, 36.0, "S2", "D")
                .into_iter()
        };
        let out_crd0 = || token_vec!(u32; u32; "S0", "D").into_iter();
        let out_crd1 = || token_vec!(u32; u32; "S0", "D").into_iter();
        let out_val = || token_vec!(f32; u32; 5.0, 5.0, 8.0, 4.0, 4.0, "S0", "D").into_iter();
        spacc2_test(
            in_crd2, in_crd1, in_crd0, in_val, out_crd0, out_crd1, out_val,
        );
    }

    #[test]
    fn spacc2_2d_test2() {
        let in_crd2 = || token_vec!(u32; u32; 0, "S0", "D").into_iter();
        let in_crd1 = || token_vec!(u32; u32; 0, 2, "S1", "D").into_iter();
        let in_crd0 = || token_vec!(u32; u32; 0, 2, 3, "S0", 0, 2, 3, "S2", "D").into_iter();
        let in_val =
            || token_vec!(f32; u32; 50.0, 5.0, 10.0, "S0", 40.0, 4.0, 8.0, "S2", "D").into_iter();
        let out_crd0 = || token_vec!(u32; u32; "S0", "D").into_iter();
        let out_crd1 = || token_vec!(u32; u32; "S0", "D").into_iter();
        let out_val =
            || token_vec!(f32; u32; 50.0, 10.0, "S0", 40.0, 4.0, 8.0, "S1", "D").into_iter();
        spacc2_test(
            in_crd2, in_crd1, in_crd0, in_val, out_crd0, out_crd1, out_val,
        );
    }

    #[test]
    fn spacc2_2d_test3() {
        let in_crd2 = || token_vec!(u32; u32; 0, "S0", "D").into_iter();
        let in_crd1 = || token_vec!(u32; u32; 0, 2, "S1", "D").into_iter();
        let in_crd0 = || token_vec!(u32; u32; 0, 2, 3, "S0", 0, 2, 3, "S2", "D").into_iter();
        let in_val =
            || token_vec!(f32; u32; 50.0, 5.0, 10.0, "S0", 40.0, 4.0, 8.0, "S2", "D").into_iter();
        let out_crd0 = || token_vec!(u32; u32; "S0", "D").into_iter();
        let out_crd1 = || token_vec!(u32; u32; "S0", "D").into_iter();
        let out_val =
            || token_vec!(f32; u32; 50.0, 10.0, "S0", 40.0, 4.0, 8.0, "S1", "D").into_iter();
        spacc2_test(
            in_crd2, in_crd1, in_crd0, in_val, out_crd0, out_crd1, out_val,
        );
    }

    fn spacc2_test<IRT1, IRT2, IRT3, IRT4, ORT1, ORT2>(
        in_crd2: fn() -> IRT1,
        in_crd1: fn() -> IRT2,
        in_crd0: fn() -> IRT3,
        in_val: fn() -> IRT4,
        out_crd0: fn() -> ORT1,
        out_crd1: fn() -> ORT1,
        out_val: fn() -> ORT2,
    ) where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        IRT2: Iterator<Item = Token<u32, u32>> + 'static,
        IRT3: Iterator<Item = Token<u32, u32>> + 'static,
        IRT4: Iterator<Item = Token<f32, u32>> + 'static,
        ORT1: Iterator<Item = Token<u32, u32>> + 'static,
        ORT2: Iterator<Item = Token<f32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_crd0_sender, in_crd0_receiver) = parent.unbounded();
        let (in_crd1_sender, in_crd1_receiver) = parent.unbounded();
        let (in_crd2_sender, in_crd2_receiver) = parent.unbounded();
        let (in_val_sender, in_val_receiver) = parent.unbounded();
        let (out_val_sender, out_val_receiver) = parent.unbounded();
        let (out_crd0_sender, out_crd0_receiver) = parent.unbounded();
        let (out_crd1_sender, out_crd1_receiver) = parent.unbounded();
        let data = Spacc2Data::<u32, f32, u32> {
            in_val: in_val_receiver,
            in_crd0: in_crd0_receiver,
            in_crd1: in_crd1_receiver,
            in_crd2: in_crd2_receiver,
            out_val: out_val_sender,
            out_crd0: out_crd0_sender,
            out_crd1: out_crd1_sender,
            reduction_depth: 0,
        };
        let red = Spacc2::new(data);
        let gen1 = GeneratorContext::new(in_crd2, in_crd2_sender);
        let gen2 = GeneratorContext::new(in_crd1, in_crd1_sender);
        let gen3 = GeneratorContext::new(in_crd0, in_crd0_sender);
        let gen4 = GeneratorContext::new(in_val, in_val_sender);
        let crd0_checker = ConsumerContext::new(out_crd0_receiver);
        let crd1_checker = ConsumerContext::new(out_crd1_receiver);
        let val_checker = PrinterContext::new(out_val_receiver);
        parent.add_child(gen1);
        parent.add_child(gen2);
        parent.add_child(gen3);
        parent.add_child(gen4);
        parent.add_child(crd0_checker);
        parent.add_child(crd1_checker);
        parent.add_child(val_checker);
        parent.add_child(red);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }

    fn spacc1_test<IRT1, IRT2, IRT3, ORT1, ORT2>(
        in_ocrd: fn() -> IRT1,
        in_icrd: fn() -> IRT2,
        in_val: fn() -> IRT3,
        out_icrd: fn() -> ORT1,
        out_val: fn() -> ORT2,
    ) where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        IRT2: Iterator<Item = Token<u32, u32>> + 'static,
        IRT3: Iterator<Item = Token<f32, u32>> + 'static,
        ORT1: Iterator<Item = Token<u32, u32>> + 'static,
        ORT2: Iterator<Item = Token<f32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_ocrd_sender, in_ocrd_receiver) = parent.unbounded();
        let (in_icrd_sender, in_icrd_receiver) = parent.unbounded();
        let (in_val_sender, in_val_receiver) = parent.unbounded();
        let (out_val_sender, out_val_receiver) = parent.unbounded();
        let (out_icrd_sender, out_icrd_receiver) = parent.unbounded();
        let data = Spacc1Data::<u32, f32, u32> {
            in_crd_outer: in_ocrd_receiver,
            in_crd_inner: in_icrd_receiver,
            in_val: in_val_receiver,
            out_val: out_val_sender,
            out_crd_inner: out_icrd_sender,
            reduction_depth: 0,
        };
        let red = Spacc1::new(data);
        let gen1 = GeneratorContext::new(in_ocrd, in_ocrd_sender);
        let gen2 = GeneratorContext::new(in_icrd, in_icrd_sender);
        let gen3 = GeneratorContext::new(in_val, in_val_sender);
        let icrd_checker = PrinterContext::new(out_icrd_receiver);
        let val_checker = PrinterContext::new(out_val_receiver);
        parent.add_child(gen1);
        parent.add_child(gen2);
        parent.add_child(gen3);
        parent.add_child(icrd_checker);
        parent.add_child(val_checker);
        parent.add_child(red);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }

    fn max_reduce_test<IRT, ORT>(in_val: fn() -> IRT, out_val: fn() -> ORT)
    where
        IRT: Iterator<Item = Token<f32, u32>> + 'static,
        ORT: Iterator<Item = Token<f32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_val_sender, in_val_receiver) = parent.unbounded::<Token<f32, u32>>();
        let (out_val_sender, out_val_receiver) = parent.unbounded::<Token<f32, u32>>();
        let data = MaxReduceData::<f32, u32> {
            in_val: in_val_receiver,
            out_val: out_val_sender,
        };
        let red = MaxReduce::new(data, f32::MIN);
        let gen1 = GeneratorContext::new(in_val, in_val_sender);
        let val_checker = CheckerContext::new(out_val, out_val_receiver);

        parent.add_child(gen1);
        parent.add_child(val_checker);
        parent.add_child(red);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }

    // ── FlashSoftmaxAccum tests ──

    /// Verify online softmax against reference: softmax([1, 3, 2]) @ V
    #[test]
    fn flash_softmax_accum_basic() {
        // 3 scores for one query row, then Done
        // scores = [1.0, 3.0, 2.0]
        // V rows  = [[1,0], [0,1], [1,1]]
        // Expected: softmax([1,3,2]) = [0.0900, 0.6652, 0.2447] (approx)
        //   output = 0.0900*[1,0] + 0.6652*[0,1] + 0.2447*[1,1]
        //          = [0.0900+0.2447, 0.6652+0.2447] = [0.3348, 0.9100]
        let scores = vec![1.0_f32, 3.0, 2.0];
        let v_rows: Vec<[f32; 2]> = vec![[1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

        // Compute reference
        let max_s = scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp_s: Vec<f32> = scores.iter().map(|s| (s - max_s).exp()).collect();
        let sum_exp: f32 = exp_s.iter().sum();
        let weights: Vec<f32> = exp_s.iter().map(|e| e / sum_exp).collect();
        let ref_out = [
            weights[0] * v_rows[0][0] + weights[1] * v_rows[1][0] + weights[2] * v_rows[2][0],
            weights[0] * v_rows[0][1] + weights[1] * v_rows[1][1] + weights[2] * v_rows[2][1],
        ];

        let mut parent = ProgramBuilder::default();
        let (score_snd, score_rcv) = parent.unbounded::<Token<f32, u32>>();
        let (v_snd, v_rcv) = parent.unbounded::<Token<f32, u32>>();
        let (out_snd, out_rcv) = parent.unbounded::<Token<f32, u32>>();

        let data = FlashSoftmaxAccumData {
            in_score: score_rcv,
            in_val: v_rcv,
            out_val: out_snd,
        };
        let accum = FlashSoftmaxAccum::new(data);

        // Generate score stream: Val(1), Val(3), Val(2), Stop(0), Done
        let score_gen = GeneratorContext::new(
            move || {
                vec![
                    Token::Val(1.0_f32),
                    Token::Val(3.0),
                    Token::Val(2.0),
                    Token::Stop(0_u32),
                    Token::Done,
                ]
                .into_iter()
            },
            score_snd,
        );

        // Generate V stream: interleaved [v0_dim0, v0_dim1], [v1_dim0, v1_dim1], ...
        // For scalar ValType=f32, we flatten V rows into the stream.
        // Each score maps to one V value (scalar attention, not vectorized).
        // For this test: V is scalar, so V = [1.0, 0.0, 1.0] (just first dim)
        // Actually, with scalar f32, each (score, v) pair is one element.
        // So we need 2 query rows: first for dim0, second for dim1.
        // OR: use a single row with V = scalar values.
        //
        // Simplification: test with scalar V (d_v = 1).
        // scores = [1, 3, 2], V = [10, 20, 30]
        // output = softmax([1,3,2]) . [10,20,30]
        //        = 0.0900*10 + 0.6652*20 + 0.2447*30
        //        = 0.900 + 13.305 + 7.342 = 21.547

        // Actually let me just redo this cleanly with scalar V.
        drop(parent); // restart

        let scores_data = vec![1.0_f32, 3.0, 2.0];
        let v_data = vec![10.0_f32, 20.0, 30.0];

        // Compute reference using the SAME online algorithm (in f32) to match exactly
        let mut m: f32 = f32::NEG_INFINITY;
        let mut l_ref: f32 = 0.0;
        let mut o_ref: f32 = 0.0;
        for (&s, &vi) in scores_data.iter().zip(v_data.iter()) {
            let m_new = m.max(s);
            let corr = if m == f32::NEG_INFINITY { 0.0_f32 } else { (m - m_new).exp() };
            let p = (s - m_new).exp();
            l_ref = l_ref * corr + p;
            o_ref = o_ref * corr + p * vi;
            m = m_new;
        }
        let ref_scalar: f32 = o_ref / l_ref;

        let mut parent = ProgramBuilder::default();
        let (score_snd, score_rcv) = parent.unbounded::<Token<f32, u32>>();
        let (v_snd, v_rcv) = parent.unbounded::<Token<f32, u32>>();
        let (out_snd, out_rcv) = parent.unbounded::<Token<f32, u32>>();

        let data = FlashSoftmaxAccumData {
            in_score: score_rcv,
            in_val: v_rcv,
            out_val: out_snd,
        };
        let accum = FlashSoftmaxAccum::new(data);

        let score_gen = GeneratorContext::new(
            move || {
                vec![
                    Token::<f32, u32>::Val(1.0),
                    Token::Val(3.0),
                    Token::Val(2.0),
                    Token::Stop(0),
                    Token::Done,
                ]
                .into_iter()
            },
            score_snd,
        );

        let v_gen = GeneratorContext::new(
            move || {
                vec![
                    Token::<f32, u32>::Val(10.0),
                    Token::Val(20.0),
                    Token::Val(30.0),
                    Token::Stop(0),
                    Token::Done,
                ]
                .into_iter()
            },
            v_snd,
        );

        // Collect output
        // Drain output into a vector for manual comparison (CheckerContext uses exact f32 ==)
        let out_vals = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
        let out_vals_clone = out_vals.clone();
        // Use a simple drain: ConsumerContext just takes a receiver
        let drain = dam::utility_contexts::ConsumerContext::new(out_rcv);

        parent.add_child(score_gen);
        parent.add_child(v_gen);
        parent.add_child(accum);
        parent.add_child(drain);

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        println!("flash_softmax_accum_basic: ref={:.6}, elapsed={:?}", ref_scalar, executed.elapsed_cycles());
        // ConsumerContext just drains — can't inspect values.
        // The test passes if the simulation completes without panic.
        // Numerical verification is done in the two_rows test which uses exact values.
    }

    /// Test with two query rows (two fibers) to verify state reset
    #[test]
    fn flash_softmax_accum_two_rows() {
        // Row 0: scores=[1,2], V=[10,20] → softmax([1,2])·[10,20]
        let ref_row0 = {
            let s = [1.0_f32, 2.0];
            let v = [10.0_f32, 20.0];
            let mx = 2.0_f32;
            let e: Vec<f32> = s.iter().map(|x| (x - mx).exp()).collect();
            let sm: f32 = e.iter().sum();
            e.iter().zip(v.iter()).map(|(a, b)| a * b / sm).sum::<f32>()
        };
        // Row 1: scores=[0,0,0], V=[5,5,5] → (1/3)*(5+5+5) = 5.0
        let ref_row1 = 5.0_f32;

        let mut parent = ProgramBuilder::default();
        let (score_snd, score_rcv) = parent.unbounded::<Token<f32, u32>>();
        let (v_snd, v_rcv) = parent.unbounded::<Token<f32, u32>>();
        let (out_snd, out_rcv) = parent.unbounded::<Token<f32, u32>>();

        let accum = FlashSoftmaxAccum::new(FlashSoftmaxAccumData {
            in_score: score_rcv,
            in_val: v_rcv,
            out_val: out_snd,
        });

        // Two fibers: [Val,Val,Stop(0)] [Val,Val,Val,Stop(1)] Done
        let score_gen = GeneratorContext::new(
            || {
                vec![
                    Token::<f32, u32>::Val(1.0), Token::Val(2.0), Token::Stop(0),
                    Token::Val(0.0), Token::Val(0.0), Token::Val(0.0), Token::Stop(1),
                    Token::Done,
                ].into_iter()
            },
            score_snd,
        );
        let v_gen = GeneratorContext::new(
            || {
                vec![
                    Token::<f32, u32>::Val(10.0), Token::Val(20.0), Token::Stop(0),
                    Token::Val(5.0), Token::Val(5.0), Token::Val(5.0), Token::Stop(1),
                    Token::Done,
                ].into_iter()
            },
            v_snd,
        );

        // Expected: Val(row0), Val(row1), Stop(0), Done
        let r0 = ref_row0;
        let r1 = ref_row1;
        let checker = CheckerContext::new(
            move || {
                vec![
                    Token::<f32, u32>::Val(r0),
                    Token::Val(r1),
                    Token::Stop(0),  // Stop(1) decremented to Stop(0)
                    Token::Done,
                ].into_iter()
            },
            out_rcv,
        );

        parent.add_child(score_gen);
        parent.add_child(v_gen);
        parent.add_child(accum);
        parent.add_child(checker);

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        println!("flash_softmax_accum_two_rows: ref0={:.4} ref1={:.4}, elapsed={:?}",
                 ref_row0, ref_row1, executed.elapsed_cycles());
        assert!(executed.passed(), "Simulation failed (checker mismatch)");
    }

    // ────────────────────────────────────────────────────────────────────────
    // Multi-rank reduction tests for Reduce / Spacc1 (reduction_depth > 0).
    //
    // These tests exercise the runtime branching introduced in Phase 2b:
    //   - Stops with depth < reduction_depth → within reduction (silently consumed)
    //   - Stop at depth == reduction_depth   → reduction-fiber-end (flush + decrement)
    //   - Stops with depth >  reduction_depth → above reduction (still flush + decrement;
    //     accumulator is empty so this acts as a stop-decrement pass-through with a
    //     spurious Val(0) — preserved from the existing untiled flush behavior).
    // ────────────────────────────────────────────────────────────────────────

    fn reduce_check_test<IRT, ORT>(in_val: fn() -> IRT, out_val: fn() -> ORT, red_depth: u32)
    where
        IRT: Iterator<Item = Token<u32, u32>> + 'static,
        ORT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_val_sender, in_val_receiver) = parent.unbounded();
        let (out_val_sender, out_val_receiver) = parent.unbounded();
        let data = ReduceData::<u32, u32, 1> {
            in_val: in_val_receiver,
            out_val: out_val_sender,
            sum: false,
            reduction_depth: red_depth,
        };
        let red = Reduce::<u32, u32, 1>::new(data);
        let gen1 = GeneratorContext::new(in_val, in_val_sender);
        let val_checker = CheckerContext::new(out_val, out_val_receiver);
        parent.add_child(gen1);
        parent.add_child(val_checker);
        parent.add_child(red);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        assert!(executed.passed(), "Reduce simulation failed (checker mismatch)");
    }

    /// red_depth=0 (default) — locks in the untiled behaviour with a real checker.
    /// Without this, the existing reduce_2d_test only used a PrinterContext.
    #[test]
    fn reduce_red_depth_0_untiled_check() {
        // Per-row sums: 5+5=10, 5, 4+8=12, 4+3=7, 4+3=7. End-of-rows yields a
        // spurious Val(0) Stop(0) (existing behaviour for outer Stop with empty acc).
        let in_val = || token_vec!(u32; u32; 5, 5, "S0", 5, "S0", 4, 8, "S0", 4, 3, "S0", 4, 3, "S1", "D").into_iter();
        let out_val = || token_vec!(u32; u32; 10, 5, 12, 7, 7, "S0", "D").into_iter();
        reduce_check_test(in_val, out_val, 0);
    }

    /// red_depth=1, single tile-group of two inner fibers.
    /// Inner Stop(0)s are absorbed; Stop(1) flushes the accumulated sum.
    #[test]
    fn reduce_red_depth_1_single_group() {
        let in_val = || token_vec!(u32; u32; 1, 2, "S0", 3, 4, "S0", "S1", "D").into_iter();
        // 1+2+3+4 = 10. Decrement Stop(1) → Stop(0).
        let out_val = || token_vec!(u32; u32; 10, "S0", "D").into_iter();
        reduce_check_test(in_val, out_val, 1);
    }

    /// red_depth=1, multiple tile-groups followed by an above-reduction Stop(2).
    /// Each Stop(1) flushes one row's reduced value; the final Stop(2) is the
    /// rank-above-reduction marker — empty-flush emits Val(0) + Stop(1).
    #[test]
    fn reduce_red_depth_1_multi_group_with_outer() {
        let in_val = || token_vec!(u32; u32; 1, "S0", 2, "S0", "S1", 3, "S0", 4, "S0", "S1", "S2", "D").into_iter();
        // Group 0: 1+2 = 3. Group 1: 3+4 = 7. Outer Stop(2) → empty-flush Val(0).
        let out_val = || token_vec!(u32; u32; 3, "S0", 7, "S0", 0, "S1", "D").into_iter();
        reduce_check_test(in_val, out_val, 1);
    }

    /// red_depth=2 — three input ranks collapse to one output rank.
    /// Stop(0) and Stop(1) absorbed; Stop(2) flushes; Stop(2) decrements to Stop(1).
    #[test]
    fn reduce_red_depth_2_deeper_nesting() {
        let in_val = || token_vec!(u32; u32; 1, 2, "S0", 3, "S0", "S1", 4, "S0", 5, 6, "S0", "S1", "S2", "D").into_iter();
        // All 6 vals sum to 21. Output rank = input rank - 2.
        let out_val = || token_vec!(u32; u32; 21, "S1", "D").into_iter();
        reduce_check_test(in_val, out_val, 2);
    }

    /// red_depth=1, empty inner fibers (Stops with no preceding Vals).
    /// Sum stays at 0; flush emits Val(0).
    #[test]
    fn reduce_red_depth_1_empty_inner_fibers() {
        let in_val = || token_vec!(u32; u32; "S0", "S0", "S1", "D").into_iter();
        let out_val = || token_vec!(u32; u32; 0, "S0", "D").into_iter();
        reduce_check_test(in_val, out_val, 1);
    }

    /// red_depth=1, single Val sandwiched between many absorbed inner Stops.
    /// Confirms inner-Stop absorption is unbounded and accumulator is not reset
    /// until the reduction-end Stop.
    #[test]
    fn reduce_red_depth_1_long_inner_chain() {
        let in_val = || token_vec!(u32; u32; "S0", 7, "S0", "S0", "S0", "S1", "D").into_iter();
        let out_val = || token_vec!(u32; u32; 7, "S0", "D").into_iter();
        reduce_check_test(in_val, out_val, 1);
    }

    fn spacc1_check_test<IRT1, IRT2, IRT3, ORT1, ORT2>(
        in_ocrd: fn() -> IRT1,
        in_icrd: fn() -> IRT2,
        in_val: fn() -> IRT3,
        out_icrd: fn() -> ORT1,
        out_val: fn() -> ORT2,
        red_depth: u32,
    ) where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        IRT2: Iterator<Item = Token<u32, u32>> + 'static,
        IRT3: Iterator<Item = Token<f32, u32>> + 'static,
        ORT1: Iterator<Item = Token<u32, u32>> + 'static,
        ORT2: Iterator<Item = Token<f32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_ocrd_sender, in_ocrd_receiver) = parent.unbounded();
        let (in_icrd_sender, in_icrd_receiver) = parent.unbounded();
        let (in_val_sender, in_val_receiver) = parent.unbounded();
        let (out_val_sender, out_val_receiver) = parent.unbounded();
        let (out_icrd_sender, out_icrd_receiver) = parent.unbounded();
        let data = Spacc1Data::<u32, f32, u32> {
            in_crd_outer: in_ocrd_receiver,
            in_crd_inner: in_icrd_receiver,
            in_val: in_val_receiver,
            out_val: out_val_sender,
            out_crd_inner: out_icrd_sender,
            reduction_depth: red_depth,
        };
        let red = Spacc1::new(data);
        let gen1 = GeneratorContext::new(in_ocrd, in_ocrd_sender);
        let gen2 = GeneratorContext::new(in_icrd, in_icrd_sender);
        let gen3 = GeneratorContext::new(in_val, in_val_sender);
        let icrd_checker = CheckerContext::new(out_icrd, out_icrd_receiver);
        let val_checker = CheckerContext::new(out_val, out_val_receiver);
        parent.add_child(gen1);
        parent.add_child(gen2);
        parent.add_child(gen3);
        parent.add_child(icrd_checker);
        parent.add_child(val_checker);
        parent.add_child(red);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        assert!(executed.passed(), "Spacc1 simulation failed (checker mismatch)");
    }

    /// red_depth=0 (default) — locks in the existing untiled Spacc1 behaviour
    /// using a real checker. Mirrors spacc1_2d_test inputs.
    #[test]
    fn spacc1_red_depth_0_untiled_check() {
        let in_ocrd = || token_vec!(u32; u32; 0, 2, "S0", 2, "S1", "D").into_iter();
        let in_icrd = || token_vec!(u32; u32; 0, 2, 3, "S0", 0, 2, 3, "S1", 0, 2, 3, "S2", "D").into_iter();
        let in_val = || token_vec!(f32; u32; 50.0, 5.0, 10.0, "S0", 40.0, 4.0, 8.0, "S1", -40.0, 33.0, 36.0, "S2", "D").into_iter();
        // First Stop(0) on ocrd flushes accum from ocrd Vals 0+2 (icrd S0+S1):
        //   accum[0]=50+40=90, accum[2]=5+4=9, accum[3]=10+8=18
        // Then ocrd Stop(1) flushes accum from ocrd Val 2 (icrd S2):
        //   accum[0]=-40, accum[2]=33, accum[3]=36
        let out_icrd = || token_vec!(u32; u32; 0, 2, 3, "S0", 0, 2, 3, "S1", "D").into_iter();
        let out_val  = || token_vec!(f32; u32; 90.0, 9.0, 18.0, "S0", -40.0, 33.0, 36.0, "S1", "D").into_iter();
        spacc1_check_test(in_ocrd, in_icrd, in_val, out_icrd, out_val, 0);
    }

    /// red_depth=1, single output row split across two tile-passes.
    /// Inner ocrd Stop(0) is absorbed (no flush); ocrd Stop(1) flushes the
    /// merged accumulator.
    #[test]
    fn spacc1_red_depth_1_two_tiles_one_row() {
        // Two tiles contributing to a single output row:
        //   Tile 0: position 0→10, position 1→20
        //   Tile 1: position 0→ 5, position 2→30
        // Expected merged row: position 0→15, position 1→20, position 2→30.
        let in_ocrd = || token_vec!(u32; u32; 0, "S0", 0, "S1", "D").into_iter();
        let in_icrd = || token_vec!(u32; u32; 0, 1, "S0", 0, 2, "S1", "D").into_iter();
        let in_val  = || token_vec!(f32; u32; 10.0, 20.0, "S0", 5.0, 30.0, "S1", "D").into_iter();
        let out_icrd = || token_vec!(u32; u32; 0, 1, 2, "S1", "D").into_iter();
        let out_val  = || token_vec!(f32; u32; 15.0, 20.0, 30.0, "S1", "D").into_iter();
        spacc1_check_test(in_ocrd, in_icrd, in_val, out_icrd, out_val, 1);
    }

    /// red_depth=1, two output rows, each split across two tile-passes.
    /// Stop(0) within reduction is absorbed; Stop(1) flushes one output row;
    /// Stop(2) flushes the (empty) accum and decrements rank for outer-end.
    #[test]
    fn spacc1_red_depth_1_two_rows_two_tiles_each() {
        // Row 0: Tile 0 (pos 0→1) + Tile 1 (pos 0→2, pos 1→3)  → merged: 0→3, 1→3
        // Row 1: Tile 0 (pos 1→4)              + Tile 1 (pos 1→5)         → merged: 1→9
        let in_ocrd = || token_vec!(u32; u32; 0, "S0", 0, "S1", 0, "S0", 0, "S2", "D").into_iter();
        let in_icrd = || token_vec!(u32; u32; 0, "S0", 0, 1, "S1", 1, "S0", 1, "S2", "D").into_iter();
        let in_val  = || token_vec!(f32; u32; 1.0, "S0", 2.0, 3.0, "S1", 4.0, "S0", 5.0, "S2", "D").into_iter();
        let out_icrd = || token_vec!(u32; u32; 0, 1, "S1", 1, "S2", "D").into_iter();
        let out_val  = || token_vec!(f32; u32; 3.0, 3.0, "S1", 9.0, "S2", "D").into_iter();
        spacc1_check_test(in_ocrd, in_icrd, in_val, out_icrd, out_val, 1);
    }
}
