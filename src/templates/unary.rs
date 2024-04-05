use super::primitive::Token;
use dam::{context_tools::*, dam_macros::context_macro};

#[context_macro]
pub struct UnaryMax<ValType: Clone, StopType: Clone> {
    pub in_val: Receiver<Token<ValType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    pub input_scalar: ValType,
}

impl<ValType: DAMType, StopType: DAMType> UnaryMax<ValType, StopType>
where
    UnaryMax<ValType, StopType>: Context,
{
    pub fn new(
        in_val: Receiver<Token<ValType, StopType>>,
        out_val: Sender<Token<ValType, StopType>>,
        input_scalar: ValType,
    ) -> Self {
        let unary = UnaryMax {
            in_val,
            out_val,
            input_scalar: input_scalar,
            context_info: Default::default(),
        };
        (unary).in_val.attach_receiver(&unary);
        (unary).out_val.attach_sender(&unary);

        unary
    }
}

impl<ValType, StopType> Context for UnaryMax<ValType, StopType>
where
    ValType: DAMType + std::cmp::PartialEq + std::cmp::PartialOrd,
    StopType: DAMType + std::cmp::PartialEq,
{
    fn init(&mut self) {}
    fn run(&mut self) {
        loop {
            let val_deq = self.in_val.dequeue(&self.time);
            match val_deq {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        let max_val = if val < self.input_scalar.clone() {
                            self.input_scalar.clone()
                        } else {
                            val
                        };
                        let channel_elem = ChannelElement::new(
                            self.time.tick() + 1,
                            Token::Val(max_val),
                        );
                        self.out_val.enqueue(&self.time, channel_elem).unwrap();
                    }
                    tkn @ Token::Stop(_) | tkn @ Token::Done => {
                        let channel_elem = ChannelElement::new(self.time.tick() + 1, tkn.clone());
                        self.out_val.enqueue(&self.time, channel_elem).unwrap();
                        if tkn.clone() == Token::Done {
                            return;
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
    use dam::{
        simulation::{InitializationOptions, ProgramBuilder, RunOptions},
        utility_contexts::{CheckerContext, GeneratorContext},
    };

    use crate::{
        templates::{primitive::Token, unary::UnaryMax},
        token_vec,
    };

    #[test]
    fn relu_test() {
        let in_val =
            || token_vec!(f32; u32; 0.0, -1.0, -2.0, 3.0, "S0", 0.0, 1.0, 2.0, 3.0, 4.0, "S1", "D").into_iter();

        let out_val =
            || token_vec!(f32; u32; 0.0, 0.0, 0.0, 3.0, "S0", 0.0, 1.0, 2.0, 3.0, 4.0, "S1", "D").into_iter();

        unary_max_test(in_val, out_val, 0.0);
    }

    fn unary_max_test<IRT, ORT>(in_val: fn() -> IRT, out_val: fn() -> ORT, input_scalar: f32)
    where
        IRT: Iterator<Item = Token<f32, u32>> + 'static,
        ORT: Iterator<Item = Token<f32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let chan_size = 128;

        let (out_val_sender, out_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
        let (in_val_sender, in_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);

        let max = UnaryMax::new(in_val_receiver, out_val_sender, input_scalar);

        let in_val = GeneratorContext::new(in_val, in_val_sender);
        let out_checker = CheckerContext::new(out_val, out_val_receiver);
        parent.add_child(max);
        parent.add_child(in_val);
        parent.add_child(out_checker);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }
}
