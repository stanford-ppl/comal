use dam::{
    channel::{ChannelElement, PeekResult, Receiver, Sender},
    context::{self, Context},
    dam_macros::context_macro,
    simulation::ProgramBuilder,
    types::StaticallySized,
};
use derive_more::Constructor;

#[derive(Debug, Clone)]
pub struct HBMConfig {
    pub addr_offset: u64, // number of bytes read for each request
    pub channel_num: usize,
    pub per_channel_latency: u64,
    pub per_channel_init_interval: u64,
    pub per_channel_outstanding: usize,
    pub per_channel_start_up_time: u64, // Time to wait before the first request can be processed
}

#[derive(Constructor, Clone, Default, Debug)]
pub struct Request {
    is_write: bool,
    address: u64,
    id: usize, // Index of the reader or the writer
}

impl StaticallySized for Request {
    const SIZE: usize = 1 + 8 + 8;
}

#[derive(Clone, Debug)]
pub enum RequestEnum {
    Request(Request),
    Done,
}

impl StaticallySized for RequestEnum {
    const SIZE: usize = 1 + 8 + 8;
}
impl Default for RequestEnum {
    fn default() -> Self {
        RequestEnum::Done
    }
}

#[derive(Constructor, Clone, Default, Debug)]
pub struct Response {
    is_write: bool,
    address: u64,
    id: usize, // Index of the reader or the writer
}

impl StaticallySized for Response {
    const SIZE: usize = 1 + 8 + 8;
}

#[context_macro]
pub struct HBMChannelContext {
    in_request: Receiver<RequestEnum>,
    out_rsp: Sender<Response>,
    latency: u64,
    init_interval: u64,
    outstanding: usize,
    start_up_time: u64,
}

impl HBMChannelContext {
    pub fn new(
        in_request: Receiver<RequestEnum>,
        out_rsp: Sender<Response>,
        per_channel_latency: u64,
        per_channel_init_interval: u64,
        per_channel_outstanding: usize,
        per_channel_start_up_time: u64,
    ) -> Self {
        let ctx = Self {
            in_request,
            out_rsp,
            latency: per_channel_latency,
            init_interval: per_channel_init_interval,
            outstanding: per_channel_outstanding,
            start_up_time: per_channel_start_up_time,
            context_info: Default::default(),
        };
        ctx.in_request.attach_receiver(&ctx);
        ctx.out_rsp.attach_sender(&ctx);

        ctx
    }
}

impl Context for HBMChannelContext {
    fn run(&mut self) {
        let mut initial_request = true;
        loop {
            // check if there's enough slot for in-flight requests (self.outstanding)
            // to incorporate this, we might have to move to peek
            match self.in_request.dequeue(&self.time) {
                Ok(ChannelElement { time, data }) => {
                    match data {
                        RequestEnum::Request(req) => {
                            // Process the request
                            let enq_start_time = if initial_request {
                                // If this is the first request, we need to wait for the start-up time
                                self.time.tick() + self.start_up_time
                            } else {
                                self.time.tick()
                            };
                            self.out_rsp
                                .enqueue(
                                    &self.time,
                                    ChannelElement {
                                        time: enq_start_time + self.latency,
                                        data: Response::new(req.is_write, req.address, req.id),
                                    },
                                )
                                .unwrap();
                        }
                        RequestEnum::Done => return,
                    }
                }
                Err(_) => return,
            }
            self.time.incr_cycles(self.init_interval);
        }
    }
}

#[derive(Constructor)]
pub struct ChannelBundle {
    pub snd: Sender<RequestEnum>,
    pub rcv: Receiver<Response>,
}

#[derive(Constructor, Clone, Default, Debug)]
pub struct ParAddrs {
    pub addrs: Vec<u64>,
}

impl StaticallySized for ParAddrs {
    const SIZE: usize = 64;
}

#[derive(Constructor)]
pub struct ReadBundle {
    pub addr: Receiver<ParAddrs>,
    pub resp: Sender<u64>,
}

