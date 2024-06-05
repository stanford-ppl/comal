use std::collections::HashSet;

use dam::{context_tools::*, dam_macros::context_macro};

use super::primitive::Token;

pub struct CrdJoinerData<ValType: Clone, StopType: Clone> {
    pub in_crd1: Receiver<Token<ValType, StopType>>,
    pub in_ref1: Receiver<Token<ValType, StopType>>,
    pub in_crd2: Receiver<Token<ValType, StopType>>,
    pub in_ref2: Receiver<Token<ValType, StopType>>,
    pub out_ref1: Sender<Token<ValType, StopType>>,
    pub out_ref2: Sender<Token<ValType, StopType>>,
    pub out_crd: Sender<Token<ValType, StopType>>,
}

pub struct NJoinerData<ValType: Clone, StopType: Clone> {
    pub in_crds: Vec<Receiver<Token<ValType, StopType>>>,
    pub in_refs: Vec<Receiver<Token<ValType, StopType>>>,
    pub out_refs: Vec<Sender<Token<ValType, StopType>>>,
    pub out_crd: Sender<Token<ValType, StopType>>,
}

#[context_macro]
pub struct Intersect<ValType: Clone, StopType: Clone> {
    intersect_data: CrdJoinerData<ValType, StopType>,
}

#[context_macro]
pub struct NIntersect<ValType: Clone, StopType: Clone> {
    intersect_data: NJoinerData<ValType, StopType>,
}

impl<ValType: DAMType, StopType: DAMType> NIntersect<ValType, StopType>
where
    NIntersect<ValType, StopType>: Context,
{
    pub fn new(intersect_data: NJoinerData<ValType, StopType>) -> Self {
        let int = NIntersect {
            intersect_data,
            context_info: Default::default(),
        };
        int.intersect_data
            .in_crds
            .iter()
            .for_each(|channel| (channel).attach_receiver(&int));
        int.intersect_data
            .in_refs
            .iter()
            .for_each(|channel| (channel).attach_receiver(&int));
        int.intersect_data
            .out_refs
            .iter()
            .for_each(|channel| (channel).attach_sender(&int));
        (int.intersect_data.out_crd).attach_sender(&int);

        int
    }
}

impl<ValType: DAMType, StopType: DAMType> Intersect<ValType, StopType>
where
    Intersect<ValType, StopType>: Context,
{
    pub fn new(intersect_data: CrdJoinerData<ValType, StopType>) -> Self {
        let int = Intersect {
            intersect_data,
            context_info: Default::default(),
        };
        (int.intersect_data.in_crd1).attach_receiver(&int);
        (int.intersect_data.in_ref1).attach_receiver(&int);
        (int.intersect_data.in_crd2).attach_receiver(&int);
        (int.intersect_data.in_ref2).attach_receiver(&int);
        (int.intersect_data.out_ref1).attach_sender(&int);
        (int.intersect_data.out_ref2).attach_sender(&int);
        (int.intersect_data.out_crd).attach_sender(&int);

        int
    }
}

