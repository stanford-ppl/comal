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
                            // dbg!(coord.clone());
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
                              //     panic!("Invalid empty inside peek");
                              // }
                        };
                        // dbg!(output);
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
                        // dbg!(Token::<ValType, StopType>::Done);
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
        loop {
            match self.rd_scan_data.in_ref.dequeue(&self.time) {
                Ok(curr_ref) => match curr_ref.data {
                    Token::Val(val) => {
                        let idx: usize = val.try_into().unwrap();
                        let mut curr_addr = self.seg_arr[idx].clone();
                        let stop_addr = self.seg_arr[idx + 1].clone();
                        self.time.incr_cycles(self.timing_config.initial_delay);
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
                                        curr_time + self.timing_config.output_latency,
                                        super::primitive::Token::Val(coord),
                                    ),
                                )
                                .unwrap();
                            self.rd_scan_data
                                .out_ref
                                .enqueue(
                                    &self.time,
                                    ChannelElement::new(
                                        curr_time + self.timing_config.output_latency,
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
    fn crd_2d_test() {
        let seg_arr = vec![0u32, 200];
        let crd_arr = vec![
            3, 13, 45, 57, 68, 84, 117, 140, 142, 155, 156, 168, 171, 177, 193, 194, 213, 219, 226,
            229, 246, 248, 270, 278, 281, 282, 308, 314, 317, 325, 336, 341, 343, 345, 352, 356,
            376, 378, 382, 383, 390, 400, 426, 434, 440, 441, 478, 485, 518, 520, 521, 523, 545,
            546, 560, 561, 573, 590, 591, 606, 612, 617, 630, 654, 658, 725, 728, 733, 735, 737,
            741, 746, 766, 778, 788, 828, 865, 869, 874, 881, 885, 890, 908, 918, 920, 922, 929,
            932, 940, 977, 986, 1013, 1021, 1029, 1040, 1044, 1046, 1050, 1059, 1088, 1105, 1117,
            1129, 1180, 1184, 1188, 1192, 1193, 1199, 1212, 1217, 1218, 1222, 1229, 1233, 1238,
            1245, 1256, 1261, 1270, 1278, 1283, 1284, 1297, 1323, 1332, 1335, 1345, 1364, 1377,
            1382, 1389, 1402, 1408, 1411, 1415, 1436, 1438, 1440, 1441, 1459, 1477, 1491, 1504,
            1508, 1522, 1559, 1564, 1580, 1583, 1584, 1588, 1594, 1602, 1606, 1612, 1616, 1648,
            1663, 1664, 1671, 1683, 1688, 1700, 1716, 1718, 1719, 1723, 1724, 1739, 1751, 1752,
            1765, 1766, 1776, 1795, 1796, 1800, 1806, 1812, 1814, 1815, 1817, 1820, 1828, 1842,
            1843, 1849, 1851, 1854, 1879, 1914, 1929, 1949, 1951, 1975, 1978, 1979, 1982, 1986,
        ];
        let in_ref = || token_vec!(u32; u32; "N", 0, "D").into_iter();

        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 479);
    }

    #[test]
    fn crd_2d_test1() {
        let seg_arr = vec![0, 12, 24, 36, 48, 60, 72, 84, 96, 108, 120];
        let crd_arr = vec![
            1, 2, 5, 7, 8, 9, 10, 11, 12, 13, 14, 17, 0, 1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 1,
            2, 4, 5, 7, 9, 10, 11, 12, 13, 14, 17, 3, 4, 5, 6, 7, 9, 10, 11, 13, 15, 16, 17, 2, 4,
            6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 0, 1, 3, 4, 5, 7, 8, 11, 13, 15, 16, 17, 0, 1, 2,
            4, 6, 8, 9, 10, 11, 14, 15, 17, 0, 1, 2, 3, 4, 6, 8, 10, 13, 14, 15, 16, 0, 1, 2, 4, 7,
            8, 10, 11, 13, 14, 15, 16, 1, 2, 3, 4, 5, 6, 7, 10, 11, 14, 15, 16,
        ];
        let in_ref = || token_vec!(u32; u32; 4, 3, 9, "S0", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 363);
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
        let seg_arr = vec![0, 19, 38, 57, 76, 95, 114];
        let crd_arr = vec![
            0, 1, 4, 6, 7, 8, 9, 11, 13, 14, 15, 16, 18, 20, 21, 23, 25, 26, 27, 0, 2, 3, 4, 5, 7,
            10, 11, 12, 13, 14, 18, 19, 20, 21, 22, 23, 24, 26, 1, 2, 3, 5, 6, 7, 9, 10, 11, 12,
            14, 17, 18, 19, 20, 23, 24, 26, 27, 0, 2, 3, 4, 5, 6, 7, 9, 13, 15, 16, 17, 18, 19, 20,
            22, 24, 26, 27, 0, 4, 7, 8, 9, 10, 11, 12, 13, 14, 15, 19, 20, 21, 22, 23, 25, 26, 27,
            1, 2, 4, 5, 6, 8, 9, 10, 11, 13, 14, 15, 16, 18, 20, 22, 23, 24, 26,
        ];
        let in_ref = || token_vec!(u32; u32; 0, 1, 5, "S0", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 82);
    }

    #[test]
    fn crd_2d_test4() {
        let seg_arr = vec![0, 12, 24, 36, 48];
        let crd_arr = vec![
            0, 3, 5, 6, 8, 9, 10, 13, 14, 16, 19, 21, 3, 4, 5, 6, 8, 9, 12, 13, 14, 18, 19, 20, 0,
            3, 5, 6, 7, 9, 13, 14, 18, 19, 21, 22, 0, 4, 5, 6, 8, 9, 10, 12, 14, 16, 18, 19,
        ];
        let in_ref = || token_vec!(u32; u32; 1, 1, 1, "S0", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 61);
    }

    #[test]
    fn crd_2d_test5() {
        let seg_arr = vec![0, 3, 6];
        let crd_arr = vec![0, 2, 3, 4, 5, 6];
        let in_ref = || token_vec!(u32; u32; 0, "S0", 1, "S1", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 262);
    }

    #[test]
    fn crd_2d_test6() {
        let seg_arr = vec![0, 13, 26, 39, 52, 65];
        let crd_arr = vec![
            0, 2, 3, 4, 6, 7, 11, 16, 17, 18, 20, 21, 22, 1, 3, 4, 5, 7, 9, 11, 12, 15, 16, 19, 20,
            23, 1, 2, 3, 7, 11, 12, 13, 14, 18, 20, 21, 23, 24, 2, 4, 6, 7, 9, 12, 15, 16, 18, 19,
            20, 22, 24, 0, 1, 2, 3, 4, 5, 6, 7, 8, 14, 15, 22, 23,
        ];
        let in_ref =
            || token_vec!(u32; u32; "S0", 3, 1, "N", "S0", "S0", "N", 1, 2, "S1", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 301);
    }

    #[test]
    fn crd_2d_test7() {
        let seg_arr = vec![0, 199];
        let crd_arr = vec![
            3, 9, 12, 13, 14, 21, 24, 25, 27, 31, 33, 35, 43, 45, 48, 49, 53, 58, 59, 68, 70, 71,
            73, 76, 78, 79, 82, 83, 86, 92, 93, 99, 104, 105, 109, 111, 114, 119, 134, 139, 141,
            145, 146, 151, 153, 157, 158, 160, 171, 172, 174, 179, 181, 182, 184, 185, 187, 190,
            196, 199, 200, 202, 206, 208, 209, 210, 211, 213, 216, 224, 230, 232, 234, 235, 238,
            239, 242, 253, 256, 259, 260, 265, 266, 272, 277, 281, 290, 291, 297, 298, 299, 302,
            303, 306, 309, 310, 311, 313, 318, 319, 332, 339, 344, 354, 372, 380, 381, 383, 384,
            386, 396, 398, 399, 402, 408, 409, 412, 419, 420, 421, 422, 428, 441, 442, 448, 449,
            451, 452, 455, 457, 463, 469, 472, 473, 475, 477, 479, 480, 486, 488, 489, 493, 496,
            497, 499, 500, 501, 510, 514, 517, 522, 524, 528, 530, 531, 532, 535, 536, 537, 539,
            541, 544, 551, 554, 555, 557, 562, 563, 564, 572, 580, 585, 588, 592, 593, 596, 600,
            602, 603, 609, 614, 615, 617, 619, 622, 623, 624, 625, 626, 628, 633, 636, 644, 647,
            650, 651, 657, 659, 663,
        ];
        let in_ref = || token_vec!(u32; u32; 0, 0, 0, "S0", "D").into_iter();
        compressed_rd_scan_calibration(seg_arr, crd_arr, in_ref, 881);
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
        // let crd_checker = ConsumerContext::new(crd_receiver);
        // let ref_checker = ConsumerContext::new(ref_receiver);

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