#[derive(Constructor)]
pub struct WriteBundle {
    pub addr: Receiver<ParAddrs>,
    pub resp: Sender<u64>,
}

#[context_macro]
pub struct HBMContext {
    channels: Vec<ChannelBundle>,
    readers: Vec<ReadBundle>,
    writers: Vec<WriteBundle>,
}

impl Context for HBMContext {
    fn run(&mut self) {
        let channel_num = self.channels.len();
        let mut off_set: usize = 0;

        while self.request_incoming() {
            // Collect finished requests in the current cycle and send the response back
            let responses = self.dequeue_responses_at_current_cycle();

            for resp in responses.iter() {
                if resp.is_write {
                    self.writers[resp.id]
                        .resp
                        .enqueue(
                            &self.time,
                            ChannelElement {
                                time: self.time.tick(),
                                data: resp.address,
                            },
                        )
                        .unwrap();
                } else {
                    self.readers[resp.id]
                        .resp
                        .enqueue(
                            &self.time,
                            ChannelElement {
                                time: self.time.tick(),
                                data: resp.address,
                            },
                        )
                        .unwrap();
                }
            }
            // Collect incoming requests in the current cycle and send it to channels
            let requests = self.dequeue_requests_at_current_cycle();

            let req_len = requests.len();
            for (i, request) in requests.into_iter().enumerate() {
                off_set = (off_set + i) % channel_num;

                self.channels[off_set]
                    .snd
                    .enqueue(
                        &self.time,
                        ChannelElement {
                            time: self.time.tick(),
                            data: RequestEnum::Request(request),
                        },
                    )
                    .unwrap();
            }
            // update the offset for the next set of requests
            off_set = (off_set + req_len) % channel_num;

            self.time.incr_cycles(1);
        }

        // Send a end request to each channel
        for channel in self.channels.iter() {
            channel
                .snd
                .enqueue(
                    &self.time,
                    ChannelElement {
                        time: self.time.tick(),
                        data: RequestEnum::Done,
                    },
                )
                .unwrap();
        }

        // collect resposnses from channels until all channels are done
        while self.channels_running() {
            let responses = self.dequeue_responses_at_current_cycle();

            for resp in responses.iter() {
                if resp.is_write {
                    self.writers[resp.id]
                        .resp
                        .enqueue(
                            &self.time,
                            ChannelElement {
                                time: self.time.tick(),
                                data: resp.address,
                            },
                        )
                        .unwrap();
                } else {
                    self.readers[resp.id]
                        .resp
                        .enqueue(
                            &self.time,
                            ChannelElement {
                                time: self.time.tick(),
                                data: resp.address,
                            },
                        )
                        .unwrap();
                }
            }

            self.time.incr_cycles(1);
        }
    }
}

