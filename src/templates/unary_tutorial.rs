use super::primitive::Token;
use dam::{channel::DequeueError, context_tools::*, dam_macros::{context_macro, event_type}};
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

impl<ValType, StopType, F> Context for Unary<ValType, StopType, F>
where
    ValType: DAMType,
    StopType: DAMType,
    F: Fn(ValType) -> ValType + Sync + Send,
{
    fn run(&mut self) {
        loop {
            //TODO: Dequeue from input channel
            let val_deq: Result<ChannelElement<Token<ValType, StopType>>, DequeueError> = todo!();
            match val_deq {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        //TODO: Add logic for when we receive a value on the stream
                        //TODO: Enqueue output to output value channel

                        //TODO: Optional! Add logging event
                    }
                    Token::Stop(stkn) => {
                        //TODO: Add logic for when we receive a stop token on the stream
                        //TODO: Enqueue stop token output to output value channel
                    }
                    Token::Done => {
                        //TODO: Add logic for when we receive a done token on the stream
                        //TODO: Enqueue done behavior output to output value channel
                    }
                    _ => {
                        panic!("Invalid token found in stream");
                    }
                },
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            //TODO: Add initiation interval cycle increment
        }
    }
}

#[cfg(test)]
mod tests {
    use dam::{
        logging::LogEvent, simulation::{
            InitializationOptions, LogFilterKind, LoggingOptions, MongoOptionsBuilder, RunOptions, ProgramBuilder, RunOptionsBuilder
        }, utility_contexts::{CheckerContext, GeneratorContext}
    };

    use crate::{
        templates::{primitive::Token, unary::{Unary, UnaryLogData}},
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

        // Define lambda for computing relu (max(input_val, 0))
        let max_func = |val: f32| -> f32 { val.max(0.0) };

        unary_test(in_val, out_val, max_func);
    }

    fn unary_test<IRT, ORT>(
        in_val: fn() -> IRT,
        out_val: fn() -> ORT,
        unary_func: impl Fn(f32) -> f32 + Send + Sync,
    ) where
        IRT: Iterator<Item = Token<f32, u32>> + 'static,
        ORT: Iterator<Item = Token<f32, u32>> + 'static,
    {
        // Declare program builder
        let mut parent = ProgramBuilder::default();
        // Declare channel size
        let chan_size = 8;

        //TODO: Declare channels

        //TODO: Declare unary node with input and output channels and unary function

        //TODO: Use generator context to initialize input stream of input channel
        //TODO: Use checker context to check for correctness by checking content of unary output channel with expected output that is passed in

        //TODO: Register contexts to program builder, program builder now owns all contexts to initialize and run

        // Initialize and run program using program builder 
        let run_options = RunOptionsBuilder::default();
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(run_options.build().unwrap());
        dbg!(executed.elapsed_cycles());
    }
}
