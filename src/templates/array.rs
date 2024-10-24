use std::io;
use std::io::prelude::*;

use dam::structures::{Identifiable, Time};
use dam::{
    context_tools::*,
    dam_macros::{context_macro, event_type},
    structures::Identifier,
};
use serde::{Deserialize, Serialize};

use super::primitive::Token;

pub struct ArrayData<RefType: Clone, ValType: Clone, StopType: Clone> {
    pub in_ref: Receiver<Token<RefType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    pub block_size: usize,
}

#[context_macro]
pub struct Array<RefType: Clone, ValType: Clone, StopType: Clone> {
    array_data: ArrayData<RefType, ValType, StopType>,
    val_arr: Vec<ValType>,
}

impl<RefType: DAMType, ValType: DAMType, StopType: DAMType> Array<RefType, ValType, StopType>
where
    Array<RefType, ValType, StopType>: Context,
{
    pub fn new(array_data: ArrayData<RefType, ValType, StopType>, val_arr: Vec<ValType>) -> Self {
        let arr = Array {
            array_data,
            val_arr,
            context_info: Default::default(),
        };
        (arr.array_data.in_ref).attach_receiver(&arr);
        (arr.array_data.out_val).attach_sender(&arr);

        arr
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[event_type]
pub struct ArrayLog {
    in_ref: Token<u32, u32>,
    val: Token<f32, u32>,
}

impl<RefType, ValType, StopType> Context for Array<RefType, ValType, StopType>
where
    RefType: DAMType
        + std::ops::Mul<RefType, Output = RefType>
        + std::ops::Add<RefType, Output = RefType>,
    RefType: TryInto<usize>,
    <RefType as TryInto<usize>>::Error: std::fmt::Debug,
    ValType: DAMType,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
    // Token<u32, u32>: From<Token<RefType, StopType>>,
    // Token<f32, u32>: From<Token<ValType, StopType>>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let id = Identifier { id: 0 };
        let curr_id = self.id();
        let mut count = 0;
        loop {
            match self.array_data.in_ref.dequeue(&self.time) {
                Ok(curr_in) => {
                    let data = curr_in.data;
                    match data.clone() {
                        Token::Val(val) => {
                            let idx: usize = val.try_into().unwrap();
                            let block_size = self.array_data.block_size;

                            // println!("Val: {:?}", self.val_arr.clone());
                            // println!("Len: {:?}", self.val_arr.clone().len());

                            // let mut stdin = io::stdin();
                            // let mut stdout = io::stdout();

                            // // We want the cursor to stay at the end of the line, so we print without a newline and flush manually.
                            // write!(stdout, "Press any key to continue...").unwrap();
                            // stdout.flush().unwrap();

                            // Read a single byte and discard
                            let _ = stdin.read(&mut [0u8]).unwrap();

                            let channel_elem = ChannelElement::new(
                                self.time.tick()
                                    + Time::new((block_size * block_size).try_into().unwrap()),
                                Token::Val(self.val_arr[idx].clone()),
                            );
                            self.array_data
                                .out_val
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            let out_val =
                                Token::Val::<ValType, StopType>(self.val_arr[idx].clone());
                            count += 1;
                            // let _ = dam::logging::log_event(&ArrayLog {
                            //     in_ref: data.clone().into(),
                            //     val: out_val.clone().into(),
                            // });
                            // println!("{:?}", out_val.clone());
                            if id == curr_id {
                                println!("ID: {:?}, Val: {:?}", id, out_val.clone());
                            }
                        }
                        Token::Stop(stkn) => {
                            let channel_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::Stop(stkn.clone()),
                            );
                            self.array_data
                                .out_val
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            let out_val = Token::<ValType, StopType>::Stop(stkn.clone());
                            // let _ = dam::logging::log_event(&ArrayLog {
                            //     in_ref: data.clone().into(),
                            //     val: out_val.clone().into(),
                            // });
                            if id == curr_id {
                                println!("ID: {:?}, Val: {:?}", id, out_val.clone());
                            }
                            // println!("{:?}", ValType::dam_size(&ValType));
                        }
                        Token::Empty => {
                            let channel_elem = ChannelElement::new(
                                self.time.tick() + 1,
                                Token::Val(ValType::default()),
                            );

                            self.array_data
                                .out_val
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            if id == curr_id {
                                println!(
                                    "ID: {:?}, Val: {:?}",
                                    id,
                                    Token::<ValType, StopType>::Val(ValType::default())
                                );
                            }
                        }
                        Token::Done => {
                            let channel_elem =
                                ChannelElement::new(self.time.tick() + 1, Token::Done);
                            self.array_data
                                .out_val
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            let out_val = Token::<ValType, StopType>::Done;
                            // let _ = dam::logging::log_event(&ArrayLog {
                            //     in_ref: data.clone().into(),
                            //     val: out_val.clone().into(),
                            // });
                            if id == curr_id {
                                println!("ID: {:?}, Val: {:?}", id, out_val.clone());
                            }
                            println!("Count {:?}: {:?}", curr_id, count);
                            return;
                        }
                    }
                }
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            // let num = 64 x 64;
            // self.time.incr_cycles(64*64);
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

    use super::Array;
    use super::ArrayData;

    #[test]
    fn array_2d_test() {
        let in_ref = || {
            token_vec![u32; u32; "N", 0, 1, 2, "S0", "N", "N", "S0", 2, 3, 4, "S0", "N", "N", "S1", "D"].into_iter()
        };
        let out_val = || {
            token_vec!(u32; u32; 0, 1, 2, 3, "S0", 0, 0, "S0", 3, 4, 5, "S0", 0, 0, "S1", "D")
                .into_iter()
        };
        let val_arr = vec![1u32, 2, 3, 4, 5];
        array_test(in_ref, out_val, val_arr);
    }

    fn array_test<IRT, ORT>(in_ref: fn() -> IRT, out_val: fn() -> ORT, val_arr: Vec<u32>)
    where
        IRT: Iterator<Item = Token<u32, u32>> + 'static,
        ORT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_val_sender, out_val_receiver) = parent.unbounded::<Token<u32, u32>>();
        let data = ArrayData::<u32, u32, u32> {
            in_ref: in_ref_receiver,
            out_val: out_val_sender,
        };
        let arr = Array::new(data, val_arr);
        let gen1 = GeneratorContext::new(in_ref, in_ref_sender);
        let out_val_checker = CheckerContext::new(out_val, out_val_receiver);
        parent.add_child(gen1);
        parent.add_child(out_val_checker);
        parent.add_child(arr);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }
}
