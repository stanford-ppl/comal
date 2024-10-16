use core::hash::Hash;
use std::{collections::BTreeMap, u32};

use dam::structures::{Identifiable, Identifier, Time};
use dam::{
    context_tools::*,
    dam_macros::{context_macro, event_type},
};
use serde::{Deserialize, Serialize};

use super::primitive::Token;

pub struct ReduceData<ValType: Clone, StopType: Clone> {
    pub in_val: Receiver<Token<ValType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    pub block_size: usize,
}

#[context_macro]
pub struct Reduce<ValType: Clone, StopType: Clone> {
    reduce_data: ReduceData<ValType, StopType>,
}

impl<ValType: DAMType, StopType: DAMType> Reduce<ValType, StopType>
where
    Reduce<ValType, StopType>: Context,
{
    pub fn new(reduce_data: ReduceData<ValType, StopType>) -> Self {
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

impl<ValType, StopType> Context for Reduce<ValType, StopType>
where
    ValType: DAMType + std::ops::AddAssign<ValType> + std::cmp::PartialEq,
    StopType: DAMType
        + std::ops::Add<u32, Output = StopType>
        + std::ops::Sub<u32, Output = StopType>
        + std::cmp::PartialEq
        + std::convert::From<u32>,
    // Token<f32, u32>: From<Token<ValType, StopType>>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut sum = ValType::default();
        // let max_num = ValType::MAX;
        // let max_tkn: StopType = max_num.into();
        // let mut prev_tkn: StopType = max_tkn.clone();
        // let mut prev_tkn: StopType = StopType::default();
        let id = self.id();
        let curr_id = Identifier { id: 0 };
        let mut prev_tkn = Token::default();
        loop {
            match self.reduce_data.in_val.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data.clone() {
                    Token::Val(val) => {
                        sum += val.clone();
                        prev_tkn = Token::Val(val.clone());
                    }
                    Token::Stop(stkn) => {
                        let curr_time = self.time.tick();
                        // if prev_tkn != Token::Stop(StopType::default())
                        //     || stkn == StopType::default()
                        // {
                        //     self.reduce_data
                        //         .out_val
                        //         .enqueue(
                        //             &self.time,
                        //             ChannelElement::new(curr_time + 1, Token::Val(sum.clone())),
                        //         )
                        //         .unwrap();
                        //     if id == curr_id {
                        //         println!(
                        //             "Out val: {:?}",
                        //             // Token::<ValType, StopType>::Val(sum.clone())
                        //             curr_in.data.clone()
                        //         );
                        //     }
                        // }
                        self.reduce_data
                            .out_val
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + Time::new((self.reduce_data.block_size * self.reduce_data.block_size).try_into().unwrap()), Token::Val(sum.clone())),
                            )
                            .unwrap();
                        if id == curr_id {
                            println!(
                                "In val: {:?}, Out val: {:?}",
                                curr_in.data.clone(),
                                Token::<ValType, StopType>::Val(sum.clone())
                            );
                        }
                        let out_val = Token::<ValType, StopType>::Val(sum.clone());
                        prev_tkn = out_val.clone();
                        // let _ = dam::logging::log_event(&ReduceLog {
                        //     out_val: out_val.clone().into(),
                        // });
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
                            let stk = Token::<ValType, StopType>::Stop(stkn.clone() - 1);
                            prev_tkn = stk.clone();
                            // let _ = dam::logging::log_event(&ReduceLog {
                            //     out_val: stk.clone().into(),
                            // });
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
                            // println!("Out val: {:?}", curr_in.data.clone());
                        }
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
    pub block_size: usize,
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
        + std::cmp::PartialEq,
    // Token<f32, u32>: From<Token<ValType, StopType>>,
    // Token<u32, u32>: From<Token<CrdType, StopType>>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut accum_storage: BTreeMap<CrdType, ValType> = BTreeMap::new();
        let id = Identifier { id: 42 };
        let id1 = Identifier { id: 0 };
        let mut icrd_stkn_pop_cnt = 0;
        let mut ocrd_val_pop_cnt = 0;
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
                                let latency = self.spacc1_data.block_size * 6;
                                self.time.incr_cycles(latency.try_into().unwrap());
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
                            self.time.tick() + Time::new(1),
                            Token::<ValType, StopType>::Val(value.clone()),
                        );
                        self.spacc1_data
                            .out_val
                            .enqueue(&self.time, val_chan_elem)
                            .unwrap();
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
                                panic!("Inner and outer stop token types don't match");
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
                            // let _ = dam::logging::log_event(&SpaccLog {
                            //     out_val: Token::<ValType, StopType>::Done.into(),
                            //     out_crd: Token::<CrdType, StopType>::Done.into(),
                            // });
                            return;
                        }
                        _ => {
                            // if self.id() == id.clone() {
                            println!("Icrd: {:?}", in_icrd.data.clone());
                            println!("Ival: {:?}", in_val.data.clone());
                            // }
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

#[context_macro]
pub struct MaxReduce<ValType: Clone, StopType: Clone> {
    max_reduce_data: ReduceData<ValType, StopType>,
    min_val: ValType,
}

impl<ValType: DAMType, StopType: DAMType> MaxReduce<ValType, StopType>
where
    MaxReduce<ValType, StopType>: Context,
{
    pub fn new(max_reduce_data: ReduceData<ValType, StopType>, min_val: ValType) -> Self {
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
        loop {
            match self.max_reduce_data.in_val.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => match val.lt(&max_elem) {
                        true => (),
                        false => max_elem = val,
                    },
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

#[cfg(test)]
mod tests {

    use dam::simulation::*;
    use dam::utility_contexts::*;

    use crate::templates::primitive::Token;
    use crate::token_vec;

    use super::{MaxReduce, Reduce, Spacc1};
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
        let data = ReduceData::<u32, u32> {
            in_val: in_val_receiver,
            out_val: out_val_sender,
        };
        let red = Reduce::new(data);
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
        let data = ReduceData::<f32, u32> {
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
}