impl<ValType, StopType> Context for NIntersect<ValType, StopType>
where
    ValType: DAMType
        + std::ops::AddAssign<u32>
        + std::ops::AddAssign<ValType>
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>
        + std::cmp::Eq
        + std::hash::Hash
        + Ord,
    StopType: DAMType
        + std::ops::Add<u32, Output = StopType>
        + std::ops::Sub<u32, Output = StopType>
        + std::cmp::PartialEq,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        loop {
            let mut crd_peeks = self
                .intersect_data
                .in_crds
                .iter()
                .map(|channel| channel.peek_next(&self.time))
                .collect::<Vec<_>>();
            let mut ref_peeks = self
                .intersect_data
                .in_refs
                .iter()
                .map(|channel| channel.peek_next(&self.time))
                .collect::<Vec<_>>();

            let mut matching_values = HashSet::new(); // Using HashSet for efficient lookups
            let mut all_values_match = true;
            let mut min_val: Option<ValType> = None;
            for peek in &crd_peeks {
                match peek {
                    Ok(ChannelElement {
                        data: Token::Val(val),
                        ..
                    }) => {
                        if !matching_values.is_empty() && !matching_values.contains(&val) {
                            all_values_match = false; // Only set to false if a mismatch is found after the first value
                        }
                        matching_values.insert(val);
                        min_val = Some(min_val.map_or(val.clone(), |v| v.min(val.clone())));
                    }
                    Ok(_) => {
                        all_values_match = false;
                    }
                    _ => {} // Handle Empty/Done tokens later
                }
            }

            if all_values_match {
                let curr_time = self.time.tick();
                let val = matching_values.iter().next().unwrap();
                self.intersect_data
                    .out_crd
                    .enqueue(
                        &self.time,
                        ChannelElement::new(curr_time + 1, Token::Val((*val).clone().into())),
                    )
                    .unwrap();
                dbg!(Token::<ValType, StopType>::Val((*val).clone().into()));

                for i in 0..self.intersect_data.in_crds.len() {
                    // Enqueue matching value to output channels

                    // Enqueue corresponding ref token to output channels
                    self.intersect_data.out_refs[i]
                        .enqueue(
                            &self.time,
                            ref_peeks[i].as_ref().unwrap().clone(), // Assuming peek is successful
                        )
                        .unwrap();
                dbg!(ref_peeks[i].as_ref().unwrap().clone());

                    // Dequeue elements from input channels
                    self.intersect_data.in_crds[i].dequeue(&self.time).unwrap();
                    self.intersect_data.in_refs[i].dequeue(&self.time).unwrap();
                }
            } else {
                let curr_time = self.time.tick();

                // Prioritize Stop tokens
                let mut stop_token = None;
                for peek in &crd_peeks {
                    if let Ok(ChannelElement {
                        data: Token::Stop(stkn),
                        ..
                    }) = peek
                    {
                        stop_token = Some(stkn);
                        break;
                    }
                }

                if let Some(stkn) = stop_token {
                    // ... (Handle Stop token - similar to original logic but for vectors)
                    self.intersect_data
                        .out_crd
                        .enqueue(
                            &self.time,
                            ChannelElement::new(curr_time + 1, Token::Stop(stkn.clone())), // Get any matching value
                        )
                        .unwrap();

                    // Enqueue corresponding ref token to output channels
                    for (i, out_ref) in self.intersect_data.out_refs.iter().enumerate() {
                        out_ref
                            .enqueue(&self.time, ref_peeks[i].as_ref().unwrap().clone())
                            .unwrap();
                        // Dequeue elements from input channels
                        self.intersect_data.in_crds[i].dequeue(&self.time).unwrap();
                        self.intersect_data.in_refs[i].dequeue(&self.time).unwrap();
                    }
                } else {
                    // Handle mismatches or Done tokens
                    for (i, peek) in crd_peeks.iter().enumerate() {
                        match peek {
                            Ok(ChannelElement {
                                data: Token::Val(val),
                                ..
                            }) if Some(val) == min_val.as_ref() => {
                                // Dequeue from channels with min val
                                self.intersect_data.in_crds[i].dequeue(&self.time).unwrap();
                                self.intersect_data.in_refs[i].dequeue(&self.time).unwrap();
                            }
                            Ok(ChannelElement {
                                data: Token::Done, ..
                            }) => {
                                dbg!("DONE");
                                let channel_elem =
                                    ChannelElement::new(self.time.tick() + 1, Token::Done);
                                self.intersect_data
                                    .out_crd
                                    .enqueue(&self.time, channel_elem.clone())
                                    .unwrap();
                                for out_ref in self.intersect_data.out_refs.iter() {
                                    out_ref.enqueue(&self.time, channel_elem.clone()).unwrap();
                                }
                                return;
                                // ... (Handle Done token - similar to original logic but for vectors)
                            }
                            _ => {} // Keep other tokens
                        }
                    }
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

impl<ValType, StopType> Context for Intersect<ValType, StopType>
where
    ValType: DAMType
        + std::ops::AddAssign<u32>
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
        loop {
            let crd1_deq = self.intersect_data.in_crd1.peek_next(&self.time);
            let crd2_deq = self.intersect_data.in_crd2.peek_next(&self.time);
            let ref1_deq = self.intersect_data.in_ref1.peek_next(&self.time);
            let ref2_deq = self.intersect_data.in_ref2.peek_next(&self.time);

            match (crd1_deq, crd2_deq) {
                (Ok(crd1), Ok(crd2)) => {
                    let ref1: Token<ValType, StopType> = ref1_deq.unwrap().data;
                    let ref2: Token<ValType, StopType> = ref2_deq.unwrap().data;
                    match (crd1.data, crd2.data) {
                        (Token::Val(crd1), Token::Val(crd2)) => match (crd1, crd2) {
                            (crd1, crd2) if crd1 == crd2 => {
                                let curr_time = self.time.tick();
                                self.intersect_data
                                    .out_crd
                                    .enqueue(
                                        &self.time,
                                        ChannelElement::new(curr_time + 1, Token::Val(crd1)),
                                    )
                                    .unwrap();
                                self.intersect_data
                                    .out_ref1
                                    .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref1))
                                    .unwrap();
                                self.intersect_data
                                    .out_ref2
                                    .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref2))
                                    .unwrap();
                                self.intersect_data.in_crd1.dequeue(&self.time).unwrap();
                                self.intersect_data.in_ref1.dequeue(&self.time).unwrap();
                                self.intersect_data.in_crd2.dequeue(&self.time).unwrap();
                                self.intersect_data.in_ref2.dequeue(&self.time).unwrap();
                            }
                            (crd1, crd2) if crd1 < crd2 => {
                                self.intersect_data.in_crd1.dequeue(&self.time).unwrap();
                                self.intersect_data.in_ref1.dequeue(&self.time).unwrap();
                            }
                            (crd1, crd2) if crd1 > crd2 => {
                                self.intersect_data.in_crd2.dequeue(&self.time).unwrap();
                                self.intersect_data.in_ref2.dequeue(&self.time).unwrap();
                            }
                            (_, _) => {
                                panic!("Unexpected case found in val comparison");
                            }
                        },
                        (Token::Val(_), Token::Stop(_)) => {
                            self.intersect_data.in_crd1.dequeue(&self.time).unwrap();
                            self.intersect_data.in_ref1.dequeue(&self.time).unwrap();
                        }
                        (Token::Val(_), Token::Done) | (Token::Done, Token::Val(_)) => {
                            let curr_time = self.time.tick();
                            self.intersect_data
                                .out_crd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Done),
                                )
                                .unwrap();
                            self.intersect_data
                                .out_ref1
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Done),
                                )
                                .unwrap();
                            self.intersect_data
                                .out_ref2
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Done),
                                )
                                .unwrap();
                        }
                        (Token::Stop(_), Token::Val(_)) => {
                            self.intersect_data.in_crd2.dequeue(&self.time).unwrap();
                            self.intersect_data.in_ref2.dequeue(&self.time).unwrap();
                        }
                        (Token::Stop(stkn1), Token::Stop(stkn2)) => {
                            assert_eq!(stkn1, stkn2);
                            let curr_time = self.time.tick();
                            self.intersect_data
                                .out_crd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Stop(stkn1)),
                                )
                                .unwrap();
                            self.intersect_data
                                .out_ref1
                                .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref1))
                                .unwrap();
                            self.intersect_data
                                .out_ref2
                                .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref2))
                                .unwrap();
                            self.intersect_data.in_crd1.dequeue(&self.time).unwrap();
                            self.intersect_data.in_ref1.dequeue(&self.time).unwrap();
                            self.intersect_data.in_crd2.dequeue(&self.time).unwrap();
                            self.intersect_data.in_ref2.dequeue(&self.time).unwrap();
                        }
                        (tkn @ Token::Empty, Token::Val(_))
                        | (Token::Val(_), tkn @ Token::Empty)
                        | (tkn @ Token::Done, Token::Done) => {
                            let channel_elem =
                                ChannelElement::new(self.time.tick() + 1, tkn.clone());
                            self.intersect_data
                                .out_crd
                                .enqueue(&self.time, channel_elem.clone())
                                .unwrap();
                            self.intersect_data
                                .out_ref1
                                .enqueue(&self.time, channel_elem.clone())
                                .unwrap();
                            self.intersect_data
                                .out_ref2
                                .enqueue(&self.time, channel_elem.clone())
                                .unwrap();
                            if tkn.clone() == Token::Done {
                                return;
                            }
                        }
                        _ => (),
                    }
                }
                (_, _) => {
                    panic!("Reached unhandled case");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

#[context_macro]
pub struct Union<ValType: Clone, StopType: Clone> {
    union_data: CrdJoinerData<ValType, StopType>,
}

impl<ValType: DAMType, StopType: DAMType> Union<ValType, StopType>
where
    Union<ValType, StopType>: Context,
{
    pub fn new(union_data: CrdJoinerData<ValType, StopType>) -> Self {
        let int = Union {
            union_data,
            context_info: Default::default(),
        };
        (int.union_data.in_crd1).attach_receiver(&int);
        (int.union_data.in_ref1).attach_receiver(&int);
        (int.union_data.in_crd2).attach_receiver(&int);
        (int.union_data.in_ref2).attach_receiver(&int);
        (int.union_data.out_ref1).attach_sender(&int);
        (int.union_data.out_ref2).attach_sender(&int);
        (int.union_data.out_crd).attach_sender(&int);

        int
    }
}

impl<ValType, StopType> Context for Union<ValType, StopType>
where
    ValType: DAMType
        + std::ops::AddAssign<u32>
        + std::ops::AddAssign<ValType>
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    StopType:
        DAMType + std::ops::Add<u32, Output = StopType> + std::ops::Sub<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut get_crd1: bool = false;
        let mut get_crd2: bool = false;

        loop {
            if get_crd1 {
                self.union_data.in_crd1.dequeue(&self.time).unwrap();
                self.union_data.in_ref1.dequeue(&self.time).unwrap();
            }
            if get_crd2 {
                self.union_data.in_crd2.dequeue(&self.time).unwrap();
                self.union_data.in_ref2.dequeue(&self.time).unwrap();
            }
            let ref1_deq = self.union_data.in_ref1.peek_next(&self.time);
            let ref2_deq = self.union_data.in_ref2.peek_next(&self.time);
            let crd1_deq = self.union_data.in_crd1.peek_next(&self.time);
            let crd2_deq = self.union_data.in_crd2.peek_next(&self.time);

            match (crd1_deq, crd2_deq) {
                (Ok(crd1), Ok(crd2)) => {
                    let ref1: Token<ValType, StopType> = ref1_deq.unwrap().data;
                    let ref2: Token<ValType, StopType> = ref2_deq.unwrap().data;
                    let curr_time = self.time.tick();
                    match (crd1.data, crd2.data) {
                        (Token::Val(crd1), Token::Val(crd2)) => match (crd1, crd2) {
                            (crd1, crd2) if crd1 == crd2 => {
                                self.union_data
                                    .out_crd
                                    .enqueue(
                                        &self.time,
                                        ChannelElement::new(curr_time + 1, Token::Val(crd1)),
                                    )
                                    .unwrap();
                                self.union_data
                                    .out_ref1
                                    .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref1))
                                    .unwrap();
                                self.union_data
                                    .out_ref2
                                    .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref2))
                                    .unwrap();
                                get_crd1 = true;
                                get_crd2 = true;
                            }
                            (crd1, crd2) if crd1 < crd2 => {
                                self.union_data
                                    .out_crd
                                    .enqueue(
                                        &self.time,
                                        ChannelElement::new(curr_time + 1, Token::Val(crd1)),
                                    )
                                    .unwrap();
                                self.union_data
                                    .out_ref1
                                    .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref1))
                                    .unwrap();
                                self.union_data
                                    .out_ref2
                                    .enqueue(
                                        &self.time,
                                        ChannelElement::new(curr_time + 1, Token::Empty),
                                    )
                                    .unwrap();
                                get_crd1 = true;
                                get_crd2 = false;
                            }
                            (crd1, crd2) if crd1 > crd2 => {
                                self.union_data
                                    .out_crd
                                    .enqueue(
                                        &self.time,
                                        ChannelElement::new(curr_time + 1, Token::Val(crd1)),
                                    )
                                    .unwrap();
                                self.union_data
                                    .out_ref1
                                    .enqueue(
                                        &self.time,
                                        ChannelElement::new(curr_time + 1, Token::Empty),
                                    )
                                    .unwrap();
                                self.union_data
                                    .out_ref2
                                    .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref2))
                                    .unwrap();
                                get_crd1 = false;
                                get_crd2 = true;
                            }
                            (_, _) => {
                                panic!("Unexpected case found in val comparison");
                            }
                        },
                        (Token::Val(crd1), Token::Stop(_)) | (Token::Val(crd1), Token::Empty) => {
                            self.union_data
                                .out_crd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Val(crd1)),
                                )
                                .unwrap();
                            self.union_data
                                .out_ref1
                                .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref1))
                                .unwrap();
                            self.union_data
                                .out_ref2
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Empty),
                                )
                                .unwrap();
                            get_crd1 = true;
                            get_crd2 = false;
                        }
                        (Token::Val(_), Token::Done)
                        | (Token::Done, Token::Val(_))
                        | (Token::Done, Token::Done)
                        | (Token::Done, Token::Empty)
                        | (Token::Empty, Token::Done) => {
                            let channel_elem =
                                ChannelElement::new(self.time.tick() + 1, Token::Done);
                            self.union_data
                                .out_crd
                                .enqueue(&self.time, channel_elem.clone())
                                .unwrap();
                            self.union_data
                                .out_ref1
                                .enqueue(&self.time, channel_elem.clone())
                                .unwrap();
                            self.union_data
                                .out_ref2
                                .enqueue(&self.time, channel_elem.clone())
                                .unwrap();
                            return;
                        }
                        (Token::Stop(_), Token::Val(crd2)) | (Token::Empty, Token::Val(crd2)) => {
                            self.union_data
                                .out_crd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Val(crd2)),
                                )
                                .unwrap();
                            self.union_data
                                .out_ref1
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Empty),
                                )
                                .unwrap();
                            self.union_data
                                .out_ref2
                                .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref2))
                                .unwrap();
                            get_crd1 = false;
                            get_crd2 = true;
                        }
                        (Token::Stop(stkn1), Token::Stop(_)) => {
                            self.union_data
                                .out_crd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(curr_time + 1, Token::Stop(stkn1)),
                                )
                                .unwrap();
                            self.union_data
                                .out_ref1
                                .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref1))
                                .unwrap();
                            self.union_data
                                .out_ref2
                                .enqueue(&self.time, ChannelElement::new(curr_time + 1, ref2))
                                .unwrap();
                            get_crd1 = true;
                            get_crd2 = true;
                        }
                        (Token::Stop(_), Token::Empty) => {
                            get_crd1 = false;
                            get_crd2 = true;
                        }
                        (Token::Empty, Token::Stop(_)) => {
                            get_crd1 = true;
                            get_crd2 = false;
                        }
                        _ => (),
                    }
                }
                (_, _) => {
                    panic!("Reached unhandled case");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use dam::{simulation::*, utility_contexts::*};

    use crate::{
        templates::{
            joiner::{NIntersect, NJoinerData},
            primitive::Token,
        },
        token_vec,
    };

    use super::{CrdJoinerData, Intersect, Union};
    #[test]
    fn intersect_2d_test() {
        let in_crd1 = || token_vec!(u32; u32; 0, "S0", 0, 1, 2, "S1", "D").into_iter();
        let in_ref1 = || token_vec!(u32; u32; 0, "S0", 1, 2, 3, "S1", "D").into_iter();
        let in_crd2 = || token_vec!(u32; u32; 0, 1, 2, "S0", 0, 1, 2, "S1", "D").into_iter();
        let in_ref2 = || token_vec!(u32; u32; 0, 1, 2, "S0", 0, 1, 2, "S1", "D").into_iter();

        let out_crd = || token_vec!(u32; u32; 0, "S0", 0, 1, 2, "S1", "D").into_iter();
        let out_ref1 = || token_vec!(u32; u32; 0, "S0", 1, 2, 3, "S1", "D").into_iter();
        let out_ref2 = || token_vec!(u32; u32; 0, "S0", 0, 1, 2, "S1", "D").into_iter();
        intersect_test(
            in_crd1, in_ref1, in_crd2, in_ref2, out_crd, out_ref1, out_ref2,
        );
    }

    #[test]
    fn union_2d_test() {
        let in_crd1 =
            || token_vec!(u32; u32; 0, 1, "S0", 2, 3, "S0", "S0", 4, 5, "S1", "D").into_iter();
        let in_ref1 =
            || token_vec!(u32; u32; 0, 1, "S0", 2, 3, "S0", "S0", 4, 5, "S1", "D").into_iter();
        let in_crd2 =
            || token_vec!(u32; u32; 1, 2, 3, "S0", "S0", 0, 1, 2, "S0", "S1", "D").into_iter();
        let in_ref2 =
            || token_vec!(u32; u32; 0, 1, 2, "S0", "S0", 2, 3, 4, "S0", "S1", "D").into_iter();

        let out_crd = || {
            token_vec!(u32; u32; 0, 1, 2, 3, "S0", 2, 3, "S0", 0, 1, 2, "S0", 4, 5, "S1", "D")
                .into_iter()
        };
        let out_ref1 = || {
            token_vec!(u32; u32; 0, 1, "N", "N", "S0", 2, 3, "S0", "N", "N", "N", "S0", 4, 5, "S1", "D").into_iter()
        };
        let out_ref2 = || {
            token_vec!(u32; u32; "N", 0, 1, 2, "S0", "N", "N", "S0", 2, 3, 4, "S0", "N", "N", "S1", "D").into_iter()
        };
        union_test(
            in_crd1, in_ref1, in_crd2, in_ref2, out_crd, out_ref1, out_ref2,
        );
    }

    fn intersect_test<IRT1, IRT2, IRT3, IRT4, ORT1, ORT2, ORT3>(
        in_crd1: fn() -> IRT1,
        in_ref1: fn() -> IRT2,
        in_crd2: fn() -> IRT3,
        in_ref2: fn() -> IRT4,
        out_crd: fn() -> ORT1,
        out_ref1: fn() -> ORT2,
        out_ref2: fn() -> ORT3,
    ) where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        IRT2: Iterator<Item = Token<u32, u32>> + 'static,
        IRT3: Iterator<Item = Token<u32, u32>> + 'static,
        IRT4: Iterator<Item = Token<u32, u32>> + 'static,
        ORT1: Iterator<Item = Token<u32, u32>> + 'static,
        ORT2: Iterator<Item = Token<u32, u32>> + 'static,
        ORT3: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let chan_size = 4;

        let mut parent = ProgramBuilder::default();
        let (in_crd1_sender, in_crd1_receiver) = parent.bounded::<Token<u32, u32>>(chan_size);
        let (in_crd2_sender, in_crd2_receiver) = parent.bounded::<Token<u32, u32>>(chan_size);
        let (in_ref1_sender, in_ref1_receiver) = parent.bounded::<Token<u32, u32>>(chan_size);
        let (in_ref2_sender, in_ref2_receiver) = parent.bounded::<Token<u32, u32>>(chan_size);
        let (out_crd_sender, out_crd_receiver) = parent.bounded::<Token<u32, u32>>(chan_size);
        let (out_ref1_sender, out_ref1_receiver) = parent.bounded::<Token<u32, u32>>(chan_size);
        let (out_ref2_sender, out_ref2_receiver) = parent.bounded::<Token<u32, u32>>(chan_size);

        // let data = CrdJoinerData::<u32, u32> {
        //     in_crd1: in_crd1_receiver,
        //     in_ref1: in_ref1_receiver,
        //     in_crd2: in_crd2_receiver,
        //     in_ref2: in_ref2_receiver,
        //     out_crd: out_crd_sender,
        //     out_ref1: out_ref1_sender,
        //     out_ref2: out_ref2_sender,
        // };
        let data = NJoinerData::<u32, u32> {
            in_crds: vec![in_crd1_receiver, in_crd2_receiver],
            in_refs: vec![in_ref1_receiver, in_ref2_receiver],
            out_crd: out_crd_sender,
            out_refs: vec![out_ref1_sender, out_ref2_sender],
        };
        let intersect = NIntersect::new(data);
        let gen1 = GeneratorContext::new(in_crd1, in_crd1_sender);
        let gen2 = GeneratorContext::new(in_ref1, in_ref1_sender);
        let gen3 = GeneratorContext::new(in_crd2, in_crd2_sender);
        let gen4 = GeneratorContext::new(in_ref2, in_ref2_sender);
        let crd_checker = CheckerContext::new(out_crd, out_crd_receiver);
        let ref1_checker = CheckerContext::new(out_ref1, out_ref1_receiver);
        let ref2_checker = CheckerContext::new(out_ref2, out_ref2_receiver);

        parent.add_child(gen1);
        parent.add_child(gen2);
        parent.add_child(gen3);
        parent.add_child(gen4);
        parent.add_child(crd_checker);
        parent.add_child(ref1_checker);
        parent.add_child(ref2_checker);
        parent.add_child(intersect);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }

    fn union_test<IRT1, IRT2, IRT3, IRT4, ORT1, ORT2, ORT3>(
        in_crd1: fn() -> IRT1,
        in_ref1: fn() -> IRT2,
        in_crd2: fn() -> IRT3,
        in_ref2: fn() -> IRT4,
        out_crd: fn() -> ORT1,
        out_ref1: fn() -> ORT2,
        out_ref2: fn() -> ORT3,
    ) where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        IRT2: Iterator<Item = Token<u32, u32>> + 'static,
        IRT3: Iterator<Item = Token<u32, u32>> + 'static,
        IRT4: Iterator<Item = Token<u32, u32>> + 'static,
        ORT1: Iterator<Item = Token<u32, u32>> + 'static,
        ORT2: Iterator<Item = Token<u32, u32>> + 'static,
        ORT3: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_crd1_sender, in_crd1_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (in_crd2_sender, in_crd2_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (in_ref1_sender, in_ref1_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (in_ref2_sender, in_ref2_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_crd_sender, out_crd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_ref1_sender, out_ref1_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_ref2_sender, out_ref2_receiver) = parent.unbounded::<Token<u32, u32>>();

        let data = CrdJoinerData::<u32, u32> {
            in_crd1: in_crd1_receiver,
            in_ref1: in_ref1_receiver,
            in_crd2: in_crd2_receiver,
            in_ref2: in_ref2_receiver,
            out_crd: out_crd_sender,
            out_ref1: out_ref1_sender,
            out_ref2: out_ref2_sender,
        };
        let intersect = Union::new(data);
        let gen1 = GeneratorContext::new(in_crd1, in_crd1_sender);
        let gen2 = GeneratorContext::new(in_ref1, in_ref1_sender);
        let gen3 = GeneratorContext::new(in_crd2, in_crd2_sender);
        let gen4 = GeneratorContext::new(in_ref2, in_ref2_sender);
        let crd_checker = CheckerContext::new(out_crd, out_crd_receiver);
        let ref1_checker = CheckerContext::new(out_ref1, out_ref1_receiver);
        let ref2_checker = CheckerContext::new(out_ref2, out_ref2_receiver);

        parent.add_child(gen1);
        parent.add_child(gen2);
        parent.add_child(gen3);
        parent.add_child(gen4);
        parent.add_child(crd_checker);
        parent.add_child(ref1_checker);
        parent.add_child(ref2_checker);
        parent.add_child(intersect);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }
}
