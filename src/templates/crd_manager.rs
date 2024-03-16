use dam::{context_tools::*, dam_macros::context_macro};

use super::primitive::Token;

pub struct CrdManagerData<ValType: Clone, StopType: Clone> {
    pub in_crd_inner: Receiver<Token<ValType, StopType>>,
    pub in_crd_outer: Receiver<Token<ValType, StopType>>,
    pub out_crd_inner: Sender<Token<ValType, StopType>>,
    pub out_crd_outer: Sender<Token<ValType, StopType>>,
}

pub struct CrdDropData<InnerValType: Clone, OuterValType: Clone, StopType: Clone> {
    pub in_crd_inner: Receiver<Token<InnerValType, StopType>>,
    pub in_crd_outer: Receiver<Token<OuterValType, StopType>>,
    pub out_crd_inner: Sender<Token<InnerValType, StopType>>,
    pub out_crd_outer: Sender<Token<OuterValType, StopType>>,
}

#[context_macro]
pub struct CrdDrop<InnerValType: Clone, OuterValType: Clone, StopType: Clone> {
    crd_drop_data: CrdDropData<InnerValType, OuterValType, StopType>,
}

impl<InnerValType: DAMType, OuterValType: DAMType, StopType: DAMType> CrdDrop<InnerValType, OuterValType, StopType>
where
    CrdDrop<InnerValType, OuterValType, StopType>: Context,
{
    pub fn new(crd_drop_data: CrdDropData<InnerValType, OuterValType, StopType>) -> Self {
        let drop = CrdDrop {
            crd_drop_data,
            context_info: Default::default(),
        };
        (drop.crd_drop_data.in_crd_inner).attach_receiver(&drop);
        (drop.crd_drop_data.in_crd_outer).attach_receiver(&drop);
        (drop.crd_drop_data.out_crd_inner).attach_sender(&drop);
        (drop.crd_drop_data.out_crd_outer).attach_sender(&drop);

        drop
    }
}

