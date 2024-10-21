use super::primitive::Token;
use dam::{
    context_tools::*,
    dam_macros::{context_macro, event_type},
    structures::Time,
};
use serde::{Deserialize, Serialize};

#[context_macro]
pub struct Binary<ValType: Clone, StopType: Clone, F> {
    pub in_val1: Receiver<Token<ValType, StopType>>,
    pub in_val2: Receiver<Token<ValType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    pub binary_func: F,
    pub block_size: u64,
    pub latency: u64,
    pub ii: u64,
}

impl<ValType: DAMType, StopType: DAMType, F> Binary<ValType, StopType, F>
where
    Binary<ValType, StopType, F>: Context,
{
    pub fn new(
        in_val1: Receiver<Token<ValType, StopType>>,
        in_val2: Receiver<Token<ValType, StopType>>,
        out_val: Sender<Token<ValType, StopType>>,
        binary_func: F,
        block_size: u64,
        latency: u64,
        ii: u64,
    ) -> Self {
        let unary = Binary {
            in_val1,
            in_val2,
            out_val,
            binary_func,
            block_size,
            latency,
            ii,
            context_info: Default::default(),
        };
        (unary).in_val1.attach_receiver(&unary);
        (unary).in_val2.attach_receiver(&unary);
        (unary).out_val.attach_sender(&unary);

        unary
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[event_type]
pub struct BinaryLogData {
    val: f32,
}

impl<ValType, StopType, F> Context for Binary<ValType, StopType, F>
where
    ValType: DAMType,
    StopType: DAMType + std::cmp::PartialEq,
    F: Fn(ValType, ValType) -> ValType + Sync + Send,
    // f32: From<ValType>,
{
    fn run(&mut self) {
        loop {
            //TODO: Dequeue from input channel
            // let val_deq = self.in_val.dequeue(&self.time);
            match (
                self.in_val1.dequeue(&self.time),
                self.in_val2.dequeue(&self.time),
            ) {
                (Ok(curr_in1), Ok(curr_in2)) => {
                    match (curr_in1.data.clone(), curr_in2.data.clone()) {
                        (Token::Val(val1), Token::Val(val2)) => {
                            let out_val = (self.binary_func)(val1, val2);
                            let out_val_elem = ChannelElement::new(
                                self.time.tick() + self.latency,
                                Token::<ValType, StopType>::Val(out_val.clone()),
                            );
                            // println!("Value: {:?}", Token::<ValType, StopType>::Val(out_val.clone()));
                            self.out_val.enqueue(&self.time, out_val_elem).unwrap();
                        }
                        (Token::Stop(stkn1), Token::Stop(stkn2)) => {
                            assert_eq!(stkn1.clone(), stkn2.clone(), "Stop tokens don't match");
                            let out_val_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::<ValType, StopType>::Stop(stkn1.clone()),
                            );
                            self.out_val.enqueue(&self.time, out_val_elem).unwrap();
                        }
                        (Token::Done, Token::Done) => {
                            let out_val_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::<ValType, StopType>::Done,
                            );
                            self.out_val.enqueue(&self.time, out_val_elem).unwrap();
                            return;
                        },
                        _ => panic!(
                            "Should not reach this case: {:?}",
                            (curr_in1.data.clone(), curr_in2.data.clone())
                        ),
                    }
                }
                _ => {
                    panic!("Reached error case")
                },
            }
            //TODO: Add initiation interval
            self.time.incr_cycles(self.ii);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Mul;

    use dam::{
        logging::LogEvent,
        simulation::{
            InitializationOptions, LogFilterKind, LoggingOptions, MongoOptionsBuilder,
            ProgramBuilder, RunOptionsBuilder,
        },
        utility_contexts::{CheckerContext, GeneratorContext},
    };

    use crate::{
        templates::{
            primitive::Token,
            binary::{Binary, BinaryLogData},
        },
        token_vec,
    };

    #[test]
    fn relu_test() {
        let in_val = || {
            token_vec!(f32; u32; 0.0, -1.0, -2.0, 3.0, "S0", 0.0, 1.0, 2.0, 3.0, 4.0, "S1", "D")
                .into_iter()
        };

        let out_val = || {
            token_vec!(f32; u32; 0.0, 0.0, 0.0, 3.0, "S0", 0.0, 1.0, 2.0, 3.0, 4.0, "S1", "D")
                .into_iter()
        };

        let max_func = |val: f32| -> f32 { val.max(0.0) };
        unary_test(in_val, out_val, max_func);
    }

    #[test]
    fn scalar_mul_test() {
        let in_val = || {
            token_vec!(f32; u32; 0.0, -1.0, -2.0, 3.0, "S0", 0.0, 1.0, 2.0, 3.0, 4.0, "S1", "D")
                .into_iter()
        };

        let out_val = || {
            token_vec!(f32; u32; 0.0, -2.0, -4.0, 6.0, "S0", 0.0, 2.0, 4.0, 6.0, 8.0, "S1", "D")
                .into_iter()
        };

        let mul_func = |val: f32| -> f32 { val.mul(2.0) };
        unary_test(in_val, out_val, mul_func);
    }

    fn unary_test<IRT, ORT>(
        in_val: fn() -> IRT,
        out_val: fn() -> ORT,
        binary_func: impl Fn(f32) -> f32 + Send + Sync,
    ) where
        IRT: Iterator<Item = Token<f32, u32>> + 'static,
        ORT: Iterator<Item = Token<f32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let chan_size = 128;

        let (out_val_sender, out_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
        let (in_val_sender, in_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);

        let max = Binary::new(in_val_receiver, out_val_sender, binary_func);

        let in_val = GeneratorContext::new(in_val, in_val_sender);
        let out_checker = CheckerContext::new(out_val, out_val_receiver);
        parent.add_child(max);
        parent.add_child(in_val);
        parent.add_child(out_checker);

        // let run_options = RunOptions::default();
        let run_options = RunOptionsBuilder::default().log_filter(LogFilterKind::Blanket(
            dam::logging::LogFilter::Some([BinaryLogData::NAME.to_owned()].into()),
        ));
        let run_options = run_options.logging(LoggingOptions::Mongo(
            MongoOptionsBuilder::default()
                .db("unary_log".to_string())
                .uri("mongodb://127.0.0.1:27017".to_string())
                .build()
                .unwrap(),
        ));
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(run_options.build().unwrap());
        dbg!(executed.elapsed_cycles());
    }
}
