use dam::{context_tools::*, dam_macros::context_macro};

use super::primitive::Token;

pub struct RdScanData<ValType: Clone, StopType: Clone> {
    pub in_ref: Receiver<Token<ValType, StopType>>,
    pub out_ref: Sender<Token<ValType, StopType>>,
    pub out_crd: Sender<Token<ValType, StopType>>,
}

#[context_macro]
pub struct UncompressedCrdRdScan<ValType: Clone, StopType: Clone> {
    rd_scan_data: RdScanData<ValType, StopType>,
    meta_dim: ValType,
}

#[context_macro]
pub struct CompressedCrdRdScan<ValType: Clone, StopType: Clone> {
    rd_scan_data: RdScanData<ValType, StopType>,
    seg_arr: Vec<ValType>,
    crd_arr: Vec<ValType>,
}

impl<ValType: DAMType, StopType: DAMType> UncompressedCrdRdScan<ValType, StopType>
where
    UncompressedCrdRdScan<ValType, StopType>: Context,
{
    pub fn new(
        rd_scan_data: RdScanData<ValType, StopType>,
        meta_dim: ValType,
    ) -> UncompressedCrdRdScan<ValType, StopType> {
        let ucr = UncompressedCrdRdScan {
            rd_scan_data,
            meta_dim,
            context_info: Default::default(),
        };
        (ucr.rd_scan_data.in_ref).attach_receiver(&ucr);
        (ucr.rd_scan_data.out_ref).attach_sender(&ucr);
        (ucr.rd_scan_data.out_crd).attach_sender(&ucr);

        ucr
    }
}

impl<ValType: DAMType, StopType: DAMType> CompressedCrdRdScan<ValType, StopType>
where
    CompressedCrdRdScan<ValType, StopType>: Context,
{
    pub fn new(
        rd_scan_data: RdScanData<ValType, StopType>,
        seg_arr: Vec<ValType>,
        crd_arr: Vec<ValType>,
    ) -> Self {
        let ucr = CompressedCrdRdScan {
            rd_scan_data,
            seg_arr,
            crd_arr,
            context_info: Default::default(),
        };
        (ucr.rd_scan_data.in_ref).attach_receiver(&ucr);
        (ucr.rd_scan_data.out_ref).attach_sender(&ucr);
        (ucr.rd_scan_data.out_crd).attach_sender(&ucr);

        ucr
    }
}

impl<ValType, StopType> Context for UncompressedCrdRdScan<ValType, StopType>
where
    ValType: DAMType
        + std::ops::AddAssign<u32>
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        // let mut curr_crd: Token<ValType, StopType>
        loop {
            match self.rd_scan_data.in_ref.dequeue(&self.time) {
                Ok(curr_ref) => match curr_ref.data {
                    Token::Val(val) => {
                        let mut crd_count: ValType = ValType::default();
                        while crd_count < self.meta_dim {
                            let curr_time = self.time.tick();
                            self.rd_scan_data
                                .out_crd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + 1,
                                        super::primitive::Token::Val(crd_count.clone()),
                                    ),
                                )
                                .unwrap();
                            self.rd_scan_data
                                .out_ref
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + 1,
                                        super::primitive::Token::Val(
                                            crd_count.clone() + val.clone() * self.meta_dim.clone(),
                                        ),
                                    ),
                                )
                                .unwrap();
                            crd_count += 1;
                            self.time.incr_cycles(1);
                        }
                        let next_tkn = self.rd_scan_data.in_ref.peek_next(&self.time).unwrap();
                        let output: Token<ValType, StopType> = match next_tkn.data {
                            Token::Val(_) | Token::Done => Token::Stop(StopType::default()),
                            Token::Stop(stop_tkn) => {
                                self.rd_scan_data.in_ref.dequeue(&self.time).unwrap();
                                Token::Stop(stop_tkn + 1)
                            }
                            Token::Empty => {
                                panic!("Invalid empty inside peek");
                            }
                        };
                        // dbg!(output);
                        let curr_time = self.time.tick();
                        self.rd_scan_data
                            .out_crd
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + 1, output.clone()),
                            )
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + 1, output.clone()),
                            )
                            .unwrap();
                    }
                    Token::Stop(token) => {
                        let curr_time = self.time.tick();
                        self.rd_scan_data
                            .out_crd
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + 1, Token::Stop(token.clone() + 1)),
                            )
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + 1, Token::Stop(token.clone() + 1)),
                            )
                            .unwrap();
                    }
                    // Could either be a done token or an empty token
                    // In the case of done token, return
                    Token::Done => {
                        let channel_elem = ChannelElement::new(self.time.tick() + 1, Token::Done);
                        self.rd_scan_data
                            .out_crd
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        return;
                    }
                    Token::Empty => {
                        let channel_elem = ChannelElement::new(self.time.tick() + 1, Token::Empty);
                        self.rd_scan_data
                            .out_crd
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                    }
                },
                Err(_) => panic!("Error: rd_scan_data dequeue error"),
            }
            self.time.incr_cycles(1);
        }
    }
}