impl<InnerValType, OuterValType, StopType> Context for CrdDrop<InnerValType, OuterValType, StopType>
where
    InnerValType: DAMType
        + std::ops::Mul<InnerValType, Output = InnerValType>
        + std::ops::Add<InnerValType, Output = InnerValType>
        + std::cmp::PartialOrd<InnerValType>
        + std::cmp::PartialEq,
    OuterValType: DAMType
        + std::ops::Mul<OuterValType, Output = OuterValType>
        + std::ops::Add<OuterValType, Output = OuterValType>
        + std::cmp::PartialOrd<OuterValType>
        + std::cmp::PartialEq,
    StopType: DAMType + std::ops::Add<u32, Output = StopType> + std::cmp::PartialEq,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        loop {
            let ocrd = self
                .crd_drop_data
                .in_crd_outer
                .peek_next(&self.time)
                .unwrap();
            let mut has_crd = false;

            match ocrd.data.clone() {
                Token::Val(val) => loop {
                    let icrd = self
                        .crd_drop_data
                        .in_crd_inner
                        .dequeue(&self.time)
                        .expect("Error getting icrd");
                    let chan_elem = ChannelElement::new(self.time.tick() + 1, icrd.data.clone());
                    self.crd_drop_data
                        .out_crd_inner
                        .enqueue(&self.time, chan_elem)
                        .unwrap();
                    match icrd.data {
                        Token::Val(_) => {
                            has_crd = true;
                        }
                        Token::Stop(_) => {
                            if has_crd {
                                let chan_elem =
                                    ChannelElement::new(self.time.tick() + 1, Token::Val(val));
                                self.crd_drop_data
                                    .out_crd_outer
                                    .enqueue(&self.time, chan_elem)
                                    .unwrap();
                            } else if let Token::Stop(stkn) = ocrd.data.clone() {
                                let chan_elem =
                                    ChannelElement::new(self.time.tick() + 1, Token::Stop(stkn));
                                self.crd_drop_data
                                    .out_crd_outer
                                    .enqueue(&self.time, chan_elem)
                                    .unwrap();
                            }

                            self.crd_drop_data.in_crd_outer.dequeue(&self.time).unwrap();
                            break;
                        }
                        Token::Done => {
                            assert!(ocrd.data.clone() == Token::Done);
                            return;
                        }
                        _ => {
                            panic!("Unexpected token");
                        }
                    }
                },
                Token::Stop(stkn) => {
                    let chan_elem = ChannelElement::new(self.time.tick() + 1, Token::Stop(stkn));
                    self.crd_drop_data
                        .out_crd_outer
                        .enqueue(&self.time, chan_elem)
                        .unwrap();
                    // if prev_ocrd_stkn {
                    //     let icrd =

                    //     let chan_elem =

                    //     enqueue(
                    //         &mut self.time,
                    //         &mut self.crd_drop_data.out_crd_inner,
                    //         chan_elem,
                    //     )

                    // } else {
                    self.crd_drop_data.in_crd_outer.dequeue(&self.time).unwrap();
                    // }
                }
                Token::Done => {
                    let icrd = self.crd_drop_data.in_crd_inner.dequeue(&self.time).unwrap();
                    if let Token::Done = icrd.data.clone() {
                        let chan_elem =
                            ChannelElement::new(self.time.tick() + 1, icrd.data.clone());
                        self.crd_drop_data
                            .out_crd_inner
                            .enqueue(&self.time, chan_elem)
                            .unwrap();
                    }
                    let chan_elem = ChannelElement::new(self.time.tick() + 1, Token::Done);
                    self.crd_drop_data
                        .out_crd_outer
                        .enqueue(&self.time, chan_elem)
                        .unwrap();
                    return;
                }
                _ => {
                    panic!("Unexpected token found");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

#[context_macro]
pub struct CrdHold<ValType: Clone, StopType: Clone> {
    crd_hold_data: CrdManagerData<ValType, StopType>,
}

impl<ValType: DAMType, StopType: DAMType> CrdHold<ValType, StopType>
where
    CrdHold<ValType, StopType>: Context,
{
    pub fn new(crd_hold_data: CrdManagerData<ValType, StopType>) -> Self {
        let hold = CrdHold {
            crd_hold_data,
            context_info: Default::default(),
        };
        (hold.crd_hold_data.in_crd_inner).attach_receiver(&hold);
        (hold.crd_hold_data.in_crd_outer).attach_receiver(&hold);
        (hold.crd_hold_data.out_crd_inner).attach_sender(&hold);
        (hold.crd_hold_data.out_crd_outer).attach_sender(&hold);

        hold
    }
}

impl<ValType, StopType> Context for CrdHold<ValType, StopType>
where
    ValType: DAMType
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        loop {
            let out_ocrd = self.crd_hold_data.in_crd_outer.peek_next(&self.time);
            match self.crd_hold_data.in_crd_inner.dequeue(&self.time) {
                Ok(curr_in) => {
                    let curr_ocrd = out_ocrd.unwrap().data.clone();

                    let in_channel_elem =
                        ChannelElement::new(self.time.tick() + 1, curr_in.data.clone());
                    self.crd_hold_data
                        .out_crd_inner
                        .enqueue(&self.time, in_channel_elem)
                        .unwrap();

                    match curr_in.data.clone() {
                        Token::Val(_) => {
                            let output = match curr_ocrd.clone() {
                                Token::Val(_) => curr_ocrd.clone(),
                                Token::Stop(_) => {
                                    self.crd_hold_data.in_crd_outer.dequeue(&self.time).unwrap();
                                    self.crd_hold_data
                                        .in_crd_outer
                                        .peek_next(&self.time)
                                        .unwrap()
                                        .data
                                }
                                _ => {
                                    panic!("Invalid token in output");
                                }
                            };
                            let channel_elem =
                                ChannelElement::new(self.time.tick() + 1, output.clone());
                            self.crd_hold_data
                                .out_crd_outer
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                        }
                        Token::Stop(_) => {
                            let channel_elem =
                                ChannelElement::new(self.time.tick() + 1, curr_in.data.clone());
                            self.crd_hold_data
                                .out_crd_outer
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            self.crd_hold_data.in_crd_outer.dequeue(&self.time).unwrap();
                        }
                        Token::Empty => todo!(),
                        tkn @ Token::Done => {
                            let channel_elem = ChannelElement::new(self.time.tick() + 1, tkn);
                            self.crd_hold_data
                                .out_crd_outer
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
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
    use dam::simulation::{InitializationOptions, ProgramBuilder, RunOptions};
    use dam::utility_contexts::*;

    use crate::templates::primitive::Token;
    use crate::token_vec;

    use super::CrdManagerData;
    use super::{CrdDrop, CrdHold};

    #[test]
    fn crd_drop_1d_test() {
        let in_ocrd = || token_vec!(u32; u32; 0, 1, "S0", "D").into_iter();
        let in_icrd = || token_vec!(u32; u32; 1, "S0", "S1", "D").into_iter();
        let out_ocrd = || token_vec!(u32; u32; 0, "S0", "D").into_iter();
        crd_drop_test(in_ocrd, in_icrd, out_ocrd);
    }

    #[test]
    fn crd_drop_1d_test1() {
        let in_ocrd = || token_vec!(u32; u32; 0, 1, 2, 3, "S0", "D").into_iter();
        let in_icrd = || token_vec!(u32; u32; 1, "S0", 1, "S0", "S0", 1, "S1", "D").into_iter();
        let out_ocrd = || token_vec!(u32; u32; 0, 1, 3, "S0", "D").into_iter();
        crd_drop_test(in_ocrd, in_icrd, out_ocrd);
    }

    #[test]
    fn crd_drop_1d_test2() {
        let in_ocrd = || token_vec!(u32; u32; 1, "S0", "D").into_iter();
        let in_icrd = || token_vec!(u32; u32; 1, 2,3, "S1", "D").into_iter();
        let out_ocrd = || token_vec!(u32; u32; 1, "S0", "D").into_iter();
        crd_drop_test(in_ocrd, in_icrd, out_ocrd);
    }

    #[test]
    fn crd_drop_1d_test3() {
        let in_ocrd = || token_vec!(u32; u32; 0, 1, "S0", "D").into_iter();
        let in_icrd = || token_vec!(u32; u32; 1, "S0", 1, "S1", "D").into_iter();
        let out_ocrd = || token_vec!(u32; u32; 0, 1, "S0", "D").into_iter();
        crd_drop_test(in_ocrd, in_icrd, out_ocrd);
    }

    #[test]
    fn crd_hold_1d_test() {
        let in_ocrd = || token_vec!(u32; u32; 0, 1, 2, "S0", "D").into_iter();
        let in_icrd = || token_vec!(u32; u32; 0, 2, "S0", 2, "S0", 2, "S1", "D").into_iter();
        let out_ocrd = || token_vec!(u32; u32; 0, 0, "S0", 1, "S0", 2, "S1", "D").into_iter();
        crd_hold_test(in_ocrd, in_icrd, out_ocrd);
    }

    #[test]
    fn crd_hold_1d_test1() {
        let in_ocrd = || token_vec!(u32; u32; 0, 2, "S0", 3, "S0", 4, "S1", "D").into_iter();
        let in_icrd = || {
            token_vec!(u32; u32; 0, 2, 3, "S0", 0, 2, 3, "S1", 0, "S1", 2, 3, "S2", "D").into_iter()
        };
        let out_ocrd = || {
            token_vec!(u32; u32; 0, 0, 0, "S0", 2, 2, 2, "S1", 3, "S1", 4, 4, "S2", "D").into_iter()
        };
        crd_hold_test(in_ocrd, in_icrd, out_ocrd);
    }

    #[test]
    fn crd_hold_1d_test2() {
        let in_ocrd = || token_vec!(u32; u32; 0, 1, 2, 5, "S0", "D").into_iter();
        let in_icrd = || {
            token_vec!(u32; u32; 1, 2, 5, "S0", 2, "S0", 2, "S0", 2, 3, 4, 5, "S1", "D").into_iter()
        };
        let out_ocrd = || {
            token_vec!(u32; u32; 0, 0, 0, "S0", 1, "S0", 2, "S0", 5, 5, 5, 5, "S1", "D").into_iter()
        };
        crd_hold_test(in_ocrd, in_icrd, out_ocrd);
    }

    fn crd_drop_test<IRT1, IRT2, ORT>(
        in_ocrd: fn() -> IRT1,
        in_icrd: fn() -> IRT2,
        out_ocrd: fn() -> ORT,
    ) where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        IRT2: Iterator<Item = Token<u32, u32>> + 'static,
        ORT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_ocrd_sender, in_ocrd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (in_icrd_sender, in_icrd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_ocrd_sender, out_ocrd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_icrd_sender, out_icrd_receiver) = parent.unbounded::<Token<u32, u32>>();

        let crd_drop_data = CrdManagerData::<u32, u32> {
            in_crd_outer: in_ocrd_receiver,
            in_crd_inner: in_icrd_receiver,
            out_crd_outer: out_ocrd_sender,
            out_crd_inner: out_icrd_sender,
        };

        let drop = CrdDrop::new(crd_drop_data);
        let ocrd_gen = GeneratorContext::new(in_ocrd, in_ocrd_sender);
        let icrd_gen = GeneratorContext::new(in_icrd, in_icrd_sender);
        let out_crd_checker = CheckerContext::new(out_ocrd, out_ocrd_receiver);
        let out_icrd_checker = CheckerContext::new(in_icrd, out_icrd_receiver);
        parent.add_child(ocrd_gen);
        parent.add_child(icrd_gen);
        parent.add_child(out_crd_checker);
        parent.add_child(out_icrd_checker);
        parent.add_child(drop);
        parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
    }

    fn crd_hold_test<IRT1, IRT2, ORT>(
        in_ocrd: fn() -> IRT1,
        in_icrd: fn() -> IRT2,
        out_ocrd: fn() -> ORT,
    ) where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        IRT2: Iterator<Item = Token<u32, u32>> + 'static,
        ORT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_ocrd_sender, in_ocrd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (in_icrd_sender, in_icrd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_ocrd_sender, out_ocrd_receiver) = parent.unbounded::<Token<u32, u32>>();

        let crd_hold_data = CrdManagerData::<u32, u32> {
            in_crd_outer: in_ocrd_receiver,
            in_crd_inner: in_icrd_receiver,
            out_crd_outer: out_ocrd_sender,
            out_crd_inner: parent.void(),
        };

        let drop = CrdHold::new(crd_hold_data);
        let ocrd_gen = GeneratorContext::new(in_ocrd, in_ocrd_sender);
        let icrd_gen = GeneratorContext::new(in_icrd, in_icrd_sender);
        let out_crd_checker = CheckerContext::new(out_ocrd, out_ocrd_receiver);
        parent.add_child(ocrd_gen);
        parent.add_child(icrd_gen);
        parent.add_child(out_crd_checker);
        parent.add_child(drop);
        parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
    }
}
