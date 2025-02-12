use std::sync::{Arc, Mutex};

use dam::{context_tools::*, dam_macros::context_macro};

use super::{
    access::MemoryData, primitive::Token, ramulator_context::get_val_addr, utils::write_outputs,
};

#[context_macro]
pub struct CompressedWrScan<ValType: Clone, StopType: Clone> {
    pub input: Receiver<Token<ValType, StopType>>,
    pub seg_arr: Arc<Mutex<Vec<ValType>>>,
    pub crd_arr: Arc<Mutex<Vec<ValType>>>,
    pub data: Sender<MemoryData>,
    pub addr: Sender<u64>,
    pub ack: Receiver<bool>,
    pub base_addr: Option<u64>,
}

impl<ValType: DAMType, StopType: DAMType> CompressedWrScan<ValType, StopType>
where
    CompressedWrScan<ValType, StopType>: Context,
{
    pub fn new(
        input: Receiver<Token<ValType, StopType>>,
        data: Sender<MemoryData>,
        addr: Sender<u64>,
        ack: Receiver<bool>,
    ) -> Self {
        let cwr = CompressedWrScan {
            input,
            seg_arr: Default::default(),
            crd_arr: Default::default(),
            data,
            addr,
            ack,
            base_addr: None,
            context_info: Default::default(),
        };
        (cwr).input.attach_receiver(&cwr);
        (cwr).data.attach_sender(&cwr);
        (cwr).addr.attach_sender(&cwr);
        (cwr).ack.attach_receiver(&cwr);

        cwr
    }

    pub fn set_base_addr(&mut self, base: u64) {
        self.base_addr = Some(base);
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
        let mut cnt = 0;
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        crd_arr.push(val.clone());

                        let val_addr =
                            get_val_addr(self.base_addr.expect("Base addr is None"), cnt);

                        cnt += 1;

                        self.addr
                            .enqueue(
                                &self.time,
                                ChannelElement::new(self.time.tick() + 1, val_addr),
                            )
                            .unwrap();
                        // We don't care about storing the data so passing dummy data
                        self.data
                            .enqueue(
                                &self.time,
                                ChannelElement::new(self.time.tick() + 1, MemoryData::U32(0)),
                            )
                            .unwrap();

                        self.ack.dequeue(&self.time).unwrap();

                        curr_crd_cnt += 1;
                        end_fiber = false;

                        // println!("{:?}", val.clone());
                    }
                    Token::Stop(_) if !end_fiber => {
                        seg_arr.push(curr_crd_cnt.clone());

                        let val_addr =
                            get_val_addr(self.base_addr.expect("Base addr is None"), cnt);

                        cnt += 1;

                        self.addr
                            .enqueue(
                                &self.time,
                                ChannelElement::new(self.time.tick() + 1, val_addr),
                            )
                            .unwrap();
                        // We don't care about storing the data so passing dummy data
                        self.data
                            .enqueue(
                                &self.time,
                                ChannelElement::new(self.time.tick() + 1, MemoryData::U32(0)),
                            )
                            .unwrap();

                        self.ack.dequeue(&self.time).unwrap();

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
    pub data: Sender<MemoryData>,
    pub addr: Sender<u64>,
    pub ack: Receiver<bool>,
    pub base_addr: Option<u64>,
}

impl<ValType: DAMType, StopType: DAMType> ValsWrScan<ValType, StopType>
where
    ValsWrScan<ValType, StopType>: Context,
{
    pub fn new(
        input: Receiver<Token<ValType, StopType>>,
        data: Sender<MemoryData>,
        addr: Sender<u64>,
        ack: Receiver<bool>,
    ) -> Self {
        let vals = ValsWrScan {
            input,
            out_val: Default::default(),
            data,
            addr,
            ack,
            base_addr: None,
            context_info: Default::default(),
        };
        (vals.input).attach_receiver(&vals);
        (vals).data.attach_sender(&vals);
        (vals).addr.attach_sender(&vals);
        (vals).ack.attach_receiver(&vals);

        vals
    }
    pub fn set_base_addr(&mut self, base: u64) {
        self.base_addr = Some(base);
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
        let mut cnt = 0;
        loop {
            match self.input.dequeue(&self.time) {
                Ok(curr_in) => match curr_in.data {
                    Token::Val(val) => {
                        // println!("Value: {:?}", Token::<ValType, StopType>::Val(val.clone()));
                        locked.push(val.clone());
                        let val_addr =
                            get_val_addr(self.base_addr.expect("Base addr is None"), cnt);

                        cnt += 1;

                        self.addr
                            .enqueue(
                                &self.time,
                                ChannelElement::new(self.time.tick() + 1, val_addr),
                            )
                            .unwrap();
                        // We don't care about storing the data so passing dummy data
                        self.data
                            .enqueue(
                                &self.time,
                                ChannelElement::new(self.time.tick() + 1, MemoryData::U32(0)),
                            )
                            .unwrap();

                        self.ack.dequeue(&self.time).unwrap();
                        // println!("{:?}", val.clone());
                    }
                    Token::Empty | Token::Stop(_) => {
                        continue;
                    }
                    Token::Done => {
                        let filename: String = "/tmp/tmp_result.txt".to_string();
                        write_outputs(filename.into(), locked.to_vec());
                        // println!("res: {:?}", locked);
                        break;
                    }
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
