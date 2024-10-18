use std::{path::PathBuf, sync::{Arc, Mutex}};

use dam::{context_tools::*, dam_macros::context_macro};

use super::{primitive::Token, utils::write_outputs};

#[context_macro]
pub struct CompressedWrScan<ValType: Clone, StopType: Clone> {
    pub input: Receiver<Token<ValType, StopType>>,
    pub seg_arr: Arc<Mutex<Vec<ValType>>>,
    pub crd_arr: Arc<Mutex<Vec<ValType>>>,
}

impl<ValType: DAMType, StopType: DAMType> CompressedWrScan<ValType, StopType>
where
    CompressedWrScan<ValType, StopType>: Context,
{
    pub fn new(input: Receiver<Token<ValType, StopType>>) -> Self {
        let cwr = CompressedWrScan {
            input,
            seg_arr: Default::default(),
            crd_arr: Default::default(),
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
        self.seg_arr.lock().unwrap().push(ValType::default());
    }

    fn run(&mut self) {
        let mut curr_crd_cnt: ValType = ValType::default();
        let mut end_fiber = false;
        let initiation_interval = 1;

        let mut crd_arr = self.crd_arr.lock().unwrap();
        let mut seg_arr = self.seg_arr.lock().unwrap();
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        crd_arr.push(val.clone());
                        curr_crd_cnt += 1;
                        end_fiber = false;
                        // println!("{:?}", val.clone());
                    }
                    Token::Stop(_) if !end_fiber => {
                        seg_arr.push(curr_crd_cnt.clone());
                        end_fiber = true;
                    }
                    Token::Empty | Token::Stop(_) => {
                        // TODO: Maybe needs to be processed too

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
    pub out_val: Arc<Mutex<Vec<ValType>>>,
}

impl<ValType: DAMType, StopType: DAMType> ValsWrScan<ValType, StopType>
where
    ValsWrScan<ValType, StopType>: Context,
{
    pub fn new(input: Receiver<Token<ValType, StopType>>) -> Self {
        let vals = ValsWrScan {
            input,
            out_val: Default::default(),
            context_info: Default::default(),
        };
        (vals.input).attach_receiver(&vals);

        vals
    }
}

impl<ValType, StopType> Context for ValsWrScan<ValType, StopType>
where
    ValType: DAMType + std::fmt::Display,
    StopType: DAMType + std::ops::Add<u32, Output = StopType>,
{
    fn init(&mut self) {}

    fn run(&mut self) {
        let latency = 1;
        let initiation_interval = 1;
        let mut locked = self.out_val.lock().unwrap();
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        // println!("Value: {:?}", Token::<ValType, StopType>::Val(val.clone()));
                        locked.push(val.clone());
                        // println!("{:?}", val.clone());
                    }
                    Token::Empty | Token::Stop(_) => {
                        continue;
                    }
                    Token::Done => {
                        let filename : String = "/tmp/tmp_result.txt".to_string();
                        write_outputs(filename.into(), locked.to_vec());
                        // println!("res: {:?}", locked);
                        break;
                    },
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
