use super::primitive::Token;
use dam::{context_tools::*, dam_macros::context_macro};

#[context_macro]
pub struct StknDrop<ValType: Clone, StopType: Clone> {
    pub in_val: Receiver<Token<ValType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
}

impl<ValType: DAMType, StopType: DAMType> StknDrop<ValType, StopType>
where
    StknDrop<ValType, StopType>: Context,
{
    pub fn new(
        in_val: Receiver<Token<ValType, StopType>>,
        out_val: Sender<Token<ValType, StopType>>,
    ) -> Self {
        let stkn_drop = StknDrop {
            in_val,
            out_val,
            context_info: Default::default(),
        };
        (stkn_drop).in_val.attach_receiver(&stkn_drop);
        (stkn_drop).out_val.attach_sender(&stkn_drop);

        stkn_drop
    }
}

impl<ValType, StopType> Context for StknDrop<ValType, StopType>
where
    ValType: DAMType + std::cmp::PartialEq + std::cmp::PartialOrd,
    StopType: DAMType + std::ops::Add<u32, Output = StopType> + std::cmp::PartialEq,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut prev_stkn = true;
        loop {
            let val_deq = self.in_val.dequeue(&self.time);
            match val_deq {
                Ok(curr_in) => match curr_in.data {
                    tkn @ Token::Val(_) | tkn @ Token::Done => {
                        let channel_elem = ChannelElement::new(self.time.tick() + 1, tkn.clone());
                        self.out_val.enqueue(&self.time, channel_elem).unwrap();
                        if tkn == Token::Done {
                            return;
                        }
                        prev_stkn = false;
                    }
                    Token::Stop(stkn) => {
                        if !prev_stkn {
                            let channel_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::<ValType, StopType>::Stop(stkn),
                            );
                            self.out_val.enqueue(&self.time, channel_elem).unwrap();
                            prev_stkn = true;
                        }
                    }
                    _ => {
                        panic!("Invalid token found in stream");
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
    use dam::{simulation::*, utility_contexts::*};

    use crate::{templates::primitive::Token, token_vec};

    use super::StknDrop;
    #[test]
    fn stkn_drop_basic_test() {
        let in_val = || token_vec!(u32; u32; "S0", "S0", "S0", 0, "S1", "D").into_iter();
        let out_val = || token_vec!(u32; u32; 0, "S1", "D").into_iter();
        stkn_drop_test(in_val, out_val);
    }

    fn stkn_drop_test<IRT1, ORT1>(in_val: fn() -> IRT1, out_val: fn() -> ORT1)
    where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        ORT1: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let chan_size = 4;

        let mut parent = ProgramBuilder::default();
        let (in_val_sender, in_val_receiver) = parent.bounded::<Token<u32, u32>>(chan_size);
        let (out_val_sender, out_val_receiever) = parent.bounded::<Token<u32, u32>>(chan_size);

        let stkn_drop = StknDrop::new(in_val_receiver, out_val_sender);
        let in_val_gen = GeneratorContext::new(in_val, in_val_sender);
        let out_val_checker = CheckerContext::new(out_val, out_val_receiever);

        parent.add_child(in_val_gen);
        parent.add_child(stkn_drop);
        parent.add_child(out_val_checker);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }
}
