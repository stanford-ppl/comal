use super::primitive::Token;
use dam::{context_tools::*, dam_macros::context_macro};

#[context_macro]
pub struct ALUMul<ValType: Clone, StopType: Clone> {
    pub in_val1: Receiver<Token<ValType, StopType>>,
    pub in_val2: Receiver<Token<ValType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
}

impl<ValType: DAMType, StopType: DAMType> ALUMul<ValType, StopType>
where
    ALUMul<ValType, StopType>: Context,
{
    pub fn new(
        in_val1: Receiver<Token<ValType, StopType>>,
        in_val2: Receiver<Token<ValType, StopType>>,
        out_val: Sender<Token<ValType, StopType>>,
    ) -> Self {
        let alu = ALUMul {
            in_val1,
            in_val2,
            out_val,
            context_info: Default::default(),
        };
        (alu).in_val1.attach_receiver(&alu);
        (alu).in_val2.attach_receiver(&alu);
        (alu).out_val.attach_sender(&alu);

        alu
    }
}

impl<ValType, StopType> Context for ALUMul<ValType, StopType>
where
    ValType: DAMType
        + std::cmp::PartialEq
        + std::cmp::PartialOrd
        + std::ops::Mul<ValType, Output = ValType>,
    StopType: DAMType + std::cmp::PartialEq,
{
    fn init(&mut self) {}
    fn run(&mut self) {
        loop {
            let val1_deq = self.in_val1.peek_next(&self.time);
            let val2_deq = self.in_val2.peek_next(&self.time);
            match (val1_deq, val2_deq) {
                (Ok(val1), Ok(val2)) => match (val1.data, val2.data) {
                    (Token::Val(val1), Token::Val(val2)) => {
                        let res = val1 * val2;
                        self.out_val
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    self.time.tick() + 1,
                                    Token::<ValType, StopType>::Val(res),
                                ),
                            )
                            .unwrap();
                        self.in_val1.dequeue(&self.time).unwrap();
                        self.in_val2.dequeue(&self.time).unwrap();
                    }
                    (Token::Val(_), Token::Stop(_)) => {
                        self.in_val1.dequeue(&self.time).unwrap();
                    }
                    (Token::Stop(_), Token::Val(_)) => {
                        self.in_val2.dequeue(&self.time).unwrap();
                    }
                    (Token::Stop(stkn1), Token::Stop(stkn2)) => {
                        assert_eq!(stkn1.clone(), stkn2.clone(), "Stop tokens must be the same");
                        self.out_val
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    self.time.tick() + 1,
                                    Token::<ValType, StopType>::Stop(stkn1),
                                ),
                            )
                            .unwrap();
                        self.in_val1.dequeue(&self.time).unwrap();
                        self.in_val2.dequeue(&self.time).unwrap();
                    }
                    (Token::Done, Token::Done) => {
                        self.out_val
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    self.time.tick() + 1,
                                    Token::<ValType, StopType>::Done,
                                ),
                            )
                            .unwrap();
                        return;
                    }
                    _ => todo!(),
                },
                (_, _) => todo!(),
            }
            self.time.incr_cycles(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use dam::{
        simulation::{InitializationOptions, ProgramBuilder, RunOptions},
        utility_contexts::{CheckerContext, GeneratorContext, PrinterContext},
    };

    use crate::{
        templates::{new_alu::ALUMul, primitive::Token},
        token_vec,
    };

    #[test]
    fn mul_test() {
        let in_val1 = || {
            token_vec!(f32; u32; 0.0, -1.0, -2.0, 3.0, "S0", 0.0, 1.0, 2.0, 3.0, 4.0, "S1", "D")
                .into_iter()
        };
        let in_val2 = || token_vec!(f32; u32; "S0", "S1", "D").into_iter();

        let out_val = || token_vec!(f32; u32; "S0", "S1", "D").into_iter();

        alu_max_test(in_val1, in_val2, out_val);
    }

    fn alu_max_test<IRT, ORT>(in_val1: fn() -> IRT, in_val2: fn() -> IRT, out_val: fn() -> ORT)
    where
        IRT: Iterator<Item = Token<f32, u32>> + 'static,
        ORT: Iterator<Item = Token<f32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let chan_size = 128;

        let (out_val_sender, out_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
        let (in_val1_sender, in_val1_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
        let (in_val2_sender, in_val2_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);

        let max = ALUMul::new(in_val1_receiver, in_val2_receiver, out_val_sender);

        let in_val1 = GeneratorContext::new(in_val1, in_val1_sender);
        let in_val2 = GeneratorContext::new(in_val2, in_val2_sender);
        let out_checker = PrinterContext::new(out_val_receiver);
        parent.add_child(max);
        parent.add_child(in_val1);
        parent.add_child(in_val2);
        parent.add_child(out_checker);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }
}
