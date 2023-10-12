use dam::{channel::utils::*, context_tools::*, dam_macros::context_macro};

use super::primitive::Token;

pub struct CrdMaskData<ValType: Clone, StopType: Clone> {
    pub in_crd_inner: Receiver<Token<ValType, StopType>>,
    pub in_crd_outer: Receiver<Token<ValType, StopType>>,
    pub out_crd_inner: Sender<Token<ValType, StopType>>,
    pub out_crd_outer: Sender<Token<ValType, StopType>>,
    pub in_ref_inner: Receiver<Token<ValType, StopType>>,
    pub out_ref_inner: Sender<Token<ValType, StopType>>,
}

#[context_macro]
pub struct CrdMask<ValType: Clone, StopType: Clone> {
    crd_mask_data: CrdMaskData<ValType, StopType>,
    predicate: fn(Token<ValType, StopType>, Token<ValType, StopType>) -> bool,
}

impl<ValType: DAMType, StopType: DAMType> CrdMask<ValType, StopType>
where
    CrdMask<ValType, StopType>: Context,
{
    pub fn new(
        crd_mask_data: CrdMaskData<ValType, StopType>,
        predicate: fn(Token<ValType, StopType>, Token<ValType, StopType>) -> bool,
    ) -> Self {
        let mask = CrdMask {
            crd_mask_data,
            predicate,
            context_info: Default::default(),
        };
        (mask.crd_mask_data.in_crd_inner).attach_receiver(&mask);
        (mask.crd_mask_data.in_crd_outer).attach_receiver(&mask);
        (mask.crd_mask_data.in_ref_inner).attach_receiver(&mask);
        (mask.crd_mask_data.out_crd_inner).attach_sender(&mask);
        (mask.crd_mask_data.out_crd_outer).attach_sender(&mask);
        (mask.crd_mask_data.out_ref_inner).attach_sender(&mask);

        mask
    }
}

