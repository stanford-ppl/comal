use dam::dam_macros::event_type;
use dam::structures::Identifiable;
use dam::{context_tools::*, dam_macros::context_macro, structures::Identifier};
use serde::{Deserialize, Serialize};

use super::primitive::{Repsiggen, Token};

pub struct RepeatData<ValType: Clone, StopType: Clone> {
    pub in_ref: Receiver<Token<ValType, StopType>>,
    pub in_repsig: Receiver<Repsiggen>,
    pub out_ref: Sender<Token<ValType, StopType>>,
}

#[context_macro]
pub struct Repeat<ValType: Clone, StopType: Clone> {
    repeat_data: RepeatData<ValType, StopType>,
}

#[derive(Serialize, Deserialize, Debug)]
#[event_type]
pub struct RepeatLog<T: dam::types::DAMType + serde::Serialize> {
    in_ref: Token<T, u32>,
    in_rep_sig: Repsiggen,
}

#[derive(Serialize, Deserialize, Debug)]
#[event_type]
pub struct RepsiggenLog<T: dam::types::DAMType + serde::Serialize> {
    in_ref: Token<T, u32>,
    out_rep_sig: Repsiggen,
}

impl<ValType: DAMType, StopType: DAMType> Repeat<ValType, StopType>
where
    Repeat<ValType, StopType>: Context,
{
    pub fn new(repeat_data: RepeatData<ValType, StopType>) -> Self {
        let repeat = Repeat {
            repeat_data,
            context_info: Default::default(),
        };
        (repeat.repeat_data.in_ref).attach_receiver(&repeat);
        (repeat.repeat_data.in_repsig).attach_receiver(&repeat);
        (repeat.repeat_data.out_ref).attach_sender(&repeat);

        repeat
    }
}

