use std::fs;

use dam::{context_tools::*, dam_macros::context_macro};

use crate::config::Data;

use super::primitive::Token;

#[context_macro]
pub struct CompressedWrScan<ValType: Clone, StopType: Clone> {
    pub input: Receiver<Token<ValType, StopType>>,
    pub seg_arr: Vec<ValType>,
    pub crd_arr: Vec<ValType>,
}

impl<ValType: DAMType, StopType: DAMType> CompressedWrScan<ValType, StopType>
where
    CompressedWrScan<ValType, StopType>: Context,
{
    pub fn new(input: Receiver<Token<ValType, StopType>>) -> Self {
        let cwr = CompressedWrScan {
            input,
            seg_arr: vec![],
            crd_arr: vec![],
            context_info: Default::default(),
        };
        (cwr).input.attach_receiver(&cwr);

        cwr
    }
}

impl<ValType, StopType> Context for CompressedWrScan<ValType, StopType>
where
    ValType: DAMType
        + std::ops::AddAssign<u32>
        + std::ops::AddAssign<ValType>
        + std::ops::Mul<ValType, Output = ValType>
        + std::ops::Add<ValType, Output = ValType>
        + std::cmp::PartialOrd<ValType>,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {
        // default is 0
        self.seg_arr.push(ValType::default());
    }

    fn run(&mut self) {
        // let mut curr_crd: Token<ValType, StopType>
        let mut curr_crd_cnt: ValType = ValType::default();
        let mut end_fiber = false;
        let filename = home::home_dir().unwrap().join("sam_config.toml");
        let contents = fs::read_to_string(filename).unwrap();
        let data: Data = toml::from_str(&contents).unwrap();
        let _latency = data.sam_config.fiberwrite_latency;
        let initiation_interval = data.sam_config.fiberwrite_ii;
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        self.crd_arr.push(val);
                        curr_crd_cnt += 1;
                        end_fiber = false;
                    }
                    Token::Stop(_) if !end_fiber => {
                        self.seg_arr.push(curr_crd_cnt.clone());
                        end_fiber = true;
                    }
                    Token::Empty | Token::Stop(_) => {
                        // TODO: Maybe needs to be processed too
                        // panic!("Reached panic in wr scanner");
                        continue;
                    }
                    Token::Done => {
                        return;
                    }
                },
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            self.time.incr_cycles(initiation_interval);
        }
    }
}

#[context_macro]
pub struct ValsWrScan<ValType: Clone, StopType: Clone> {
    pub input: Receiver<Token<ValType, StopType>>,
    pub out_val: Vec<ValType>,
}

impl<ValType: DAMType, StopType: DAMType> ValsWrScan<ValType, StopType>
where
    ValsWrScan<ValType, StopType>: Context,
{
    pub fn new(input: Receiver<Token<ValType, StopType>>) -> Self {
        let vals = ValsWrScan {
            input,
            out_val: vec![],
            context_info: Default::default(),
        };
        (vals.input).attach_receiver(&vals);

        vals
    }
}

impl<ValType, StopType> Context for ValsWrScan<ValType, StopType>
where
    ValType: DAMType,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let filename = home::home_dir().unwrap().join("sam_config.toml");
        let contents = fs::read_to_string(filename).unwrap();
        let data: Data = toml::from_str(&contents).unwrap();
        let latency = data.sam_config.fiberwrite_latency;
        let initiation_interval = data.sam_config.fiberwrite_ii;
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        self.out_val.push(val);
                    }
                    Token::Empty | Token::Stop(_) => {
                        continue;
                    }
                    Token::Done => break,
                },
                Err(_) => {
                    panic!("Unexpected end of stream");
                }
            }
            self.time.incr_cycles(initiation_interval);
        }
        self.time.incr_cycles(latency);
    }
}
