use super::primitive::Token;
use dam::{
    context_tools::*,
    dam_macros::{context_macro, event_type},
};
use serde::{Deserialize, Serialize};

#[context_macro]
pub struct Unary<ValType: Clone, StopType: Clone, F> {
    pub in_val: Receiver<Token<ValType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    pub unary_func: F,
}

impl<ValType: DAMType, StopType: DAMType, F> Unary<ValType, StopType, F>
where
    Unary<ValType, StopType, F>: Context,
{
    pub fn new(
        in_val: Receiver<Token<ValType, StopType>>,
        out_val: Sender<Token<ValType, StopType>>,
        unary_func: F,
    ) -> Self {
        let unary = Unary {
            in_val,
            out_val,
            unary_func,
            context_info: Default::default(),
        };
        (unary).in_val.attach_receiver(&unary);
        (unary).out_val.attach_sender(&unary);

        unary
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[event_type]
pub struct UnaryLogData {
    val: f32,
}

impl<ValType, StopType, F> Context for Unary<ValType, StopType, F>
where
    ValType: DAMType,
    StopType: DAMType,
    F: Fn(ValType) -> ValType + Sync + Send,
    // f32: From<ValType>,
{
    fn run(&mut self) {
        loop {
            //TODO: Dequeue from input channel
            let val_deq = self.in_val.dequeue(&self.time);
            match val_deq {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        //TODO: Add logic for when we receive a value on the stream
                        //TODO: Enqueue output to output value channel

                        // let log_val: f32 = val.clone().into();
                        // if log_val < 0.0 {
                        //     dam::logging::log_event(&UnaryLogData { val: log_val }).unwrap();
                        // }

                        let out_val = (self.unary_func)(val);
                        let out_val_elem = ChannelElement::new(
                            self.time.tick() + 64*64,
                            Token::<ValType, StopType>::Val(out_val),
                        );
                        self.out_val.enqueue(&self.time, out_val_elem).unwrap();
                    }
                    Token::Stop(stkn) => {
                        //TODO: Add logic for when we receive a stop token on the stream
                        //TODO: Enqueue stop token output to output value channel
                        let out_val_elem = ChannelElement::new(
                            self.time.tick() + 1,
                            Token::<ValType, StopType>::Stop(stkn),
                        );
                        self.out_val.enqueue(&self.time, out_val_elem).unwrap();
                    }
                    Token::Done => {
                        //TODO: Add logic for when we receive a done token on the stream
                        //TODO: Enqueue done behavior output to output value channel
                        let out_val_elem = ChannelElement::new(
                            self.time.tick() + 1,
                            Token::<ValType, StopType>::Done,
                        );
                        self.out_val.enqueue(&self.time, out_val_elem).unwrap();
                        return;
                    }
                    _ => {
                        panic!("Invalid token found in stream");
                    }
                },
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            //TODO: Add initiation interval
            self.time.incr_cycles(1);
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
            unary::{Unary, UnaryLogData},
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
        unary_func: impl Fn(f32) -> f32 + Send + Sync,
    ) where
        IRT: Iterator<Item = Token<f32, u32>> + 'static,
        ORT: Iterator<Item = Token<f32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let chan_size = 128;

        let (out_val_sender, out_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
        let (in_val_sender, in_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);

        let max = Unary::new(in_val_receiver, out_val_sender, unary_func);

        let in_val = GeneratorContext::new(in_val, in_val_sender);
        let out_checker = CheckerContext::new(out_val, out_val_receiver);
        parent.add_child(max);
        parent.add_child(in_val);
        parent.add_child(out_checker);

        // let run_options = RunOptions::default();
        let run_options = RunOptionsBuilder::default().log_filter(LogFilterKind::Blanket(
            dam::logging::LogFilter::Some([UnaryLogData::NAME.to_owned()].into()),
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
