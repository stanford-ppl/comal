use crate::config::rd_scanner::CompressedCrdRdScanConfig;
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

    timing_config: CompressedCrdRdScanConfig,
}

#[context_macro]
pub struct TileRdScan<ValType: Clone, StopType: Clone> {
    rd_scan_data: RdScanData<ValType, StopType>,
    seg_arrs: Vec<Vec<ValType>>,
    crd_arrs: Vec<Vec<ValType>>,
    num_tiles: usize,
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
            timing_config: Default::default(),
            context_info: Default::default(),
        };
        (ucr.rd_scan_data.in_ref).attach_receiver(&ucr);
        (ucr.rd_scan_data.out_ref).attach_sender(&ucr);
        (ucr.rd_scan_data.out_crd).attach_sender(&ucr);

        ucr
    }

    pub fn set_timings(&mut self, new_config: CompressedCrdRdScanConfig) {
        self.timing_config = new_config
    }
}

impl<ValType: DAMType, StopType: DAMType> TileRdScan<ValType, StopType>
where
    TileRdScan<ValType, StopType>: Context,
{
    pub fn new(
        rd_scan_data: RdScanData<ValType, StopType>,
        seg_arrs: Vec<Vec<ValType>>,
        crd_arrs: Vec<Vec<ValType>>,
        num_tiles: usize,
    ) -> Self {
        let ucr = TileRdScan {
            rd_scan_data,
            seg_arrs,
            crd_arrs,
            num_tiles,
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
                            Token::Val(_) | Token::Done | Token::Empty => {
                                Token::Stop(StopType::default())
                            }
                            Token::Stop(stop_tkn) => {
                                self.rd_scan_data.in_ref.dequeue(&self.time).unwrap();
                                Token::Stop(stop_tkn + 1)
                            } // Token::Empty => {

                              // }
                        };

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

impl<ValType, StopType> Context for TileRdScan<ValType, StopType>
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
        let latency = 1;
        let initiation_interval = 1;
        dbg!(latency);
        dbg!(initiation_interval);
        let mut tile: usize = 0;
        loop {
            match self.rd_scan_data.in_ref.dequeue(&self.time) {
                Ok(curr_ref) => match curr_ref.data {
                    Token::Val(val) => {
                        let idx: usize = val.try_into().unwrap();
                        let mut curr_addr = self.seg_arrs[tile][idx].clone();
                        let stop_addr = self.seg_arrs[tile][idx + 1].clone();
                        while curr_addr < stop_addr {
                            let read_addr: usize = curr_addr.clone().try_into().unwrap();
                            let coord = self.crd_arrs[tile][read_addr].clone();
                            let curr_time = self.time.tick();

                            self.rd_scan_data
                                .out_crd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + latency,
                                        super::primitive::Token::Val(coord),
                                    ),
                                )
                                .unwrap();
                            self.rd_scan_data
                                .out_ref
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + latency,
                                        super::primitive::Token::Val(curr_addr.clone()),
                                    ),
                                )
                                .unwrap();
                            curr_addr += 1;
                            self.time.incr_cycles(initiation_interval);
                        }
                        let next_tkn = self.rd_scan_data.in_ref.peek_next(&self.time).unwrap();
                        let output: Token<ValType, StopType> = match next_tkn.data {
                            Token::Val(_) | Token::Done | Token::Empty => {
                                Token::Stop(StopType::default())
                            }
                            Token::Stop(stop_tkn) => {
                                self.rd_scan_data.in_ref.dequeue(&self.time).unwrap();
                                Token::Stop(stop_tkn + 1)
                            } // Token::Empty => {

                              // }
                        };

                        let curr_time = self.time.tick();
                        self.rd_scan_data
                            .out_crd
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + latency, output.clone()),
                            )
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(
                                &self.time,
                                ChannelElement::new(curr_time + latency, output.clone()),
                            )
                            .unwrap();
                    }
                    Token::Stop(token) => {
                        let curr_time = self.time.tick();
                        self.rd_scan_data
                            .out_crd
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    curr_time + latency,
                                    Token::Stop(token.clone() + 1),
                                ),
                            )
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    curr_time + latency,
                                    Token::Stop(token.clone() + 1),
                                ),
                            )
                            .unwrap();
                    }
                    // Could either be a done token or an empty token
                    // In the case of done token, return
                    Token::Done => {
                        let channel_elem =
                            ChannelElement::new(self.time.tick() + latency, Token::Done);
                        self.rd_scan_data
                            .out_crd
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();

                        tile += 1;
                        if tile == self.num_tiles {
                            return;
                        }
                    }
                    Token::Empty => {
                        let channel_elem =
                            ChannelElement::new(self.time.tick() + latency, Token::Empty);
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
            self.time.incr_cycles(initiation_interval);
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
        let factor = self.timing_config.data_load_factor;
        let seg_arr_len = self.seg_arr.len() as f64 * factor;
        let crd_arr_len = self.crd_arr.len() as f64 * factor;
        self.time.incr_cycles(seg_arr_len.ceil() as u64);
        self.time.incr_cycles(crd_arr_len.ceil() as u64);
        self.time.incr_cycles(self.timing_config.startup_delay);
        let mut seg_initiated = false;
        loop {
            match self.rd_scan_data.in_ref.dequeue(&self.time) {
                Ok(curr_ref) => match curr_ref.data {
                    Token::Val(val) => {
                        let idx: usize = val.try_into().unwrap();
                        let mut curr_addr = self.seg_arr[idx].clone();
                        let stop_addr = self.seg_arr[idx + 1].clone();
                        self.time.incr_cycles(self.timing_config.initial_delay);
                        let mut initiated = true;

                        let mut start_seg = 0;
                        if seg_initiated {
                            start_seg = idx;
                            seg_initiated = false;
                        }

                        if idx - start_seg >= self.timing_config.row_size {
                            seg_initiated = true;

                            self.time.incr_cycles(self.timing_config.miss_latency);
                        }

                        while curr_addr < stop_addr {
                            let mut start_rd_addr = 0;
                            let read_addr: usize = curr_addr.clone().try_into().unwrap();
                            let coord = self.crd_arr[read_addr].clone();
                            let curr_time = self.time.tick();
                            if initiated {
                                start_rd_addr = read_addr;
                                initiated = false;
                            }
                            let mut final_rd_latency = self.timing_config.output_latency;
                            if read_addr - start_rd_addr >= self.timing_config.row_size {
                                initiated = true;

                                final_rd_latency = self.timing_config.miss_latency;
                            }
                            self.rd_scan_data
                                .out_crd
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + final_rd_latency,
                                        super::primitive::Token::Val(coord),
                                    ),
                                )
                                .unwrap();
                            self.rd_scan_data
                                .out_ref
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + final_rd_latency,
                                        super::primitive::Token::Val(curr_addr.clone()),
                                    ),
                                )
                                .unwrap();
                            curr_addr += 1;
                            self.time
                                .incr_cycles(self.timing_config.sequential_interval);
                        }
                        let next_tkn = self.rd_scan_data.in_ref.peek_next(&self.time).unwrap();
                        let output: Token<ValType, StopType> = match next_tkn.data {
                            Token::Val(_) | Token::Done | Token::Empty => {
                                Token::Stop(StopType::default())
                            }
                            Token::Stop(stop_tkn) => {
                                self.rd_scan_data.in_ref.dequeue(&self.time).unwrap();
                                Token::Stop(stop_tkn + 1)
                            } // Token::Empty => {
                              //     panic!("Invalid empty inside peek");
                              // }
                        };
                        // dbg!(output);
                        let curr_time = self.time.tick();
                        self.rd_scan_data
                            .out_crd
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    curr_time + self.timing_config.output_latency,
                                    output.clone(),
                                ),
                            )
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    curr_time + self.timing_config.output_latency,
                                    output.clone(),
                                ),
                            )
                            .unwrap();
                    }
                    Token::Stop(token) => {
                        let curr_time = self.time.tick();
                        self.rd_scan_data
                            .out_crd
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    curr_time + self.timing_config.output_latency,
                                    Token::Stop(token.clone() + 1),
                                ),
                            )
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(
                                &self.time,
                                ChannelElement::new(
                                    curr_time + self.timing_config.output_latency,
                                    Token::Stop(token.clone() + 1),
                                ),
                            )
                            .unwrap();
                    }
                    // Could either be a done token or an empty token
                    // In the case of done token, return
                    Token::Done => {
                        let channel_elem = ChannelElement::new(
                            self.time.tick() + self.timing_config.output_latency,
                            Token::Done,
                        );
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
                        let channel_elem = ChannelElement::new(
                            self.time.tick() + self.timing_config.output_latency,
                            Token::Empty,
                        );
                        self.rd_scan_data
                            .out_crd
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        self.rd_scan_data
                            .out_ref
                            .enqueue(&self.time, channel_elem.clone())
                            .unwrap();
                        let next_tkn = self.rd_scan_data.in_ref.peek_next(&self.time).unwrap();
                        let output: Token<ValType, StopType> = match next_tkn.data {
                            Token::Val(_) | Token::Done | Token::Empty => {
                                Token::Stop(StopType::default())
                            }
                            Token::Stop(stop_tkn) => {
                                self.rd_scan_data.in_ref.dequeue(&self.time).unwrap();
                                Token::Stop(stop_tkn + 1)
                            }
                        };
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
                },
                Err(_) => panic!("Error: rd_scan_data dequeue error"),
            }
            self.time
                .incr_cycles(self.timing_config.sequential_interval);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use dam::simulation::InitializationOptionsBuilder;
    use dam::simulation::ProgramBuilder;
    use dam::simulation::RunMode;
    use dam::simulation::RunOptionsBuilder;
    use dam::utility_contexts::CheckerContext;
    use dam::utility_contexts::ConsumerContext;
    use dam::utility_contexts::GeneratorContext;

    use crate::templates::primitive::Token;
    use crate::token_vec;

    use super::CompressedCrdRdScan;
    use super::RdScanData;

    #[test]
    fn crd_2d_maybe_token() {
        let seg_arr = vec![0u32, 3, 6];
        let crd_arr = vec![0, 2, 3, 4, 5, 6];
        let in_ref = || token_vec!(u32; u32; 0, "N", "S0", "D").into_iter();

        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 27);
    }

    #[test]
    fn crd_2d_test1() {
        let seg_arr = vec![0, 10, 20, 30, 40, 50, 60, 70];
        let crd_arr = vec![
            5, 6, 7, 10, 12, 21, 23, 25, 27, 32, 0, 1, 4, 5, 8, 10, 13, 24, 27, 33, 0, 4, 5, 8, 11,
            12, 17, 19, 24, 33, 2, 6, 10, 15, 22, 23, 25, 26, 30, 33, 3, 5, 8, 12, 19, 23, 24, 26,
            27, 30, 0, 1, 2, 6, 17, 22, 23, 24, 25, 33, 0, 2, 5, 7, 12, 13, 20, 25, 27, 30,
        ];
        let in_ref = || token_vec!(u32; u32; "N", 1, 2, 1, "S0", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 54);
    }

    #[test]
    fn crd_2d_test2() {
        let seg_arr = vec![0, 16, 32, 48, 64, 80, 96];
        let crd_arr = vec![
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
            11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3,
            4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13,
            14, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
        ];
        let in_ref =
            || token_vec!(u32; u32; "S0", 2, "N", "N", "S0", "S0", 5, 1, 2, "S1", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 93);
    }

    #[test]
    fn crd_2d_test3() {
        // rd_2d_0.5_100_2d_0.3_30
        let seg_arr = vec![0, 55, 110];
        let crd_arr = vec![
            0, 1, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 26,
            27, 28, 29, 30, 31, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 44, 45, 46, 47, 48, 50, 51,
            52, 53, 54, 55, 56, 57, 59, 60, 61, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 13, 14, 15, 16, 17,
            18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 30, 31, 32, 33, 35, 36, 37, 38, 39, 40, 41,
            42, 43, 44, 45, 46, 48, 49, 50, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61,
        ];
        let in_ref = || {
            token_vec!(u32; u32; 0, 1, 1, 0, "N", 1, 0, "N", "S0", 1, 0, "N", 0, 1, 0, 1, "N", "S1", "D").into_iter()
        };
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 698);
    }

    #[test]
    fn crd_2d_test4() {
        let seg_arr = vec![0, 13, 26, 39, 52, 65, 78];
        let crd_arr = vec![
            20, 21, 22, 23, 24, 25, 26, 27, 29, 210, 211, 212, 214, 20, 22, 23, 24, 25, 26, 27, 28,
            29, 210, 212, 213, 214, 0, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 0, 1, 2, 3, 5, 7, 8,
            9, 10, 11, 12, 13, 14, 0, 1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 0, 1, 2, 3, 4, 6, 7,
            8, 9, 10, 12, 13, 14,
        ];
        // empty_root_seq_d
        let in_ref = || token_vec!(u32; u32; 0, "S1", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 36);
    }

    #[test]
    fn crd_2d_test5() {
        let seg_arr = vec![0, 3, 6];
        let crd_arr = vec![0, 2, 3, 4, 5, 6];
        let in_ref = || token_vec!(u32; u32; 0, "S0", 1, "S1", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 30);
    }

    #[test]
    fn crd_2d_test6() {
        let seg_arr = vec![0, 13, 26, 39, 52, 65, 78];
        let crd_arr = vec![
            0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 14, 0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 13, 14,
            0, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 0, 1, 2, 3, 5, 7, 8, 9, 10, 11, 12, 13, 14,
            0, 1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 0, 1, 2, 3, 4, 6, 7, 8, 9, 10, 12, 13, 14,
        ];
        let in_ref = || token_vec!(u32; u32; "S0", 5, 5, 0, "S0", 3, 1, "S1", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 93);
    }

    #[test]
    fn crd_2d_test7() {
        let seg_arr = vec![0, 199];
        let crd_arr = vec![
            2, 3, 4, 24, 29, 31, 34, 37, 49, 57, 60, 61, 67, 68, 70, 72, 86, 91, 100, 101, 102,
            110, 111, 113, 115, 116, 119, 123, 124, 127, 131, 133, 146, 150, 152, 155, 159, 160,
            163, 164, 165, 167, 168, 170, 171, 173, 174, 175, 176, 177, 180, 191, 193, 195, 200,
            202, 206, 208, 210, 217, 218, 219, 221, 224, 225, 230, 231, 234, 239, 240, 246, 248,
            249, 253, 254, 257, 260, 266, 268, 276, 277, 278, 279, 280, 292, 297, 311, 314, 320,
            321, 322, 326, 329, 330, 331, 332, 336, 338, 339, 340, 342, 343, 344, 345, 346, 348,
            351, 358, 363, 365, 376, 377, 380, 381, 396, 399, 403, 408, 411, 414, 415, 416, 417,
            423, 424, 428, 429, 433, 442, 444, 452, 454, 455, 459, 460, 461, 462, 465, 469, 470,
            473, 475, 477, 478, 479, 484, 486, 489, 490, 491, 493, 495, 500, 501, 503, 513, 514,
            517, 518, 525, 528, 532, 535, 542, 545, 548, 550, 557, 560, 563, 564, 565, 569, 570,
            574, 576, 578, 580, 583, 585, 589, 592, 595, 597, 600, 601, 613, 614, 615, 617, 619,
            620, 624, 625, 627, 628, 632, 650, 663,
        ];
        let in_ref = || token_vec!(u32; u32; "N", 0, 0, "S0", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 422);
    }

    #[test]
    fn crd_2d_rd_1d_0_5_200_1d_1_0_3() {
        let seg_arr = vec![0, 200];
        let crd_arr = vec![
            1, 3, 4, 6, 11, 14, 17, 19, 21, 22, 24, 27, 28, 30, 31, 33, 36, 37, 42, 45, 46, 51, 52,
            53, 55, 57, 61, 62, 67, 68, 69, 70, 71, 73, 74, 78, 79, 80, 85, 86, 91, 93, 95, 96, 97,
            100, 101, 103, 106, 108, 109, 110, 112, 113, 116, 122, 124, 127, 131, 133, 139, 140,
            144, 145, 148, 151, 152, 154, 157, 159, 161, 162, 164, 165, 168, 169, 170, 173, 174,
            176, 178, 180, 183, 184, 185, 186, 187, 188, 191, 192, 194, 195, 198, 199, 201, 203,
            206, 211, 213, 214, 215, 218, 221, 222, 223, 224, 225, 227, 231, 235, 243, 245, 246,
            247, 248, 253, 255, 256, 257, 258, 259, 260, 262, 263, 264, 265, 266, 267, 268, 269,
            271, 272, 274, 279, 283, 284, 286, 291, 293, 299, 300, 302, 304, 305, 306, 307, 308,
            309, 310, 311, 313, 314, 315, 316, 317, 318, 319, 320, 321, 322, 323, 324, 325, 326,
            329, 330, 331, 332, 335, 336, 337, 339, 341, 343, 345, 346, 347, 348, 351, 352, 354,
            356, 359, 364, 365, 367, 368, 369, 376, 377, 378, 379, 382, 383, 385, 387, 388, 390,
            392, 393,
        ];
        let in_ref = || token_vec!(u32; u32; "N", "N", 0, "S0", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 223);
    }

    #[test]
    fn crd_2d_rd_1d_1_0_200_1d_1_0_3() {
        let seg_arr = vec![0, 200];
        let crd_arr = vec![
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
            46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67,
            68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89,
            90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108,
            109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125,
            126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142,
            143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159,
            160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175, 176,
            177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193,
            194, 195, 196, 197, 198, 199,
        ];
        let in_ref = || token_vec!(u32; u32; "N", 0, "N", "S0", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 224);
    }

    fn compressed_rd_scan_calibration<IRT>(
        seg_arr: Vec<u32>,
        crd_arr: Vec<u32>,
        in_ref: fn() -> IRT,
        actual_cycle: i64,
    ) where
        IRT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (ref_sender, ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (crd_sender, crd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let data = RdScanData::<u32, u32> {
            in_ref: in_ref_receiver,
            out_ref: ref_sender,
            out_crd: crd_sender,
        };
        let cr = CompressedCrdRdScan::new(data, seg_arr, crd_arr);
        let gen1 = GeneratorContext::new(in_ref, in_ref_sender);
        let crd_checker = ConsumerContext::new(crd_receiver);
        let ref_checker = ConsumerContext::new(ref_receiver);

        parent.add_child(gen1);
        parent.add_child(crd_checker);
        parent.add_child(ref_checker);
        parent.add_child(cr);

        let initialized = parent
            .initialize(
                InitializationOptionsBuilder::default()
                    .run_flavor_inference(true)
                    .build()
                    .unwrap(),
            )
            .unwrap();

        let executed = initialized.run(
            RunOptionsBuilder::default()
                .mode(RunMode::Simple)
                .build()
                .unwrap(),
        );

        let diff: i64 =
            TryInto::<i64>::try_into(executed.elapsed_cycles().unwrap()).unwrap() - actual_cycle;
        println!("Elapsed: {:?}", executed.elapsed_cycles().unwrap());
        println!("Diff: {:?}", diff);
    }

    fn compressed_rd_scan_test<IRT, ORT, CRT>(
        seg_arr: Vec<u32>,
        crd_arr: Vec<u32>,
        in_ref: fn() -> IRT,
        out_ref: fn() -> ORT,
        out_crd: fn() -> CRT,
    ) where
        IRT: Iterator<Item = Token<u32, u32>> + 'static,
        CRT: Iterator<Item = Token<u32, u32>> + 'static,
        ORT: Iterator<Item = Token<u32, u32>> + 'static,
    {
        let mut parent = ProgramBuilder::default();
        let (ref_sender, ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (crd_sender, crd_receiver) = parent.unbounded::<Token<u32, u32>>();
        let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
        let data = RdScanData::<u32, u32> {
            in_ref: in_ref_receiver,
            out_ref: ref_sender,
            out_crd: crd_sender,
        };
        let cr = CompressedCrdRdScan::new(data, seg_arr, crd_arr);
        let gen1 = GeneratorContext::new(in_ref, in_ref_sender);
        let crd_checker = CheckerContext::new(out_crd, crd_receiver);
        let ref_checker = CheckerContext::new(out_ref, ref_receiver);

        parent.add_child(gen1);
        parent.add_child(crd_checker);
        parent.add_child(ref_checker);
        parent.add_child(cr);

        let init_start = Instant::now();
        let initialized = parent
            .initialize(
                InitializationOptionsBuilder::default()
                    .run_flavor_inference(true)
                    .build()
                    .unwrap(),
            )
            .unwrap();
        let init_end = Instant::now();
        println!("Init took: {:.2?}", init_end - init_start);

        let executed = initialized.run(
            RunOptionsBuilder::default()
                .mode(RunMode::Simple)
                .build()
                .unwrap(),
        );
        println!("Elapsed cycles: {:?}", executed.elapsed_cycles().unwrap());
    }
}