impl<ValType, StopType> Context for CrdMask<ValType, StopType>
where
    ValType: DAMType
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut has_crd = false;
        // let icrd_vec: Vec<Token<ValType, StopType>> = Vec::new();
        loop {
            let out_ocrd = self.crd_mask_data.in_crd_outer.peek_next(&self.time);
            match self.crd_mask_data.in_crd_inner.dequeue(&self.time) {
                Ok(curr_in) => {
                    let curr_iref = self.crd_mask_data.in_ref_inner.dequeue(&self.time).unwrap();
                    let curr_ocrd = out_ocrd.unwrap().data.clone();
                    match curr_ocrd.clone() {
                        Token::Stop(stkn) => {
                            let channel_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::<ValType, StopType>::Stop(stkn.clone()),
                            );
                            self.crd_mask_data
                                .out_crd_outer
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            self.crd_mask_data.in_crd_outer.dequeue(&self.time).unwrap();
                        }
                        _ => (),
                    }
                    match curr_in.data {
                        Token::Val(val) => {
                            if (self.predicate)(curr_ocrd, Token::Val(val.clone())) == false {
                                let icrd_channel_elem = ChannelElement::new(
                                    self.time.tick() + 1,
                                    Token::<ValType, StopType>::Val(val.clone()),
                                );
                                self.crd_mask_data
                                    .out_crd_inner
                                    .enqueue(&self.time, icrd_channel_elem)
                                    .unwrap();
                                let iref_channel_elem = ChannelElement::new(
                                    self.time.tick() + 1,
                                    curr_iref.data.clone(),
                                );
                                self.crd_mask_data
                                    .out_ref_inner
                                    .enqueue(&self.time, iref_channel_elem)
                                    .unwrap();
                                has_crd = true;
                            }
                        }
                        Token::Stop(stkn) => {
                            if has_crd {
                                let icrd_channel_elem = ChannelElement::new(
                                    self.time.tick() + 1,
                                    Token::<ValType, StopType>::Stop(stkn.clone()),
                                );
                                self.crd_mask_data
                                    .out_crd_inner
                                    .enqueue(&self.time, icrd_channel_elem)
                                    .unwrap();
                                let iref_channel_elem = ChannelElement::new(
                                    self.time.tick() + 1,
                                    curr_iref.data.clone(),
                                );
                                self.crd_mask_data
                                    .out_ref_inner
                                    .enqueue(&self.time, iref_channel_elem)
                                    .unwrap();
                                let ocrd_channel_elem =
                                    ChannelElement::new(self.time.tick() + 1, curr_ocrd.clone());
                                self.crd_mask_data
                                    .out_crd_outer
                                    .enqueue(&self.time, ocrd_channel_elem)
                                    .unwrap();
                                has_crd = false;
                            }
                            self.crd_mask_data.in_crd_outer.dequeue(&self.time).unwrap();
                        }
                        Token::Done => {
                            let channel_elem =
                                ChannelElement::new(self.time.tick() + 1, Token::Done);
                            self.crd_mask_data
                                .out_crd_inner
                                .enqueue(&self.time, channel_elem.clone())
                                .unwrap();
                            self.crd_mask_data
                                .out_ref_inner
                                .enqueue(&self.time, channel_elem.clone())
                                .unwrap();
                            self.crd_mask_data
                                .out_crd_outer
                                .enqueue(&self.time, channel_elem.clone())
                                .unwrap();
                            return;
                        }
                        _ => {
                            dbg!(curr_in.data.clone());
                            panic!("Invalid case found");
                        }
                    }
                }
                Err(_) => {
                    panic!("Error encountered!");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::{
//         context::{checker_context::CheckerContext, generator_context::GeneratorContext},
//         simulation::Program,
//         templates::sam::primitive::Token,
//         token_vec,
//     };

//     use super::{CrdMask, CrdMaskData};

//     #[test]
//     fn test_tril_mask() {
//         let in_crd_outer = || token_vec!(u32; u32; 0, 1, 2, "S0", "D").into_iter();
//         let in_crd_inner =
//             || token_vec!(u32; u32; 0, 1, 3, "S0", 0, 1, 2, "S0", 0, 1, 2, "S1", "D").into_iter();
//         let in_ref_inner =
//             || token_vec!(u32; u32; 0, 1, 2, "S0", 3, 4, 5, "S0", 6, 7, 8, "S1", "D").into_iter();

//         let out_crd_outer = || token_vec!(u32; u32; 0, 1, 2, "S0", "D").into_iter();
//         let out_crd_inner =
//             || token_vec!(u32; u32; 0, 1, 3, "S0", 1, 2, "S0", 2, "S1", "D").into_iter();
//         let out_ref_inner =
//             || token_vec!(u32; u32; 0, 1, 2, "S0", 4, 5, "S0", 8, "S1", "D").into_iter();
//         mask_test(
//             in_crd_outer,
//             in_crd_inner,
//             in_ref_inner,
//             out_crd_outer,
//             out_crd_inner,
//             out_ref_inner,
//         );
//     }

//     #[test]
//     fn test_tril_mask1() {
//         let in_crd_outer = || token_vec!(u32; u32; 4, 1, 2, "S0", "D").into_iter();
//         let in_crd_inner =
//             || token_vec!(u32; u32; 0, 1, 3, "S0", 0, 1, 2, "S0", 0, 1, 2, "S1", "D").into_iter();
//         let in_ref_inner =
//             || token_vec!(u32; u32; 0, 1, 2, "S0", 3, 4, 5, "S0", 6, 7, 8, "S1", "D").into_iter();

//         let out_crd_outer = || token_vec!(u32; u32; 1, 2, "S0", "D").into_iter();
//         let out_crd_inner = || token_vec!(u32; u32; 1, 2, "S0", 2, "S1", "D").into_iter();
//         let out_ref_inner = || token_vec!(u32; u32;  4, 5, "S0", 8, "S1", "D").into_iter();
//         mask_test(
//             in_crd_outer,
//             in_crd_inner,
//             in_ref_inner,
//             out_crd_outer,
//             out_crd_inner,
//             out_ref_inner,
//         );
//     }

//     fn mask_test<IRT1, IRT2, IRT3, ORT1, ORT2, ORT3>(
//         in_crd_outer: fn() -> IRT2,
//         in_crd_inner: fn() -> IRT1,
//         in_ref_inner: fn() -> IRT3,
//         out_crd_outer: fn() -> ORT2,
//         out_crd_inner: fn() -> ORT1,
//         out_ref_inner: fn() -> ORT3,
//     ) where
//         IRT1: Iterator<Item = Token<u32, u32>> + 'static,
//         IRT2: Iterator<Item = Token<u32, u32>> + 'static,
//         IRT3: Iterator<Item = Token<u32, u32>> + 'static,
//         ORT1: Iterator<Item = Token<u32, u32>> + 'static,
//         ORT2: Iterator<Item = Token<u32, u32>> + 'static,
//         ORT3: Iterator<Item = Token<u32, u32>> + 'static,
//     {
//         let mut parent = ProgramBuilder::default();
//         let (mask_in_crd_sender, mask_in_crd_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let (mask_in_ocrd_sender, mask_in_ocrd_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let (mask_in_ref_sender, mask_in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();

//         let (mask_out_crd_sender, mask_out_crd_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let (mask_out_ocrd_sender, mask_out_ocrd_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let (mask_out_ref_sender, mask_out_ref_receiver) = parent.unbounded::<Token<u32, u32>>();

//         let gen1 = GeneratorContext::new(in_crd_outer, mask_in_ocrd_sender);
//         let gen2 = GeneratorContext::new(in_crd_inner, mask_in_crd_sender);
//         let gen3 = GeneratorContext::new(in_ref_inner, mask_in_ref_sender);
//         let ocrd_checker = CheckerContext::new(out_crd_outer, mask_out_ocrd_receiver);
//         let icrd_checker = CheckerContext::new(out_crd_inner, mask_out_crd_receiver);
//         let iref_checker = CheckerContext::new(out_ref_inner, mask_out_ref_receiver);

//         let mask_data = CrdMaskData::<u32, u32> {
//             in_crd_inner: mask_in_crd_receiver,
//             in_ref_inner: mask_in_ref_receiver,
//             in_crd_outer: mask_in_ocrd_receiver,
//             out_crd_inner: mask_out_crd_sender,
//             out_crd_outer: mask_out_ocrd_sender,
//             out_ref_inner: mask_out_ref_sender,
//         };
//         let mask = CrdMask::new(mask_data, |x, y| x > y);
//         parent.add_child(gen1);
//         parent.add_child(gen2);
//         parent.add_child(gen3);
//         parent.add_child(ocrd_checker);
//         parent.add_child(icrd_checker);
//         parent.add_child(iref_checker);
//         parent.add_child(mask);
//         parent.init();
//         parent.run();
//     }
// }
