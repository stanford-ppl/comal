use dam::structures::Identifiable;
use dam::{
    context_tools::*,
    dam_macros::{context_macro, event_type},
    structures::Identifier,
};
use serde::{Deserialize, Serialize};

use super::primitive::Token;

#[context_macro]
pub struct IterateLocate<ValType: Clone, StopType: Clone> {
    pub in_ref: Receiver<Token<ValType, StopType>>,
    pub in_crd: Receiver<Token<ValType, StopType>>,
    pub out_ref1: Sender<Token<ValType, StopType>>,
    pub out_ref2: Sender<Token<ValType, StopType>>,
    pub out_crd: Sender<Token<ValType, StopType>>,
}

impl<ValType: DAMType, StopType: DAMType> IterateLocate<ValType, StopType>
where
    IterateLocate<ValType, StopType>: Context,
{
    pub fn new(
        in_ref: Receiver<Token<ValType, StopType>>,
        in_crd: Receiver<Token<ValType, StopType>>,
        out_ref1: Sender<Token<ValType, StopType>>,
        out_ref2: Sender<Token<ValType, StopType>>,
        out_crd: Sender<Token<ValType, StopType>>,
    ) -> Self {
        let loc = IterateLocate {
            in_ref,
            in_crd,
            out_ref1,
            out_ref2,
            out_crd,
            context_info: Default::default(),
        };
        (loc.in_ref).attach_receiver(&loc);
        (loc.in_crd).attach_receiver(&loc);
        (loc.out_ref1).attach_sender(&loc);
        (loc.out_ref2).attach_sender(&loc);
        (loc.out_crd).attach_sender(&loc);

        loc
    }
}

impl<ValType, StopType> Context for IterateLocate<ValType, StopType>
where
    ValType: DAMType + std::cmp::PartialEq,
    StopType: DAMType + std::ops::Add<u32, Output = StopType> + std::cmp::PartialEq,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let id = Identifier { id: 0 };
        let curr_id = self.id();
        loop {
            match (
                self.in_ref.dequeue(&self.time),
                self.in_crd.dequeue(&self.time),
            ) {
                (Ok(in_ref), Ok(in_crd)) => match (in_ref.data.clone(), in_crd.data.clone()) {
                    _ => {
                        self.out_ref1
                            .enqueue(
                                &self.time,
                                ChannelElement::new(self.time.tick() + 1, in_crd.data.clone()),
                            )
                            .unwrap();
                        self.out_ref2
                            .enqueue(
                                &self.time,
                                ChannelElement::new(self.time.tick() + 1, in_ref.data.clone()),
                            )
                            .unwrap();
                        self.out_crd
                            .enqueue(
                                &self.time,
                                ChannelElement::new(self.time.tick() + 1, in_crd.data.clone()),
                            )
                            .unwrap();
                        if in_ref.data.clone() == Token::Done {
                            return;
                        }
                    }
                },
                _ => panic!("Should not reach this case"),
            }
            self.time.incr_cycles(1);
        }
    }
}