impl HBMContext {
    pub fn new<'a>(builder: &mut ProgramBuilder<'a>, config: HBMConfig) -> Self {
        let mut channels = vec![];
        // Create Channels and attach to the ProgramBuilder
        for _ in 0..config.channel_num {
            let (req_snd, req_rcv) = builder.unbounded();
            let (rsp_snd, rsp_rcv) = builder.unbounded();

            builder.add_child(HBMChannelContext::new(
                req_rcv,
                rsp_snd,
                config.per_channel_latency,
                config.per_channel_init_interval,
                config.per_channel_outstanding,
                config.per_channel_start_up_time,
            ));

            channels.push(ChannelBundle::new(req_snd, rsp_rcv));
        }

        let ctx = Self {
            channels,
            readers: vec![],
            writers: vec![],
            context_info: Default::default(),
        };

        for bundle in ctx.channels.iter() {
            bundle.rcv.attach_receiver(&ctx);
            bundle.snd.attach_sender(&ctx);
        }

        ctx
    }

    pub fn add_reader(&mut self, ReadBundle { addr, resp }: ReadBundle) {
        addr.attach_receiver(self);
        resp.attach_sender(self);
        self.readers.push(ReadBundle { addr, resp });
    }

    pub fn add_writer(&mut self, WriteBundle { addr, resp }: WriteBundle) {
        addr.attach_receiver(self);
        resp.attach_sender(self);
        self.writers.push(WriteBundle { addr, resp });
    }

    fn dequeue_requests_at_current_cycle(&mut self) -> Vec<Request> {
        let mut requests = vec![];

        for (i, reader) in self.readers.iter().enumerate() {
            match reader.addr.peek() {
                PeekResult::Something(ChannelElement {
                    time: elem_time,
                    data: addr_vec,
                }) => {
                    let context_time = self.time.tick().time();
                    let element_visible_time = elem_time.time();
                    if context_time >= element_visible_time {
                        // If the response is visible at the current time, we can dequeue it
                        for addr_i in addr_vec.addrs {
                            requests.push(Request::new(false, addr_i, i));
                        }
                        reader.addr.dequeue(&self.time).unwrap();
                    } else {
                        // If not, we skip this response
                        continue;
                    }
                }
                PeekResult::Nothing(_time) => continue,
                PeekResult::Closed => continue,
            }
        }

        for (i, writer) in self.writers.iter().enumerate() {
            match writer.addr.peek() {
                PeekResult::Something(ChannelElement {
                    time: elem_time,
                    data: addr_vec,
                }) => {
                    let context_time = self.time.tick().time();
                    let element_visible_time = elem_time.time();
                    if context_time >= element_visible_time {
                        for addr_i in addr_vec.addrs {
                            requests.push(Request::new(true, addr_i, i));
                        }
                        writer.addr.dequeue(&self.time).unwrap();
                    }
                }
                PeekResult::Nothing(_time) => continue,
                PeekResult::Closed => continue,
            }
        }

        requests
    }

    fn dequeue_responses_at_current_cycle(&mut self) -> Vec<Response> {
        let mut responses = vec![];

        for (i, channel) in self.channels.iter().enumerate() {
            match channel.rcv.peek() {
                PeekResult::Something(ChannelElement {
                    time: elem_time,
                    data: addr,
                }) => {
                    let context_time = self.time.tick().time();
                    let element_visible_time = elem_time.time();
                    // if i == 0 {
                    //     println!(
                    //         "Channel {}: Peeked response at time {}, element time {}",
                    //         i, context_time, element_visible_time
                    //     );
                    // }
                    if context_time >= element_visible_time {
                        // If the response is visible at the current time, we can dequeue it
                        responses.push(addr);
                        channel.rcv.dequeue(&self.time).unwrap();
                    } else {
                        // If not, we skip this response
                        continue;
                    }
                }
                PeekResult::Nothing(_time) => continue,
                PeekResult::Closed => continue,
            }
        }
        responses
    }

    fn channels_running(&self) -> bool {
        let channels_all_closed =
            self.channels
                .iter()
                .all(|ChannelBundle { snd: _, rcv }| match rcv.peek() {
                    PeekResult::Closed => true,
                    _ => false,
                });
        !channels_all_closed
    }

    fn request_incoming(&mut self) -> bool {
        // check all of the writers
        let mut writers_done =
            self.writers
                .iter()
                .all(|WriteBundle { addr, resp }| match addr.peek() {
                    PeekResult::Closed => true,
                    _ => false,
                });

        if self.writers.is_empty() {
            writers_done = true;
        }

        if !writers_done {
            return true;
        }

        let readers_done =
            self.readers
                .iter()
                .all(|ReadBundle { addr, resp: _ }| match addr.peek() {
                    PeekResult::Closed => true,
                    _ => false,
                });

        if !readers_done {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod test {
    use dam::{
        channel::ChannelElement,
        simulation::{InitializationOptions, ProgramBuilder, RunOptions},
        utility_contexts::{FunctionContext, GeneratorContext},
    };

    use crate::templates::ramulator::hbm_context::{HBMConfig, HBMContext, ParAddrs, ReadBundle};

    #[test]
    fn read_from_two_bundle() {
        const MEM_SIZE: usize = 256;
        const PAR_DISPATCH: usize = 8;
        // The result of using any parallel dispatch factor equal or larger than
        // 4 gives the same result as we have two readers.
        // This is because we're using 8 channels in the HBM configuraation,
        // meaning that a parallel dispatch factor of 4 will saturate the parallelsim across channels.

        let mut parent = ProgramBuilder::default();

        let mut mem_context = HBMContext::new(
            &mut parent,
            HBMConfig {
                addr_offset: 64,
                channel_num: 8,
                per_channel_latency: 2,
                per_channel_init_interval: 2,
                per_channel_outstanding: 1, // For now, this does not have any effect
                per_channel_start_up_time: 14, // Time to wait before the first request can be processed
            },
        );

        // ========================== Read Bundle 1 =============================
        let (raddr_snd, raddr_rcv) = parent.unbounded();
        let (resp_addr_snd, resp_addr_rcv) = parent.unbounded();

        let addrs = || {
            (0..((MEM_SIZE / PAR_DISPATCH) as u64)).map(|x| ParAddrs::new(vec![0; PAR_DISPATCH]))
        };
        parent.add_child(GeneratorContext::new(addrs, raddr_snd));

        let mut read_ctx = FunctionContext::new();
        resp_addr_rcv.attach_receiver(&read_ctx);
        read_ctx.set_run(move |time| {
            let mut received_addr_time: Vec<(u64, u64)> = vec![];
            for _ in 0..MEM_SIZE {
                let (addr, addr_time) = match resp_addr_rcv.dequeue(time) {
                    Ok(ChannelElement {
                        data: addr,
                        time: _,
                    }) => (addr, time.tick().time()),
                    Err(_) => {
                        panic!("Failed to dequeue response address");
                    }
                };
                received_addr_time.push((addr, addr_time));
                // time.incr_cycles(1);
            }
            println!("Received: {:?}", received_addr_time);
        });
        parent.add_child(read_ctx);

        mem_context.add_reader(ReadBundle {
            addr: raddr_rcv,
            resp: resp_addr_snd,
        });

        // // ========================== Read Bundle 2 =============================
        let (raddr_snd2, raddr_rcv2) = parent.unbounded();
        let (resp_addr_snd2, resp_addr_rcv2) = parent.unbounded::<u64>();

        let addrs2 = || {
            (0..((MEM_SIZE / PAR_DISPATCH) as u64)).map(|x| ParAddrs::new(vec![32; PAR_DISPATCH]))
        };
        parent.add_child(GeneratorContext::new(addrs2, raddr_snd2));

        let mut read_ctx2 = FunctionContext::new();
        resp_addr_rcv2.attach_receiver(&read_ctx2);
        read_ctx2.set_run(move |time| {
            let mut received_addr_time: Vec<(u64, u64)> = vec![];
            for _ in 0..MEM_SIZE {
                let (addr, addr_time) = match resp_addr_rcv2.dequeue(time) {
                    Ok(ChannelElement {
                        time: _time,
                        data: addr,
                    }) => (addr, time.tick().time()),
                    Err(_) => {
                        panic!("Failed to dequeue response address");
                    }
                };
                received_addr_time.push((addr, addr_time));
                // time.incr_cycles(1);
            }
            println!("Received: {:?}", received_addr_time);
        });
        parent.add_child(read_ctx2);

        mem_context.add_reader(ReadBundle {
            addr: raddr_rcv2,
            resp: resp_addr_snd2,
        });

        parent.add_child(mem_context);

        println!("Finished building");

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());

        println!("Elapsed: {:?}", executed.elapsed_cycles());
    }

    #[test]
    fn read_from_eight_bundles() {
        const MEM_SIZE: usize = 32 * 4;
        const PAR_SENDS: i32 = 16;

        let mut parent = ProgramBuilder::default();

        let mut mem_context = HBMContext::new(
            &mut parent,
            HBMConfig {
                addr_offset: 64,
                channel_num: 8,
                per_channel_latency: 2,
                per_channel_init_interval: 2,
                per_channel_outstanding: 1,
                per_channel_start_up_time: 14,
            },
        );

        for i in 0..PAR_SENDS {
            let (raddr_snd, raddr_rcv) = parent.unbounded();
            let (resp_addr_snd, resp_addr_rcv) = parent.unbounded();

            // Each bundle generates a different address pattern for clarity
            let addrs =
                move || (0..(MEM_SIZE as u64)).map(move |_| ParAddrs::new(vec![i as u64 * 100]));
            parent.add_child(GeneratorContext::new(addrs, raddr_snd));

            let mut read_ctx = FunctionContext::new();
            resp_addr_rcv.attach_receiver(&read_ctx);
            let idx = i; // capture for move
            read_ctx.set_run(move |time| {
                let mut received_addr_time: Vec<(u64, u64)> = vec![];
                for _ in 0..MEM_SIZE {
                    let (addr, addr_time) = match resp_addr_rcv.dequeue(time) {
                        Ok(ChannelElement {
                            data: addr,
                            time: _,
                        }) => (addr, time.tick().time()),
                        Err(_) => {
                            panic!("Failed to dequeue response address for bundle {}", idx);
                        }
                    };
                    received_addr_time.push((addr, addr_time));
                }
                println!("Bundle {} received: {:?}", idx, received_addr_time);
            });
            parent.add_child(read_ctx);

            mem_context.add_reader(ReadBundle {
                addr: raddr_rcv,
                resp: resp_addr_snd,
            });
        }

        parent.add_child(mem_context);

        println!("Finished building");

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());

        println!("Elapsed: {:?}", executed.elapsed_cycles());
    }

    #[test]
    fn read_simple_32() {
        const MEM_SIZE: usize = 32 * 8;

        let mut parent = ProgramBuilder::default();

        let mut mem_context = HBMContext::new(
            &mut parent,
            HBMConfig {
                addr_offset: 64,
                channel_num: 8,
                per_channel_latency: 4,
                per_channel_init_interval: 4,
                per_channel_outstanding: 1, // For now, this does not have any effect
                per_channel_start_up_time: 14, // Time to wait before the first request can be processed
            },
        );

        // ========================== Read Bundle 1 =============================
        let (raddr_snd, raddr_rcv) = parent.unbounded();
        let (resp_addr_snd, resp_addr_rcv) = parent.unbounded();

        let addrs = || (0..(MEM_SIZE as u64)).map(|x| ParAddrs::new(vec![0]));
        parent.add_child(GeneratorContext::new(addrs, raddr_snd));

        let mut read_ctx = FunctionContext::new();
        resp_addr_rcv.attach_receiver(&read_ctx);
        read_ctx.set_run(move |time| {
            let mut received_addr_time: Vec<(u64, u64)> = vec![];
            for _ in 0..MEM_SIZE {
                let (addr, addr_time) = match resp_addr_rcv.dequeue(time) {
                    Ok(ChannelElement {
                        data: addr,
                        time: _,
                    }) => (addr, time.tick().time()),
                    Err(_) => {
                        panic!("Failed to dequeue response address");
                    }
                };
                received_addr_time.push((addr, addr_time));
                // time.incr_cycles(1);
            }
            println!("Received: {:?}", received_addr_time);
        });
        parent.add_child(read_ctx);

        mem_context.add_reader(ReadBundle {
            addr: raddr_rcv,
            resp: resp_addr_snd,
        });

        parent.add_child(mem_context);

        println!("Finished building");

        let executed = parent
            .initialize(InitializationOptions::default())
            .unwrap()
            .run(RunOptions::default());

        println!("Elapsed: {:?}", executed.elapsed_cycles());
    }
}