impl<ValType, StopType> Context for CompressedCrdRdScan<ValType, StopType>
where
    ValType: DAMType
        + std::ops::AddAssign<u32>
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    // usize: From<ValType>,
    ValType: TryInto<usize>,
    <ValType as TryInto<usize>>::Error: std::fmt::Debug,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        // let mut curr_crd: Token<ValType, StopType>
        loop {
            match self.rd_scan_data.in_ref.dequeue(&self.time) {
                Ok(curr_ref) => match curr_ref.data {
                    Token::Val(val) => {
                        let idx: usize = val.try_into().unwrap();
                        let mut curr_addr = self.seg_arr[idx].clone();
                        let stop_addr = self.seg_arr[idx + 1].clone();
                        while curr_addr < stop_addr {
                            let read_addr: usize = curr_addr.clone().try_into().unwrap();
                            let coord = self.crd_arr[read_addr].clone();
                            let curr_time = self.time.tick();
                            // dbg!(coord.clone());
                            self.rd_scan_data
                                .out_crd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + 1,
                                        super::primitive::Token::Val(coord),
                                    ),
                                )
                                .unwrap();
                            self.rd_scan_data
                                .out_ref
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + 1,
                                        super::primitive::Token::Val(curr_addr.clone()),
                                    ),
                                )
                                .unwrap();
                            curr_addr += 1;
                            self.time.incr_cycles(1);
                        }
                        let next_tkn = self.rd_scan_data.in_ref.peek_next(&self.time).unwrap();
                        let output: Token<ValType, StopType> = match next_tkn.data {
                            Token::Val(_) | Token::Done => Token::Stop(StopType::default()),
                            Token::Stop(stop_tkn) => {
                                self.rd_scan_data.in_ref.dequeue(&self.time).unwrap();
                                Token::Stop(stop_tkn + 1)
                            }
                            Token::Empty => {
                                panic!("Invalid empty inside peek");
                            }
                        };
                        // dbg!(output);
                        let curr_time = self.time.tick();
                        self.rd_scan_data
                            .out_crd
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + 1, output.clone()),
                            )
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + 1, output.clone()),
                            )
                            .unwrap();
                    }
                    Token::Stop(token) => {
                        let curr_time = self.time.tick();
                        self.rd_scan_data
                            .out_crd
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + 1, Token::Stop(token.clone() + 1)),
                            )
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + 1, Token::Stop(token.clone() + 1)),
                            )
                            .unwrap();
                    }
                    // Could either be a done token or an empty token
                    // In the case of done token, return
                    Token::Done => {
                        let channel_elem = ChannelElement::new(self.time.tick() + 1, Token::Done);
                        self.rd_scan_data
                            .out_crd
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        // dbg!(Token::<ValType, StopType>::Done);
                        return;
                    }
                    Token::Empty => {
                        let channel_elem = ChannelElement::new(self.time.tick() + 1, Token::Empty);
                        self.rd_scan_data
                            .out_crd
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                    }
                },
                Err(_) => panic!("Error: rd_scan_data dequeue error"),
            }
            self.time.incr_cycles(1);
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::{
//         context::{checker_context::CheckerContext, generator_context::GeneratorContext},
//         simulation::Program,
//         templates::sam::primitive::Token,
//     };

//     use super::CompressedCrdRdScan;
//     use super::RdScanData;
//     use super::UncompressedCrdRdScan;

//     #[test]
//     fn ucrd_1d_test() {
//         let in_ref = || [Token::Val(0u32), Token::Done].into_iter();
//         let out_ref = || {
//             (0u32..32)
//                 .map(Token::Val)
//                 .chain([Token::Stop(0), Token::Done])
//         };
//         uncompressed_rd_scan_test(in_ref, out_ref, out_ref);
//     }

//     #[test]
//     fn ucrd_2d_test() {
//         let in_ref = || {
//             (0u32..4)
//                 .map(Token::Val)
//                 .chain([Token::Stop(0), Token::Done])
//         };
//         let out_ref = || {
//             (0u32..32)
//                 .map(Token::Val)
//                 .chain([Token::Stop(0)])
//                 .chain((32u32..64).map(Token::Val))
//                 .chain([Token::Stop(0)])
//                 .chain((64u32..96).map(Token::Val))
//                 .chain([Token::Stop(0)])
//                 .chain((96u32..128).map(Token::Val))
//                 .chain([Token::Stop(1), Token::Done])
//         };
//         let out_crd = || {
//             (0u32..32)
//                 .map(Token::Val)
//                 .chain([Token::Stop(0)])
//                 .cycle()
//                 // Repeat 3 fibers with stops and another fiber without the first level stop token since it gets replaced with second level stop
//                 .take(33 * 4 - 1)
//                 .chain([Token::Stop(1), Token::Done])
//         };
//         uncompressed_rd_scan_test(in_ref, out_ref, out_crd);
//     }

//     // #[test]
//     fn uncompressed_rd_scan_test<IRT, ORT, CRT>(
//         in_ref: fn() -> IRT,
//         out_ref: fn() -> ORT,
//         out_crd: fn() -> CRT,
//     ) where
//         IRT: Iterator<Item = Token<u32, u32>> + 'static,
//         CRT: Iterator<Item = Token<u32, u32>> + 'static,
//         ORT: Iterator<Item = Token<u32, u32>> + 'static,
//     {
//         let mut parent = ProgramBuilder::default();
//         let meta_dim: u32 = 32;
//         let (ref_sender, ref_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let (crd_sender, crd_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let data = RdScanData::<u32, u32> {
//             in_ref: in_ref_receiver,
//             out_ref: ref_sender,
//             out_crd: crd_sender,
//         };
//         let ucr = UncompressedCrdRdScan::new(data, meta_dim);
//         let gen1 = GeneratorContext::new(in_ref, in_ref_sender);
//         let crd_checker = CheckerContext::new(out_crd, crd_receiver);
//         let ref_checker = CheckerContext::new(out_ref, ref_receiver);

//         parent.add_child(gen1);
//         parent.add_child(crd_checker);
//         parent.add_child(ref_checker);
//         parent.add_child(ucr);
//         parent.init();
//         parent.run();
//     }

//     #[test]
//     fn crd_1d_test() {
//         let seg_arr = vec![0u32, 3];
//         let crd_arr = vec![0u32, 1, 3];
//         let in_ref = || [Token::Val(0u32), Token::Done].into_iter();
//         let out_ref = || {
//             (0u32..3)
//                 .map(Token::Val)
//                 .chain([Token::Stop(0), Token::Done])
//         };
//         let out_crd = || {
//             vec![0u32, 1, 3]
//                 .into_iter()
//                 .map(Token::Val)
//                 .chain([Token::Stop(0u32), Token::Done])
//         };
//         compressed_rd_scan_test(seg_arr, crd_arr, in_ref, out_ref, out_crd);
//     }

//     #[test]
//     fn crd_2d_test() {
//         let seg_arr = vec![0u32, 3, 4, 6];
//         let crd_arr = vec![0u32, 2, 3, 0, 2, 3];
//         let in_ref = || {
//             [
//                 Token::Val(0u32),
//                 Token::Val(0),
//                 Token::Stop(0),
//                 Token::Val(1),
//                 Token::Stop(0),
//                 Token::Val(2),
//                 Token::Stop(1),
//                 Token::Done,
//             ]
//             .into_iter()
//         };
//         let out_ref = || {
//             [0u32, 1, 2]
//                 .into_iter()
//                 .map(Token::Val)
//                 .chain([Token::Stop(0)])
//                 .chain([0u32, 1, 2].into_iter().map(Token::Val))
//                 .chain(
//                     [
//                         Token::Stop(1),
//                         Token::Val(3),
//                         Token::Stop(1),
//                         Token::Val(4),
//                         Token::Val(5),
//                         Token::Stop(2),
//                         Token::Done,
//                     ]
//                     .into_iter(),
//                 )
//         };
//         let out_crd = || {
//             [0u32, 2, 3]
//                 .into_iter()
//                 .map(Token::Val)
//                 .chain([Token::Stop(0)])
//                 .chain([0u32, 2, 3].into_iter().map(Token::Val))
//                 .chain(
//                     [
//                         Token::Stop(1),
//                         Token::Val(0),
//                         Token::Stop(1),
//                         Token::Val(2),
//                         Token::Val(3),
//                         Token::Stop(2),
//                         Token::Done,
//                     ]
//                     .into_iter(),
//                 )
//         };
//         compressed_rd_scan_test(seg_arr, crd_arr, in_ref, out_ref, out_crd);
//     }

//     fn compressed_rd_scan_test<IRT, ORT, CRT>(
//         seg_arr: Vec<u32>,
//         crd_arr: Vec<u32>,
//         in_ref: fn() -> IRT,
//         out_ref: fn() -> ORT,
//         out_crd: fn() -> CRT,
//     ) where
//         IRT: Iterator<Item = Token<u32, u32>> + 'static,
//         CRT: Iterator<Item = Token<u32, u32>> + 'static,
//         ORT: Iterator<Item = Token<u32, u32>> + 'static,
//     {
//         let mut parent = ProgramBuilder::default();
//         let (ref_sender, ref_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let (crd_sender, crd_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
//         let data = RdScanData::<u32, u32> {
//             in_ref: in_ref_receiver,
//             out_ref: ref_sender,
//             out_crd: crd_sender,
//         };
//         let cr = CompressedCrdRdScan::new(data, seg_arr, crd_arr);
//         let gen1 = GeneratorContext::new(in_ref, in_ref_sender);
//         let crd_checker = CheckerContext::new(out_crd, crd_receiver);
//         let ref_checker = CheckerContext::new(out_ref, ref_receiver);

//         parent.add_child(gen1);
//         parent.add_child(crd_checker);
//         parent.add_child(ref_checker);
//         parent.add_child(cr);
//         parent.init();
//         parent.run();
//     }
// }
