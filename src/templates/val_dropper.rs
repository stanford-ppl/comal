use dam::{channel::utils::*, context_tools::*, dam_macros::context_macro};

use super::primitive::Token;

pub struct ValDropData<CrdType: Clone, ValType: Clone, StopType: Clone> {
    pub in_val: Receiver<Token<ValType, StopType>>,
    pub in_crd: Receiver<Token<CrdType, StopType>>,
    pub out_val: Sender<Token<ValType, StopType>>,
    pub out_crd: Sender<Token<CrdType, StopType>>,
}

#[context_macro]
pub struct ValDrop<CrdType: Clone, ValType: Clone, StopType: Clone> {
    val_drop_data: ValDropData<CrdType, ValType, StopType>,
}

impl<CrdType: DAMType, ValType: DAMType, StopType: DAMType> ValDrop<CrdType, ValType, StopType>
where
    ValDrop<CrdType, ValType, StopType>: Context,
{
    pub fn new(array_data: ValDropData<CrdType, ValType, StopType>) -> Self {
        let val_drop = ValDrop {
            val_drop_data: array_data,
            context_info: Default::default(),
        };
        (val_drop.val_drop_data.in_val).attach_receiver(&val_drop);
        (val_drop.val_drop_data.in_crd).attach_receiver(&val_drop);
        (val_drop.val_drop_data.out_val).attach_sender(&val_drop);
        (val_drop.val_drop_data.out_crd).attach_sender(&val_drop);

        val_drop
    }
}

impl<CrdType, ValType, StopType> Context for ValDrop<CrdType, ValType, StopType>
where
    CrdType: DAMType + std::cmp::PartialEq + std::cmp::PartialOrd,
    ValType: DAMType + std::cmp::PartialEq + std::cmp::PartialOrd,
    StopType: DAMType + std::ops::Add<u32, Output = StopType> + std::cmp::PartialEq,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let mut prev_stkn = false;
        loop {
            let _ = self.val_drop_data.in_val.next_event();
            let _ = self.val_drop_data.in_crd.next_event();

            let val_deq = self.val_drop_data.in_val.dequeue(&self.time);
            let crd_deq = self.val_drop_data.in_crd.dequeue(&self.time);
            match (val_deq, crd_deq) {
                (Ok(val), Ok(crd)) => match (val.data, crd.data) {
                    (Token::Val(value), Token::Val(coord)) if value != ValType::default() => {
                        let val_chan_elem = ChannelElement::new(
                            self.time.tick() + 1,
                            Token::<ValType, StopType>::Val(value),
                        );
                        self.val_drop_data
                            .out_val
                            .enqueue(&self.time, val_chan_elem)
                            .unwrap();
                        let crd_chan_elem = ChannelElement::new(
                            self.time.tick() + 1,
                            Token::<CrdType, StopType>::Val(coord),
                        );
                        self.val_drop_data
                            .out_crd
                            .enqueue(&self.time, crd_chan_elem)
                            .unwrap();
                    }
                    (Token::Val(val), Token::Val(_)) if val == ValType::default() => (),
                    (tkn1 @ Token::Stop(_), tkn2 @ Token::Stop(_))
                    | (tkn1 @ Token::Done, tkn2 @ Token::Done) => {
                        if tkn1 != Token::Done && prev_stkn {
                            prev_stkn = false;
                            continue;
                        }
                        let val_chan_elem = ChannelElement::new(self.time.tick() + 1, tkn1.clone());
                        self.val_drop_data
                            .out_val
                            .enqueue(&self.time, val_chan_elem)
                            .unwrap();
                        let crd_chan_elem = ChannelElement::new(self.time.tick() + 1, tkn2.clone());
                        self.val_drop_data
                            .out_crd
                            .enqueue(&self.time, crd_chan_elem)
                            .unwrap();
                        if tkn1 == Token::Done {
                            return;
                        } else {
                            prev_stkn = true;
                        }
                    }
                    _ => {
                        panic!("Invalid case reached in val_dropper");
                    }
                },
                _ => {
                    panic!("dequeue error in val, crd match");
                }
            }
            self.time.incr_cycles(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use dam::simulation::*;
    use dam::utility_contexts::*;

    use super::ValDrop;
    use super::ValDropData;
    use crate::templates::primitive::Token;
    use crate::token_vec;

    #[test]
    fn val_drop_2d_test() {
        let in_val = || {
            token_vec![f32; u32; 0.0, 1.0, 2.0, "S0", 0.0, "S0", 2.0, 3.0, 4.0, "S1", "D"]
                .into_iter()
        };
        let in_crd =
            || token_vec![u32; u32; 0, 1, 2, "S0", 0, "S0", 2, 3, 4, "S1", "D"].into_iter();
        let out_val = || token_vec![f32; u32; 1.0, 2.0, "S0", 2.0, 3.0, 4.0, "S1", "D"].into_iter();
        let out_crd = || token_vec![u32; u32; 1, 2, "S0", 2, 3, 4, "S1", "D"].into_iter();
        val_drop_test(in_val, in_crd, out_val, out_crd);
    }

    fn val_drop_test<IRT1, IRT2, ORT1, ORT2>(
        in_val: fn() -> IRT1,
        in_crd: fn() -> IRT2,
        out_val: fn() -> ORT1,
        out_crd: fn() -> ORT2,
    ) where
        IRT1: Iterator<Item = Token<f32, u32>> + 'static,
        IRT2: Iterator<Item = Token<u32, u32>> + 'static,
        ORT1: Iterator<Item = Token<f32, u32>> + 'static,
        ORT2: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (in_val_sender, in_val_receiver) = parent.unbounded::<Token<f32, u32>>();
        let (in_crd_sender, in_crd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (out_val_sender, out_val_receiver) = parent.unbounded::<Token<f32, u32>>();
        let (out_crd_sender, out_crd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let data = ValDropData::<u32, f32, u32> {
            in_val: in_val_receiver,
            in_crd: in_crd_receiver,
            out_val: out_val_sender,
            out_crd: out_crd_sender,
        };
        let val_drop = ValDrop::new(data);
        let gen1 = GeneratorContext::new(in_val, in_val_sender);
        let gen2 = GeneratorContext::new(in_crd, in_crd_sender);
        let out_val_checker = CheckerContext::new(out_val, out_val_receiver);
        let out_crd_checker = CheckerContext::new(out_crd, out_crd_receiver);
        parent.add_child(gen1);
        parent.add_child(gen2);
        parent.add_child(out_val_checker);
        parent.add_child(out_crd_checker);
        parent.add_child(val_drop);
        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());
        dbg!(executed.elapsed_cycles());
    }
}