impl<ValType, StopType> Context for Repeat<ValType, StopType>
where
    ValType: DAMType
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>
        + serde::Serialize,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
    Token<ValType, u32>: From<Token<ValType, StopType>>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let id = Identifier { id: 0 };
        let curr_id = self.id();
        loop {
            let in_ref = self.repeat_data.in_ref.peek_next(&self.time);
            match self.repeat_data.in_repsig.dequeue(&self.time) {
                Ok(curr_in) => {
                    let curr_ref = in_ref.as_ref().unwrap().data.clone();
                    match curr_in.data {
                        Repsiggen::Repeat => {
                            let channel_elem =
                                ChannelElement::new(self.time.tick() + 1, curr_ref.clone());
                            self.repeat_data
                                .out_ref
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            if curr_id == id {
                                println!(
                                    "ID: {:?}, In ref: {:?}, Output: {:?}, In repsig: {:?}",
                                    curr_id,
                                    curr_ref.clone(),
                                    curr_ref.clone(),
                                    Repsiggen::Repeat
                                );
                            }
                            let _ = dam::logging::log_event(&RepeatLog {
                                in_ref: curr_ref.clone().into(),
                                in_rep_sig: Repsiggen::Repeat,
                            });
                        }
                        Repsiggen::Stop => {
                            // Always dequeue when we see a stop
                            self.repeat_data.in_ref.dequeue(&self.time).unwrap();
                            let output: Token<ValType, StopType>;
                            // If the dequeued token is a val, look at next token for stop level to increment 
                            if let Token::Val(_) = in_ref.as_ref().unwrap().data.clone() {
                                let next_tkn =
                                    self.repeat_data.in_ref.peek_next(&self.time).unwrap();
                                output = if let Token::Stop(stop_tkn) = next_tkn.data.clone() {
                                    self.repeat_data.in_ref.dequeue(&self.time).unwrap();
                                    Token::Stop(stop_tkn + 1)
                                } else {
                                    Token::Stop(StopType::default())
                                };
                            } else {
                                // If dequeued token is a stop token, increment but don't look at next token
                                output = if let Token::Stop(stop_tkn) = in_ref.as_ref().unwrap().data.clone() {
                                    // self.repeat_data.in_ref.dequeue(&self.time).unwrap();
                                    Token::Stop(stop_tkn + 1)
                                } else {
                                    Token::Stop(StopType::default())
                                };
                            }
                            let channel_elem =
                                ChannelElement::new(self.time.tick() + 1, output.clone());
                            self.repeat_data
                                .out_ref
                                .enqueue(&self.time, channel_elem)
                                .unwrap();
                            if curr_id == id {
                                println!(
                                    "ID: {:?}, In ref: {:?}, Output: {:?}, In repsig: {:?}",
                                    curr_id,
                                    in_ref.as_ref().unwrap().data.clone(),
                                    output.clone(),
                                    Repsiggen::Stop
                                );
                            }
                            let _ = dam::logging::log_event(&RepeatLog {
                                in_ref: output.clone().into(),
                                in_rep_sig: Repsiggen::Stop,
                            });
                        }
                        Repsiggen::Done => {
                            if let Token::Done = curr_ref.clone() {
                                let channel_elem =
                                    ChannelElement::new(self.time.tick() + 1, Token::Done);
                                self.repeat_data
                                    .out_ref
                                    .enqueue(&self.time, channel_elem.clone())
                                    .unwrap();
                                if curr_id == id {
                                    println!(
                                        "ID: {:?}, Output: {:?}",
                                        curr_id,
                                        Token::<ValType, StopType>::Done
                                    );
                                }
                                let _ = dam::logging::log_event(&RepeatLog {
                                    in_ref: channel_elem.clone().data.into(),
                                    in_rep_sig: Repsiggen::Done,
                                });
                            } else {
                                if curr_id == id {
                                    println!(
                                        "ID: {:?}, Output: {:?}",
                                        curr_id,
                                        // Token::<ValType, StopType>::Done
                                        curr_ref.clone()
                                    );
                                }
                                let _ = dam::logging::log_event(&RepeatLog {
                                    in_ref: curr_ref.clone().into(),
                                    in_rep_sig: Repsiggen::Repeat,
                                });
                                panic!("Input reference and repeat signal must both be on Done");
                            }
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

pub struct RepSigGenData<ValType: Clone, StopType: Clone> {
    pub input: Receiver<Token<ValType, StopType>>,
    pub out_repsig: Sender<Repsiggen>,
}

#[context_macro]
pub struct RepeatSigGen<ValType: Clone, StopType: Clone> {
    rep_sig_gen_data: RepSigGenData<ValType, StopType>,
}

impl<ValType: DAMType, StopType: DAMType> RepeatSigGen<ValType, StopType>
where
    RepeatSigGen<ValType, StopType>: Context,
{
    pub fn new(rep_sig_gen_data: RepSigGenData<ValType, StopType>) -> Self {
        let rep_sig_gen = RepeatSigGen {
            rep_sig_gen_data,
            context_info: Default::default(),
        };
        (rep_sig_gen.rep_sig_gen_data.input).attach_receiver(&rep_sig_gen);
        (rep_sig_gen.rep_sig_gen_data.out_repsig).attach_sender(&rep_sig_gen);

        rep_sig_gen
    }
}

impl<ValType, StopType> Context for RepeatSigGen<ValType, StopType>
where
    ValType: DAMType
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>
        + serde::Serialize,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
    Repsiggen: DAMType,
    Token<ValType, u32>: From<Token<ValType, StopType>>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let id = Identifier { id: 0 };
        let curr_id = self.id();
        loop {
            match self.rep_sig_gen_data.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    tkn @ Token::Val(_) | tkn @ Token::Empty => {
                        let channel_elem =
                            ChannelElement::new(self.time.tick() + 1, Repsiggen::Repeat);
                        self.rep_sig_gen_data
                            .out_repsig
                            .enqueue(&self.time, channel_elem)
                            .unwrap();
                        let _ = dam::logging::log_event(&RepsiggenLog {
                            in_ref: tkn.clone().into(),
                            out_rep_sig: Repsiggen::Repeat,
                        });
                        if curr_id == id {
                            println!(
                                "ID: {:?}, Ref: {:?}, Ref_sig: {:?}",
                                curr_id,
                                tkn.clone(),
                                Repsiggen::Repeat
                            );
                        }
                    }
                    tkn @ Token::Stop(_) => {
                        let channel_elem =
                            ChannelElement::new(self.time.tick() + 1, Repsiggen::Stop);
                        self.rep_sig_gen_data
                            .out_repsig
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        let _ = dam::logging::log_event(&RepsiggenLog {
                            out_rep_sig: Repsiggen::Stop,
                            in_ref: tkn.clone().into(),
                        });
                        if curr_id == id {
                            println!(
                                "ID: {:?}, Ref: {:?}, Ref_sig: {:?}",
                                curr_id,
                                tkn.clone(),
                                Repsiggen::Stop
                            );
                        }
                    }
                    Token::Done => {
                        let channel_elem =
                            ChannelElement::new(self.time.tick() + 1, Repsiggen::Done);
                        self.rep_sig_gen_data
                            .out_repsig
                            .enqueue(&self.time, channel_elem)
                            .unwrap();
                        let _ = dam::logging::log_event(&RepsiggenLog {
                            out_rep_sig: Repsiggen::Done,
                            in_ref: Token::<ValType, StopType>::Done.into(),
                        });
                        if curr_id == id {
                            println!(
                                "ID: {:?}, Ref: {:?}, Ref_sig: {:?}",
                                curr_id,
                                Token::<ValType, StopType>::Done,
                                Repsiggen::Done
                            );
                        }
                        return;
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
    use dam::simulation::*;
    use dam::utility_contexts::{CheckerContext, GeneratorContext, PrinterContext};

    use crate::templates::primitive::{Repsiggen, Token};
    use crate::{repsig_vec, token_vec};

    use super::RepSigGenData;
    use super::Repeat;
    use super::RepeatData;
    use super::RepeatSigGen;

    #[test]
    fn repeat_2d_test() {
        let in_ref = || token_vec!(u32; u32; 0, 1, "S0", 2, "S0", 3, "S1", "D").into_iter();
        let in_repsig = || {
            repsig_vec!("R", "R", "R", "S", "R", "R", "R", "S", "R", "S", "R", "R", "S", "D")
                .into_iter()
        };
        let out_ref = || {
            token_vec!(u32; u32; 0, 0, 0, "S0", 1, 1, 1, "S1", 2, "S1", 3, 3, "S2", "D").into_iter()
        };
        repeat_test(in_ref, in_repsig, out_ref);
    }

    #[test]
    fn repeat_1d_test() {
        let in_ref = || token_vec!(u32; u32; 0, 1, 2, "S0", "D").into_iter();
        let in_repsig = || repsig_vec!("R", "R", "S", "R", "S", "R", "S", "D").into_iter();
        let out_ref = || token_vec!(u32; u32; 0, 0, "S0", 1, "S0", 2, "S1", "D").into_iter();
        repeat_test(in_ref, in_repsig, out_ref);
    }

    #[test]
    fn repeat_1d_test1() {
        let in_ref = || token_vec!(u32; u32; 0, "S0", 1, "S1", "D").into_iter();
        let in_repsig = || repsig_vec!("S", "R", "S", "D").into_iter();
        let out_ref = || token_vec!(u32; u32; "S1", 2, "S2", "D").into_iter();
        repeat_test(in_ref, in_repsig, out_ref);
    }

    #[test]
    fn repsiggen_2d_test() {
        // let in_ref = || token_vec!(u32; u32; 0, 1, "S0", 2, "S0", 3, "S1", "D").into_iter();
        let in_ref =
            || token_vec!(u32; u32; 0, 0, 0, "S0", 1, 1, 1, "S0", 2, 2, 2, "S1", "D").into_iter();
        let out_repsig = || {
            repsig_vec!(
                "R", "R", "S", "S", "S", "R", "R", "S", "S", "S", "R", "R", "S", "S", "S", "D"
            )
            .into_iter()
        };
        repsiggen_test(in_ref, out_repsig);
    }

    #[test]
    fn full_repeat_2d_test() {
        let in_ref =
            || token_vec!(u32; u32; 0, 0, 0, "S0", 1, 1, 1, "S0", 2, 2, 2, "S1", "D").into_iter();
        let in_repsig_ref = || {
            token_vec!(u32; u32; 1, 4, "S0", "S0", "S1", 1, 4, "S0", "S0", "S1", 1, 4, "S0", "S0", "S1", "D").into_iter()
        };
        let out_ref = || token_vec!(u32; u32; 0, 0, "S0", 1, "S0", 2, "S1", "D").into_iter();

        full_repeat_test(in_repsig_ref, in_ref, out_ref);
    }

    fn full_repeat_test<IRT1, IRT2, ORT>(
        in_ref_sig: fn() -> IRT1,
        in_ref: fn() -> IRT2,
        out_ref: fn() -> ORT,
    ) where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        IRT2: Iterator<Item = Token<u32, u32>> + 'static,
        ORT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_repsig_ref_sender, in_repsig_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_repsig_sender, out_repsig_receiver) = parent.unbounded::<Repsiggen>();
        let repsig_data = RepSigGenData::<u32, u32> {
            input: in_repsig_ref_receiver,
            out_repsig: out_repsig_sender,
        };
        let repsig = RepeatSigGen::new(repsig_data);

        let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_ref_sender, out_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let data = RepeatData::<u32, u32> {
            in_ref: in_ref_receiver,
            in_repsig: out_repsig_receiver,
            out_ref: out_ref_sender,
        };
        let rep = Repeat::new(data);
        let repsig_gen = GeneratorContext::new(in_ref_sig, in_repsig_ref_sender);
        let gen1 = GeneratorContext::new(in_ref, in_ref_sender);

        // let val_checker = CheckerContext::new(out_ref, out_ref_receiver);
        let val_checker = PrinterContext::new(out_ref_receiver);
        parent.add_child(gen1);
        parent.add_child(repsig_gen);
        parent.add_child(val_checker);
        parent.add_child(rep);
        parent.add_child(repsig);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }

    fn repeat_test<IRT1, IRT2, ORT>(
        in_ref: fn() -> IRT1,
        in_repsig: fn() -> IRT2,
        out_ref: fn() -> ORT,
    ) where
        IRT1: Iterator<Item = Token<u32, u32>> + 'static,
        IRT2: Iterator<Item = Repsiggen> + 'static,
        ORT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (in_repsig_sender, in_repsig_receiver) = parent.unbounded::<Repsiggen>();
        let (out_ref_sender, out_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let data = RepeatData::<u32, u32> {
            in_ref: in_ref_receiver,
            in_repsig: in_repsig_receiver,
            out_ref: out_ref_sender,
        };
        let rep = Repeat::new(data);
        let gen1 = GeneratorContext::new(in_ref, in_ref_sender);
        let gen2 = GeneratorContext::new(in_repsig, in_repsig_sender);
        // let val_checker = CheckerContext::new(out_ref, out_ref_receiver);
        let test = PrinterContext::new(out_ref_receiver);
        parent.add_child(test);

        parent.add_child(gen1);
        parent.add_child(gen2);
        // parent.add_child(val_checker);
        parent.add_child(rep);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }

    fn repsiggen_test<IRT, ORT>(in_ref: fn() -> IRT, out_repsig: fn() -> ORT)
    where
        IRT: Iterator<Item = Token<u32, u32>> + 'static,
        ORT: Iterator<Item = Repsiggen> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_repsig_sender, out_repsig_receiver) = parent.unbounded::<Repsiggen>();
        let data = RepSigGenData::<u32, u32> {
            input: in_ref_receiver,
            out_repsig: out_repsig_sender,
        };

        let repsig = RepeatSigGen::new(data);
        let gen1 = GeneratorContext::new(in_ref, in_ref_sender);
        let val_checker = CheckerContext::new(out_repsig, out_repsig_receiver);

        parent.add_child(gen1);
        parent.add_child(val_checker);
        parent.add_child(repsig);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }
}
