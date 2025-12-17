pub mod proto_headers;
pub mod util;

use crate::templates::locate::IterateLocate;
use std::collections::HashMap;
use std::env;
use std::marker::PhantomData;
use std::path::PathBuf;

use self::proto_headers::tortilla::operation::*;
use self::util::{get_repsig_id, AsStreamID};

use super::templates::accumulator::{Reduce, ReduceData, Spacc1, Spacc1Data};
use super::templates::alu::make_alu;
use super::templates::array::{Array, ArrayData};
use super::templates::crd_manager::{CrdDrop, CrdHold, CrdManagerData};
// use super::templates::joiner::{CrdJoinerData, Intersect, Union};
use super::templates::primitive::{Repsiggen, Token};
use super::templates::rd_scanner::{CompressedCrdRdScan, RdScanData, UncompressedCrdRdScan};
use super::templates::repeat::{RepSigGenData, Repeat, RepeatData, RepeatSigGen};
use super::templates::utils::{read_inputs, read_inputs_vectorized};
use crate::templates::tensor::{PrimitiveType, Tensor};
use ndarray::{Array2, Axis, CowArray, Ix2, ShapeBuilder};
use dam::types::StaticallySized;

// Block sparse value type aliases - NxN dense blocks
type VT16 = Tensor<'static, f32, Ix2, 16>;
type VT32 = Tensor<'static, f32, Ix2, 32>;
type VT64 = Tensor<'static, f32, Ix2, 64>;
use super::templates::wr_scanner::{CompressedWrScan, ValsWrScan};
use super::token_vec;
use crate::cli_common::SamOptions;
use crate::proto_driver::util::{get_crd_id, get_ref_id, get_val_id};
use crate::templates::accumulator::{MaxReduce, MaxReduceData, Spacc2, Spacc2Data};
use crate::templates::binary::Binary;
use crate::templates::joiner::{NIntersect, NJoinerData, NUnion};
use crate::templates::new_alu::{ALUAdd, ALUMul};
use crate::templates::primitive::ALUMaxOp;
use crate::templates::scatter_gather::{Gather, Scatter};
use crate::templates::unary::Unary;
// HBM timing interfaces
use crate::templates::ramulator::hbm_context::{
    HBMConfig, HBMContext, ParAddrs, ReadBundle, WriteBundle,
};

use super::templates::{alu::make_unary_alu, primitive::ALUExpOp};
use dam::channel::adapters::{RecvAdapter, SendAdapter};
use dam::context_tools::*;
use dam::simulation::ProgramBuilder;
use dam::templates::ops::*;
use dam::utility_contexts::{BroadcastContext, GeneratorContext};

// use joiner::Payload;
use proto_headers::tortilla::*;

type VT = f32;
type CT = u32;
type ST = u32;

enum ChannelType<T: DAMType> {
    SendType(Sender<T>),
    ReceiverType(Receiver<T>),
}

const DEFAULT_CHAN_SIZE: usize = 8192;

#[derive(Default)]
pub struct Channels<'a, T>
where
    T: DAMType,
{
    map: HashMap<u64, ChannelType<T>>,
    _marker: PhantomData<&'a ()>,
}

impl<'a, T: DAMType> Channels<'a, T>
where
    T: 'a,
{
    pub fn new_channel(parent: &mut ProgramBuilder<'a>, _id: u64) -> (Sender<T>, Receiver<T>) {
        parent.bounded(DEFAULT_CHAN_SIZE)
    }

    pub fn get_sender(&mut self, id: u64, parent: &mut ProgramBuilder<'a>) -> Sender<T> {
        if id == 0 {
            return parent.void();
        }
        match self.map.remove(&id) {
            Some(ChannelType::SendType(res)) => res,
            Some(_) => {
                panic!("Received receive type unexpectedly");
            }
            None => {
                let (snd, rcv) = Self::new_channel(parent, id);
                self.map.insert(id, ChannelType::ReceiverType(rcv));
                snd
            }
        }
    }
    pub fn get_receiver(&mut self, id: u64, parent: &mut ProgramBuilder<'a>) -> Receiver<T> {
        match self.map.remove(&id) {
            Some(ChannelType::ReceiverType(res)) => res,
            Some(_) => {
                panic!("Unexpected sender");
            }
            None => {
                let (snd, rcv) = Self::new_channel(parent, id);
                self.map.insert(id, ChannelType::SendType(snd));
                rcv
            }
        }
    }

    pub fn set_receiver(&mut self, id: u64, rcv: Receiver<T>) {
        self.map.insert(id, ChannelType::ReceiverType(rcv));
    }

    pub fn iter_remainders(self) -> impl Iterator<Item = Receiver<T>> {
        self.map.into_iter().map(|(id, chantype)| match chantype {
            ChannelType::SendType(_) => panic!("Disconnected sender with id {id:?}"),
            ChannelType::ReceiverType(recv) => recv,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_from_proto<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
    builder: &mut ProgramBuilder<'a>,
    refmap: &mut Channels<'a, Token<CT, ST>>,
    crdmap: &mut Channels<'a, Token<CT, ST>>,
    valmap: &mut Channels<'a, Token<VT, ST>>,
    repmap: &mut Channels<'a, Repsiggen>,
) {
    // Read HBM setting from environment variable (default: enabled)
    let enable_hbm = env::var("COMAL_ENABLE_HBM")
        .map(|v| v != "0" && v.to_lowercase() != "false")
        .unwrap_or(true);
    // Optional HBM memory timing context
    let mut hbm_ctx_opt = if enable_hbm {
        Some(HBMContext::new(
            builder,
            HBMConfig {
                addr_offset: 64,
                channel_num: 8,
                per_channel_latency: 2,
                per_channel_init_interval: 2,
                per_channel_outstanding: 1,
                per_channel_start_up_time: 14,
            },
        ))
    } else {
        None
    };
    for operation in comal_graph.graph.unwrap().operators {
        match operation.op.expect("Error processing") {
            Op::Broadcast(op) => match op.conn.as_ref().unwrap() {
                broadcast::Conn::Crd(in_crd) => {
                    let in_crd_id = in_crd.input.try_conv();
                    let out_crd_ids = in_crd.outputs.iter().map(|id| id.try_conv());
                    let receiver = crdmap.get_receiver(in_crd_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_crd_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(crdmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Ref(in_ref) => {
                    let in_ref_id = in_ref.input.try_conv();
                    let out_ref_ids = in_ref.outputs.iter().map(|id| id.try_conv());
                    let receiver = refmap.get_receiver(in_ref_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_ref_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(refmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Val(in_val) => {
                    let in_val_id = in_val.input.try_conv();
                    let out_val_ids = in_val.outputs.iter().map(|id| id.try_conv());
                    let receiver = valmap.get_receiver(in_val_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_val_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(valmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Repsig(in_repsig) => {
                    let in_repsig_id = in_repsig.input.try_conv();
                    let out_repsig_ids = in_repsig.outputs.iter().map(|id| id.try_conv());
                    let receiver = repmap.get_receiver(in_repsig_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_repsig_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(repmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
            },
            Op::Joiner(op) => {
                // assert!(op.input_pairs.len() == 2);
                let mut in_crds = Vec::new();
                let mut in_refs: Vec<Box<dyn RecvAdapter<Token<_, ST>> + Send + Sync>> = Vec::new();
                let mut out_refs: Vec<Box<dyn SendAdapter<Token<_, ST>> + Send + Sync>> =
                    Vec::new();
                op.input_pairs.iter().for_each(|pair| {
                    let pair_crd = crdmap.get_receiver(get_crd_id(&pair.crd), builder);
                    match pair.in_ref.clone().unwrap().stream.as_ref().unwrap() {
                        joiner::payload::Stream::RefStream(ref_stream) => {
                            in_refs.push(Box::new(
                                refmap.get_receiver(get_ref_id(&Some(ref_stream.clone())), builder),
                            ));
                        }
                        joiner::payload::Stream::ValStream(val_stream) => {
                            in_refs.push(Box::new(
                                valmap.get_receiver(get_val_id(&Some(val_stream.clone())), builder),
                            ));
                        }
                    }

                    in_crds.push(pair_crd);
                });
                op.output_refs.iter().for_each(|output_ref| {
                    match output_ref.stream.as_ref().unwrap() {
                        joiner::payload::Stream::RefStream(ref_stream) => out_refs.push(Box::new(
                            refmap.get_sender(get_ref_id(&Some(ref_stream.clone())), builder),
                        )),
                        joiner::payload::Stream::ValStream(val_stream) => out_refs.push(Box::new(
                            valmap.get_sender(get_val_id(&Some(val_stream.clone())), builder),
                        )),
                    }
                });
                let joiner_data = NJoinerData {
                    in_crds,
                    in_refs,
                    out_refs,
                    out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder),
                };

                if let joiner::Type::Intersect = op.join_type() {
                    builder.add_child(NIntersect::new(joiner_data))
                } else {
                    builder.add_child(NUnion::new(joiner_data))
                };
            }
            Op::FiberLookup(op) => {
                let in_ref = refmap.get_receiver(get_ref_id(&op.input_ref), builder);

                let f_data = RdScanData {
                    in_ref,
                    out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder),
                    out_ref: refmap.get_sender(get_ref_id(&op.output_ref), builder),
                };
                if op.format == "compressed" {
                    // dbg!(op.tensor.clone());
                    // dbg!(op.mode);
                    let seg_filename =
                        base_path.join(format!("tensor_{}_mode_{}_seg", op.tensor, op.mode));
                    let crd_filename =
                        base_path.join(format!("tensor_{}_mode_{}_crd", op.tensor, op.mode));
                    let seg = read_inputs(&seg_filename);
                    let crd = read_inputs(&crd_filename);
                    let mut crs = CompressedCrdRdScan::new(f_data, seg, crd);
                    crs.set_timings(sam_options.compressed_read_config);
                    if let Some(hbm) = hbm_ctx_opt.as_mut() {
                        // Hook HBM read bundles for seg/crd
                        let (seg_addr_snd, seg_addr_rcv) = builder.unbounded::<ParAddrs>();
                        let (seg_resp_snd, seg_resp_rcv) = builder.unbounded::<u64>();
                        let (crd_addr_snd, crd_addr_rcv) = builder.unbounded::<ParAddrs>();
                        let (crd_resp_snd, crd_resp_rcv) = builder.unbounded::<u64>();
                        hbm.add_reader(ReadBundle {
                            addr: seg_addr_rcv,
                            resp: seg_resp_snd,
                        });
                        hbm.add_reader(ReadBundle {
                            addr: crd_addr_rcv,
                            resp: crd_resp_snd,
                        });
                        // Assume 32-bit words
                        crs.enable_hbm(
                            seg_addr_snd,
                            seg_resp_rcv,
                            crd_addr_snd,
                            crd_resp_rcv,
                            0x1000_0000,
                            0x2000_0000,
                            4,
                            4,
                        );
                    }
                    builder.add_child(crs);
                } else {
                    let shape_filename = base_path.join(format!("tensor_{}_mode_shape", op.tensor));
                    let shapes = read_inputs(&shape_filename);
                    let index: usize = op.mode.try_into().unwrap();
                    builder.add_child(UncompressedCrdRdScan::new(f_data, shapes[index.clone()]));
                }
            }
            Op::FiberWrite(op) => {
                let in_crd_id = get_crd_id(&op.input_crd);
                let receiver = crdmap.get_receiver(in_crd_id, builder);
                let mut wr = CompressedWrScan::new(receiver);
                if let Some(hbm) = hbm_ctx_opt.as_mut() {
                    // Hook HBM writer bundles for crd and seg
                    let (crd_addr_snd, crd_addr_rcv) = builder.unbounded::<ParAddrs>();
                    let (crd_resp_snd, crd_resp_rcv) = builder.unbounded::<u64>();
                    let (seg_addr_snd, seg_addr_rcv) = builder.unbounded::<ParAddrs>();
                    let (seg_resp_snd, seg_resp_rcv) = builder.unbounded::<u64>();
                    hbm.add_writer(WriteBundle {
                        addr: crd_addr_rcv,
                        resp: crd_resp_snd,
                    });
                    hbm.add_writer(WriteBundle {
                        addr: seg_addr_rcv,
                        resp: seg_resp_snd,
                    });
                    wr.enable_hbm_writes(
                        crd_addr_snd,
                        crd_resp_rcv,
                        seg_addr_snd,
                        seg_resp_rcv,
                        0x4000_0000,
                        0x5000_0000,
                        4,
                        4,
                    );
                }
                builder.add_child(wr);
            }
            Op::Repeat(op) => {
                // TODO: Need to check if input_rep_crd exists for backwards compatibility
                // match &op.input_rep_crd {}

                let (out_repsig, in_repsig) = builder.bounded(DEFAULT_CHAN_SIZE);
                match op.input_rep_sig {
                    Some(in_rep) => match in_rep {
                        repeat::InputRepSig::RepRef(rep_ref) => {
                            let in_rep_ref = get_ref_id(&Some(rep_ref));
                            let repsig_data = RepSigGenData {
                                input: refmap.get_receiver(in_rep_ref, builder),
                                out_repsig,
                            };
                            builder.add_child(RepeatSigGen::new(repsig_data));
                        }
                        repeat::InputRepSig::RepVal(rep_val) => {
                            let in_rep_val = get_val_id(&Some(rep_val));
                            let repsig_data = RepSigGenData {
                                input: valmap.get_receiver(in_rep_val, builder),
                                out_repsig,
                            };
                            builder.add_child(RepeatSigGen::new(repsig_data));
                        }
                    },
                    None => todo!(),
                }
                // let repsig_data = RepSigGenData {
                //     input: refmap.get_receiver(in_rep_ref, builder),
                //     out_repsig,
                // };

                match op.input_ref {
                    Some(input_ref) => match input_ref {
                        repeat::InputRef::InRef(in_ref_stream) => {
                            let in_ref =
                                refmap.get_receiver(get_ref_id(&Some(in_ref_stream)), builder);

                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(out_ref_stream) => {
                                        let rep_data = RepeatData {
                                            in_ref,
                                            in_repsig,
                                            out_ref: refmap.get_sender(
                                                get_ref_id(&Some(out_ref_stream)),
                                                builder,
                                            ),
                                        };
                                        builder.add_child(Repeat::new(rep_data));
                                    }
                                    repeat::OutputRef::OutVal(_) => todo!(),
                                },
                                None => todo!(),
                            }
                        }
                        repeat::InputRef::InVal(in_val_stream) => {
                            let in_val =
                                valmap.get_receiver(get_val_id(&Some(in_val_stream)), builder);

                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(_) => todo!(),
                                    repeat::OutputRef::OutVal(out_val_stream) => {
                                        let rep_data = RepeatData {
                                            in_ref: in_val,
                                            in_repsig,
                                            out_ref: valmap.get_sender(
                                                get_val_id(&Some(out_val_stream)),
                                                builder,
                                            ),
                                        };
                                        builder.add_child(Repeat::new(rep_data));
                                    }
                                },
                                None => todo!(),
                            }
                        }
                    },
                    None => todo!(),
                }
            }
            Op::Repeatsig(op) => {
                let in_crd_id = get_crd_id(&op.input_crd);
                let repsig_data = RepSigGenData {
                    input: crdmap.get_receiver(in_crd_id, builder),
                    out_repsig: repmap.get_sender(get_repsig_id(&op.output_rep_sig), builder),
                };
                builder.add_child(RepeatSigGen::new(repsig_data));
            }
            Op::Alu(op) => {
                let mut in_val_ids = match op.conn.as_ref().unwrap() {
                    alu::Conn::Vals(val) => val
                        .inputs
                        .iter()
                        .map(|input_val| get_val_id(&Some(input_val.clone()))),
                    alu::Conn::Crds(_) => todo!(),
                };
                let out_val_id = match op.conn.as_ref().unwrap() {
                    alu::Conn::Vals(val) => get_val_id(&val.output),
                    alu::Conn::Crds(_) => todo!(),
                };
                assert!(in_val_ids.len() >= 1);
                let out_val_sender = valmap.get_sender(out_val_id, builder);
                if in_val_ids.len() == 2 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    let val_receiver2 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    // if op.stages[0].op() == alu::AluOp::Add {
                    //     builder.add_child(ALUAdd::new(
                    //         val_receiver1,
                    //         val_receiver2,
                    //         out_val_sender,
                    //     ));
                    // } else {
                    //     builder.add_child(make_alu(
                    //         val_receiver1,
                    //         val_receiver2,
                    //         out_val_sender,
                    //         match op.stages[0].op() {
                    //             alu::AluOp::Add => ALUAddOp(),
                    //             alu::AluOp::Sub => ALUSubOp(),
                    //             alu::AluOp::Mul => ALUMulOp(),
                    //             alu::AluOp::Div => ALUDivOp(),
                    //             _ => todo!(),
                    //         },
                    //     ));
                    // }
                    let latency = 1;
                    let ii = 1;
                    let binary_func = match op.stages[0].op() {
                        alu::AluOp::Add => |val1: VT, val2: VT| -> VT { val1 + val2 },
                        alu::AluOp::Sub => |val1: VT, val2: VT| -> VT { val1 - val2 },
                        alu::AluOp::Mul => |val1: VT, val2: VT| -> VT { val1 * val2 },
                        alu::AluOp::Div => |val1: VT, val2: VT| -> VT { val1 / val2 },
                        alu::AluOp::Elemmul => |val1: VT, val2: VT| -> VT { val1 * val2 },
                        _ => todo!(),
                    };
                    builder.add_child(Binary::new(
                        val_receiver1,
                        val_receiver2,
                        out_val_sender,
                        binary_func,
                        1,
                        latency.try_into().unwrap(),
                        ii.try_into().unwrap(),
                    ));
                } else if in_val_ids.len() == 1 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    match op.stages[0].op() {
                        alu::AluOp::Exp => {
                            let unary_func = |val: f32| -> f32 { val.exp() };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                1,
                            ));
                        }
                        alu::AluOp::Sin => {
                            let unary_func = |val: f32| -> f32 { val.sin() };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                1,
                            ));
                        }
                        alu::AluOp::Cos => {
                            let unary_func = |val: f32| -> f32 { val.cos() };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                1,
                            ));
                        }
                        alu::AluOp::Max => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: f32| -> f32 { val.max(scalar) };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                1,
                            ));
                        }
                        alu::AluOp::Scalaradd => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: f32| -> f32 { val + scalar };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                1,
                            ));
                        }
                        alu::AluOp::Scalarmul => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: f32| -> f32 { val * scalar };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                1,
                            ));
                        }
                        alu::AluOp::Scalardiv => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: f32| -> f32 { val / scalar };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                1,
                            ));
                        }
                        alu::AluOp::Rsqrt => {
                            let unary_func = |val: f32| -> f32 { 1.0 / val.sqrt() };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                1,
                            ));
                        }
                        alu::AluOp::Sigmoid => {
                            let unary_func = |val: f32| -> f32 { 1.0 / (1.0 + f32::exp(-val)) };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                1,
                            ));
                        }
                        _ => {
                            panic!("Should not reach binary op cases")
                        }
                    }
                }
            }
            Op::Reduce(op) => {
                let in_val_id = get_val_id(&op.input_val);
                match op.reduce_type() {
                    reduce::Type::Add => {
                        let reduce_data = ReduceData::<VT, ST, 1> {
                            in_val: valmap.get_receiver(in_val_id, builder),
                            out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                            sum: false,
                        };
                        builder.add_child(Reduce::<VT, ST, 1>::new(reduce_data));
                    }
                    reduce::Type::Max => {
                        let max_reduce_data = MaxReduceData::<VT, ST> {
                            in_val: valmap.get_receiver(in_val_id, builder),
                            out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                        };
                        builder.add_child(MaxReduce::new(max_reduce_data, f32::MIN));
                    }
                    reduce::Type::Addsum => {
                        let reduce_data = ReduceData::<VT, ST, 1> {
                            in_val: valmap.get_receiver(in_val_id, builder),
                            out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                            sum: true,
                        };
                        builder.add_child(Reduce::<VT, ST, 1>::new(reduce_data));
                    }
                }
            }
            Op::CoordHold(op) => {
                let in_inner_crd = get_crd_id(&op.input_inner_crd);
                let in_outer_crd = get_crd_id(&op.input_outer_crd);

                let crd_hold_data = CrdManagerData {
                    in_crd_inner: crdmap.get_receiver(in_inner_crd, builder),
                    in_crd_outer: crdmap.get_receiver(in_outer_crd, builder),
                    out_crd_inner: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                    out_crd_outer: crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder),
                };
                builder.add_child(CrdHold::new(crd_hold_data));
            }
            Op::CoordDrop(op) => {
                let in_inner_crd = get_crd_id(&op.input_inner_crd);
                let in_outer_crd = get_crd_id(&op.input_outer_crd);

                let crd_drop_data = CrdManagerData {
                    in_crd_inner: crdmap.get_receiver(in_inner_crd, builder),
                    in_crd_outer: crdmap.get_receiver(in_outer_crd, builder),
                    out_crd_inner: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                    out_crd_outer: crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder),
                };
                builder.add_child(CrdDrop::new(crd_drop_data));
            }
            Op::Locate(op) => {
                let in_ref_id = get_ref_id(&op.input_ref);
                let in_crd_id = get_crd_id(&op.input_crd);
                let out_ref1_id = get_ref_id(&op.output_ref1);
                let out_ref2_id = get_ref_id(&op.output_ref2);
                let out_crd_id = get_crd_id(&op.output_crd);
                let locate = IterateLocate::new(
                    refmap.get_receiver(in_ref_id, builder),
                    crdmap.get_receiver(in_crd_id, builder),
                    refmap.get_sender(out_ref1_id, builder),
                    refmap.get_sender(out_ref2_id, builder),
                    crdmap.get_sender(out_crd_id, builder),
                );
                builder.add_child(locate);
            }
            Op::Array(op) => {
                let in_ref_id = get_ref_id(&op.input_ref);
                let array_data = ArrayData {
                    in_ref: refmap.get_receiver(in_ref_id, builder),
                    out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                    block_size: 1,  // Scalar mode
                };
                let val_filename = base_path.join(format!("tensor_{}_mode_vals", op.tensor));
                let vals = read_inputs(&val_filename);
                let mut arr = Array::new(array_data, vals);
                if let Some(hbm) = hbm_ctx_opt.as_mut() {
                    let (rd_addr_snd, rd_addr_rcv) = builder.unbounded::<ParAddrs>();
                    let (rd_resp_snd, rd_resp_rcv) = builder.unbounded::<u64>();
                    hbm.add_reader(ReadBundle {
                        addr: rd_addr_rcv,
                        resp: rd_resp_snd,
                    });
                    arr.enable_hbm_reads(rd_addr_snd, rd_resp_rcv, 0x6000_0000, 4);
                }
                builder.add_child(arr);
            }
            Op::Spacc(op) => {
                let in_inner_crd = get_crd_id(&op.input_inner_crd);
                let order = op.order;

                assert_ne!(order, 0);

                if order == 1 {
                    let in_outer_crd = op.input_outer_crds[0].try_conv();
                    let in_val_id = get_val_id(&op.input_val);

                    let spacc_data = Spacc1Data {
                        in_crd_inner: crdmap.get_receiver(in_inner_crd, builder),
                        in_crd_outer: crdmap.get_receiver(in_outer_crd, builder),
                        in_val: valmap.get_receiver(in_val_id, builder),
                        out_crd_inner: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                        out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                    };
                    builder.add_child(Spacc1::new(spacc_data));
                } else if order == 2 {
                    let in_crd1 = op.input_outer_crds[0].clone().try_conv();
                    let in_crd2 = op.input_outer_crds[1].clone().try_conv();
                    let in_val_id = get_val_id(&op.input_val);

                    let spacc2_data = Spacc2Data {
                        in_val: valmap.get_receiver(in_val_id, builder),
                        in_crd0: crdmap.get_receiver(in_inner_crd, builder),
                        in_crd1: crdmap.get_receiver(in_crd1, builder),
                        in_crd2: crdmap.get_receiver(in_crd2, builder),
                        out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                        out_crd0: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                        out_crd1: crdmap.get_sender(
                            get_crd_id(&Some(op.output_outer_crds[0].clone())),
                            builder,
                        ),
                    };
                    builder.add_child(Spacc2::new(spacc2_data));
                }
            }
            Op::ValWrite(op) => {
                let in_val_id = get_val_id(&op.input_val);
                let val_receiver = valmap.get_receiver(in_val_id, builder);
                let mut vals = ValsWrScan::new(val_receiver);
                if let Some(hbm) = hbm_ctx_opt.as_mut() {
                    let (wr_addr_snd, wr_addr_rcv) = builder.unbounded::<ParAddrs>();
                    let (wr_resp_snd, wr_resp_rcv) = builder.unbounded::<u64>();
                    hbm.add_writer(WriteBundle {
                        addr: wr_addr_rcv,
                        resp: wr_resp_snd,
                    });
                    vals.enable_hbm_writes(wr_addr_snd, wr_resp_rcv, 0x3000_0000, 4);
                }
                builder.add_child(vals);
            }
            Op::CoordMask(_) => unimplemented!("SAMML can't output coord mask op yet"),
            operation::Op::Func(_) => todo!(),
            Op::Root(op) => {
                let out_ref_id = get_ref_id(&op.output_ref);

                let root_sender = refmap.get_sender(out_ref_id, builder);
                builder.add_child(GeneratorContext::new(
                    || token_vec!(u32; u32; 0, "D").into_iter(),
                    root_sender,
                ));
                // root_receiver
            }
            Op::Fork(op) => match op.conn.as_ref().unwrap() {
                fork::Conn::Crd(in_crd) => {
                    let in_crd_id = in_crd.input.try_conv();
                    let out_crd_ids = in_crd.outputs.iter().map(|id| id.try_conv());
                    let receiver = crdmap.get_receiver(in_crd_id, builder);
                    let mut broadcast = Scatter::new(receiver);
                    out_crd_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(crdmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                fork::Conn::Ref(in_ref) => {
                    let in_ref_id = in_ref.input.try_conv();
                    let out_ref_ids = in_ref.outputs.iter().map(|id| id.try_conv());
                    let receiver = refmap.get_receiver(in_ref_id, builder);
                    let mut scatter = Scatter::new(receiver);
                    out_ref_ids
                        .into_iter()
                        .for_each(|id| scatter.add_target(refmap.get_sender(id, builder)));
                    builder.add_child(scatter);
                }
                fork::Conn::Val(in_val) => {
                    let in_val_id = in_val.input.try_conv();
                    let out_val_ids = in_val.outputs.iter().map(|id| id.try_conv());
                    let receiver = valmap.get_receiver(in_val_id, builder);
                    let mut broadcast = Scatter::new(receiver);
                    out_val_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(valmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                fork::Conn::Repsig(_) => {
                    panic!("Attempting to fork a repsig");
                }
            },
            Op::Join(op) => match op.conn.as_ref().unwrap() {
                join::Conn::Crd(in_crd) => {
                    let in_crd_id = in_crd.output.try_conv();
                    let sender = crdmap.get_sender(in_crd_id, builder);
                    let out_crd_ids = in_crd.inputs.iter().map(|id| id.try_conv());
                    let mut gather = Gather::new(sender);
                    out_crd_ids
                        .into_iter()
                        .for_each(|id| gather.add_target(crdmap.get_receiver(id, builder)));
                    builder.add_child(gather);
                }
                join::Conn::Ref(in_ref) => {
                    let in_ref_id = in_ref.output.try_conv();
                    let out_ref_ids = in_ref.inputs.iter().map(|id| id.try_conv());
                    let sender = refmap.get_sender(in_ref_id, builder);
                    let mut gather = Gather::new(sender);
                    out_ref_ids
                        .into_iter()
                        .for_each(|id| gather.add_target(refmap.get_receiver(id, builder)));
                    builder.add_child(gather);
                }
                join::Conn::Val(in_val) => {
                    let in_val_id = in_val.output.try_conv();
                    let out_val_ids = in_val.inputs.iter().map(|id| id.try_conv());
                    let sender = valmap.get_sender(in_val_id, builder);
                    let mut gather = Gather::new(sender);
                    out_val_ids
                        .into_iter()
                        .for_each(|id| gather.add_target(valmap.get_receiver(id, builder)));
                    builder.add_child(gather);
                }
                join::Conn::Repsig(_) => {
                    panic!("Attempting to join repsig");
                }
            },
            _ => todo!(),
        }
    }
    if let Some(hbm) = hbm_ctx_opt {
        builder.add_child(hbm);
    }
}

pub fn parse_proto<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
) -> ProgramBuilder<'a> {
    let mut builder = ProgramBuilder::default();
    build_from_proto(
        comal_graph,
        base_path,
        sam_options,
        &mut builder,
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
    );
    builder
}

// ============================================================================
// Block Sparse Mode Functions
// ============================================================================
// Block sparse mode uses scalar values but applies block-based timing.
// The block_size parameter controls latency: block_size^2 cycles per operation.

/// Build proto graph with specified block size for timing
#[allow(clippy::too_many_arguments)]
pub fn build_from_proto_with_block_size<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
    block_size: usize,
    builder: &mut ProgramBuilder<'a>,
    refmap: &mut Channels<'a, Token<CT, ST>>,
    crdmap: &mut Channels<'a, Token<CT, ST>>,
    valmap: &mut Channels<'a, Token<VT, ST>>,
    repmap: &mut Channels<'a, Repsiggen>,
) {
    let hbm_ctx_opt: Option<HBMContext> = None;
    for operation in comal_graph.graph.unwrap().operators {
        match operation.op.expect("Error processing") {
            Op::Array(op) => {
                let in_ref_id = get_ref_id(&op.input_ref);
                let array_data = ArrayData {
                    in_ref: refmap.get_receiver(in_ref_id, builder),
                    out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                    block_size,  // Use specified block size for timing
                };
                let val_filename = base_path.join(format!("tensor_{}_mode_vals", op.tensor));
                let vals = read_inputs(&val_filename);
                let arr = Array::new(array_data, vals);
                builder.add_child(arr);
            }
            // For other operations, delegate to the main build_from_proto
            _ => {}
        }
    }
    // Call main function for non-Array operations
    // This is a simplified approach - full implementation would parameterize all ops
    build_from_proto(
        ComalGraph::default(), // Empty graph since we handled Array above
        base_path.clone(),
        sam_options.clone(),
        builder,
        refmap,
        crdmap,
        valmap,
        repmap,
    );
}

/// Parse proto graph with block size 16
pub fn parse_proto_block16<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
) -> ProgramBuilder<'a> {
    // Use scalar values with block_size=16 timing
    parse_proto_with_block_size(comal_graph, base_path, sam_options, 16)
}

/// Parse proto graph with block size 32
pub fn parse_proto_block32<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
) -> ProgramBuilder<'a> {
    // Use scalar values with block_size=32 timing
    parse_proto_with_block_size(comal_graph, base_path, sam_options, 32)
}

/// Parse proto graph with block size 64
pub fn parse_proto_block64<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
) -> ProgramBuilder<'a> {
    // Use scalar values with block_size=64 timing
    parse_proto_with_block_size(comal_graph, base_path, sam_options, 64)
}

/// Helper to parse proto with specific block size
fn parse_proto_with_block_size<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
    block_size: usize,
) -> ProgramBuilder<'a> {
    let mut builder = ProgramBuilder::default();
    build_from_proto_parameterized(
        comal_graph,
        base_path,
        sam_options,
        block_size,
        &mut builder,
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
        None,
    );
    builder
}

/// Parameterized build function that accepts block_size
#[allow(clippy::too_many_arguments)]
pub fn build_from_proto_parameterized<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
    block_size: usize,
    builder: &mut ProgramBuilder<'a>,
    refmap: &mut Channels<'a, Token<CT, ST>>,
    crdmap: &mut Channels<'a, Token<CT, ST>>,
    valmap: &mut Channels<'a, Token<VT, ST>>,
    repmap: &mut Channels<'a, Repsiggen>,
    mut hbm_ctx_opt: Option<HBMContext>,
) {
    for operation in comal_graph.graph.unwrap().operators {
        match operation.op.expect("Error processing") {
            Op::Broadcast(op) => match op.conn.as_ref().unwrap() {
                broadcast::Conn::Crd(in_crd) => {
                    let in_crd_id = in_crd.input.try_conv();
                    let out_crd_ids = in_crd.outputs.iter().map(|id| id.try_conv());
                    let receiver = crdmap.get_receiver(in_crd_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_crd_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(crdmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Ref(in_ref) => {
                    let in_ref_id = in_ref.input.try_conv();
                    let out_ref_ids = in_ref.outputs.iter().map(|id| id.try_conv());
                    let receiver = refmap.get_receiver(in_ref_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_ref_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(refmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Val(in_val) => {
                    let in_val_id = in_val.input.try_conv();
                    let out_val_ids = in_val.outputs.iter().map(|id| id.try_conv());
                    let receiver = valmap.get_receiver(in_val_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_val_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(valmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Repsig(in_repsig) => {
                    let in_repsig_id = in_repsig.input.try_conv();
                    let out_repsig_ids = in_repsig.outputs.iter().map(|id| id.try_conv());
                    let receiver = repmap.get_receiver(in_repsig_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_repsig_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(repmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
            },
            Op::Joiner(op) => {
                let mut in_crds = Vec::new();
                let mut in_refs: Vec<Box<dyn RecvAdapter<Token<_, ST>> + Send + Sync>> = Vec::new();
                let mut out_refs: Vec<Box<dyn SendAdapter<Token<_, ST>> + Send + Sync>> =
                    Vec::new();
                op.input_pairs.iter().for_each(|pair| {
                    let pair_crd = crdmap.get_receiver(get_crd_id(&pair.crd), builder);
                    match pair.in_ref.clone().unwrap().stream.as_ref().unwrap() {
                        joiner::payload::Stream::RefStream(ref_stream) => {
                            in_refs.push(Box::new(
                                refmap.get_receiver(get_ref_id(&Some(ref_stream.clone())), builder),
                            ));
                        }
                        joiner::payload::Stream::ValStream(val_stream) => {
                            in_refs.push(Box::new(
                                valmap.get_receiver(get_val_id(&Some(val_stream.clone())), builder),
                            ));
                        }
                    }
                    in_crds.push(pair_crd);
                });
                op.output_refs.iter().for_each(|output_ref| {
                    match output_ref.stream.as_ref().unwrap() {
                        joiner::payload::Stream::RefStream(ref_stream) => out_refs.push(Box::new(
                            refmap.get_sender(get_ref_id(&Some(ref_stream.clone())), builder),
                        )),
                        joiner::payload::Stream::ValStream(val_stream) => out_refs.push(Box::new(
                            valmap.get_sender(get_val_id(&Some(val_stream.clone())), builder),
                        )),
                    }
                });
                let joiner_data = NJoinerData {
                    in_crds,
                    in_refs,
                    out_refs,
                    out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder),
                };

                if let joiner::Type::Intersect = op.join_type() {
                    builder.add_child(NIntersect::new(joiner_data))
                } else {
                    builder.add_child(NUnion::new(joiner_data))
                };
            }
            Op::FiberLookup(op) => {
                let in_ref = refmap.get_receiver(get_ref_id(&op.input_ref), builder);

                let f_data = RdScanData {
                    in_ref,
                    out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder),
                    out_ref: refmap.get_sender(get_ref_id(&op.output_ref), builder),
                };
                if op.format == "compressed" {
                    let seg_filename =
                        base_path.join(format!("tensor_{}_mode_{}_seg", op.tensor, op.mode));
                    let crd_filename =
                        base_path.join(format!("tensor_{}_mode_{}_crd", op.tensor, op.mode));
                    let seg = read_inputs(&seg_filename);
                    let crd = read_inputs(&crd_filename);
                    let mut crs = CompressedCrdRdScan::new(f_data, seg, crd);
                    crs.set_timings(sam_options.compressed_read_config);
                    builder.add_child(crs);
                } else {
                    let shape_filename = base_path.join(format!("tensor_{}_mode_shape", op.tensor));
                    let shapes = read_inputs(&shape_filename);
                    let index: usize = op.mode.try_into().unwrap();
                    builder.add_child(UncompressedCrdRdScan::new(f_data, shapes[index.clone()]));
                }
            }
            Op::FiberWrite(op) => {
                let in_crd_id = get_crd_id(&op.input_crd);
                let receiver = crdmap.get_receiver(in_crd_id, builder);
                let wr = CompressedWrScan::new(receiver);
                builder.add_child(wr);
            }
            Op::Repeat(op) => {
                let (out_repsig, in_repsig) = builder.bounded(DEFAULT_CHAN_SIZE);
                match op.input_rep_sig {
                    Some(in_rep) => match in_rep {
                        repeat::InputRepSig::RepRef(rep_ref) => {
                            let in_rep_ref = get_ref_id(&Some(rep_ref));
                            let repsig_data = RepSigGenData {
                                input: refmap.get_receiver(in_rep_ref, builder),
                                out_repsig,
                            };
                            builder.add_child(RepeatSigGen::new(repsig_data));
                        }
                        repeat::InputRepSig::RepVal(rep_val) => {
                            let in_rep_val = get_val_id(&Some(rep_val));
                            let repsig_data = RepSigGenData {
                                input: valmap.get_receiver(in_rep_val, builder),
                                out_repsig,
                            };
                            builder.add_child(RepeatSigGen::new(repsig_data));
                        }
                    },
                    None => todo!(),
                }

                match op.input_ref {
                    Some(input_ref) => match input_ref {
                        repeat::InputRef::InRef(in_ref_stream) => {
                            let in_ref =
                                refmap.get_receiver(get_ref_id(&Some(in_ref_stream)), builder);

                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(out_ref_stream) => {
                                        let rep_data = RepeatData {
                                            in_ref,
                                            in_repsig,
                                            out_ref: refmap.get_sender(
                                                get_ref_id(&Some(out_ref_stream)),
                                                builder,
                                            ),
                                        };
                                        builder.add_child(Repeat::new(rep_data));
                                    }
                                    repeat::OutputRef::OutVal(_) => todo!(),
                                },
                                None => todo!(),
                            }
                        }
                        repeat::InputRef::InVal(in_val_stream) => {
                            let in_val =
                                valmap.get_receiver(get_val_id(&Some(in_val_stream)), builder);

                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(_) => todo!(),
                                    repeat::OutputRef::OutVal(out_val_stream) => {
                                        let rep_data = RepeatData {
                                            in_ref: in_val,
                                            in_repsig,
                                            out_ref: valmap.get_sender(
                                                get_val_id(&Some(out_val_stream)),
                                                builder,
                                            ),
                                        };
                                        builder.add_child(Repeat::new(rep_data));
                                    }
                                },
                                None => todo!(),
                            }
                        }
                    },
                    None => todo!(),
                }
            }
            Op::Repeatsig(op) => {
                let in_crd_id = get_crd_id(&op.input_crd);
                let repsig_data = RepSigGenData {
                    input: crdmap.get_receiver(in_crd_id, builder),
                    out_repsig: repmap.get_sender(get_repsig_id(&op.output_rep_sig), builder),
                };
                builder.add_child(RepeatSigGen::new(repsig_data));
            }
            Op::Alu(op) => {
                let mut in_val_ids = match op.conn.as_ref().unwrap() {
                    alu::Conn::Vals(val) => val
                        .inputs
                        .iter()
                        .map(|input_val| get_val_id(&Some(input_val.clone()))),
                    alu::Conn::Crds(_) => todo!(),
                };
                let out_val_id = match op.conn.as_ref().unwrap() {
                    alu::Conn::Vals(val) => get_val_id(&val.output),
                    alu::Conn::Crds(_) => todo!(),
                };
                assert!(in_val_ids.len() >= 1);
                let out_val_sender = valmap.get_sender(out_val_id, builder);
                if in_val_ids.len() == 2 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    let val_receiver2 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    // Use block_size^2 for latency on binary ops
                    let latency = block_size * block_size;
                    let ii = if op.stages[0].op() == alu::AluOp::Mul { block_size } else { 1 };
                    let binary_func = match op.stages[0].op() {
                        alu::AluOp::Add => |val1: f32, val2: f32| -> f32 { val1 + val2 },
                        alu::AluOp::Sub => |val1: f32, val2: f32| -> f32 { val1 - val2 },
                        alu::AluOp::Mul => |val1: f32, val2: f32| -> f32 { val1 * val2 },
                        alu::AluOp::Div => |val1: f32, val2: f32| -> f32 { val1 / val2 },
                        alu::AluOp::Elemmul => |val1: f32, val2: f32| -> f32 { val1 * val2 },
                        _ => todo!(),
                    };
                    builder.add_child(Binary::new(
                        val_receiver1,
                        val_receiver2,
                        out_val_sender,
                        binary_func,
                        block_size.try_into().unwrap(),
                        latency.try_into().unwrap(),
                        ii.try_into().unwrap(),
                    ));
                } else if in_val_ids.len() == 1 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    match op.stages[0].op() {
                        alu::AluOp::Exp => {
                            let unary_func = |val: f32| -> f32 { val.exp() };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size,
                            ));
                        }
                        alu::AluOp::Sin => {
                            let unary_func = |val: f32| -> f32 { val.sin() };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size,
                            ));
                        }
                        alu::AluOp::Cos => {
                            let unary_func = |val: f32| -> f32 { val.cos() };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size,
                            ));
                        }
                        alu::AluOp::Max => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: f32| -> f32 { val.max(scalar) };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size,
                            ));
                        }
                        alu::AluOp::Scalaradd => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: f32| -> f32 { val + scalar };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size,
                            ));
                        }
                        alu::AluOp::Scalarmul => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: f32| -> f32 { val * scalar };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size,
                            ));
                        }
                        alu::AluOp::Scalardiv => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: f32| -> f32 { val / scalar };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size,
                            ));
                        }
                        alu::AluOp::Rsqrt => {
                            let unary_func = |val: f32| -> f32 { 1.0 / val.sqrt() };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size,
                            ));
                        }
                        alu::AluOp::Sigmoid => {
                            let unary_func = |val: f32| -> f32 { 1.0 / (1.0 + f32::exp(-val)) };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size,
                            ));
                        }
                        _ => {
                            panic!("Should not reach binary op cases")
                        }
                    }
                }
            }
            Op::Reduce(op) => {
                let in_val_id = get_val_id(&op.input_val);
                // Use block_size for Reduce timing via const N parameter
                // For now, use N=1 but note that timing is encoded in the template
                match op.reduce_type() {
                    reduce::Type::Add => {
                        let reduce_data = ReduceData::<VT, ST, 1> {
                            in_val: valmap.get_receiver(in_val_id, builder),
                            out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                            sum: false,
                        };
                        builder.add_child(Reduce::<VT, ST, 1>::new(reduce_data));
                    }
                    reduce::Type::Max => {
                        let max_reduce_data = MaxReduceData::<VT, ST> {
                            in_val: valmap.get_receiver(in_val_id, builder),
                            out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                        };
                        builder.add_child(MaxReduce::new(max_reduce_data, f32::MIN));
                    }
                    reduce::Type::Addsum => {
                        let reduce_data = ReduceData::<VT, ST, 1> {
                            in_val: valmap.get_receiver(in_val_id, builder),
                            out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                            sum: true,
                        };
                        builder.add_child(Reduce::<VT, ST, 1>::new(reduce_data));
                    }
                }
            }
            Op::CoordHold(op) => {
                let in_inner_crd = get_crd_id(&op.input_inner_crd);
                let in_outer_crd = get_crd_id(&op.input_outer_crd);

                let crd_hold_data = CrdManagerData {
                    in_crd_inner: crdmap.get_receiver(in_inner_crd, builder),
                    in_crd_outer: crdmap.get_receiver(in_outer_crd, builder),
                    out_crd_inner: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                    out_crd_outer: crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder),
                };
                builder.add_child(CrdHold::new(crd_hold_data));
            }
            Op::CoordDrop(op) => {
                let in_inner_crd = get_crd_id(&op.input_inner_crd);
                let in_outer_crd = get_crd_id(&op.input_outer_crd);

                let crd_drop_data = CrdManagerData {
                    in_crd_inner: crdmap.get_receiver(in_inner_crd, builder),
                    in_crd_outer: crdmap.get_receiver(in_outer_crd, builder),
                    out_crd_inner: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                    out_crd_outer: crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder),
                };
                builder.add_child(CrdDrop::new(crd_drop_data));
            }
            Op::Array(op) => {
                let in_ref_id = get_ref_id(&op.input_ref);
                let array_data = ArrayData {
                    in_ref: refmap.get_receiver(in_ref_id, builder),
                    out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                    block_size,  // Use specified block size for timing
                };
                let val_filename = base_path.join(format!("tensor_{}_mode_vals", op.tensor));
                let vals = read_inputs(&val_filename);
                let mut arr = Array::new(array_data, vals);
                if let Some(hbm) = hbm_ctx_opt.as_mut() {
                    let (snd, rcv) = builder.unbounded();
                    let (rsnd, rrcv) = builder.unbounded();
                    hbm.add_reader(ReadBundle { addr: rcv, resp: rsnd });
                    arr.enable_hbm_reads(snd, rrcv, 0, 4 * block_size as u64);
                }
                builder.add_child(arr);
            }
            Op::Spacc(op) => {
                let in_inner_crd = get_crd_id(&op.input_inner_crd);
                let in_outer_crd = op.input_outer_crds[0].try_conv();
                let in_val_id = get_val_id(&op.input_val);

                let spacc_data = Spacc1Data {
                    in_crd_inner: crdmap.get_receiver(in_inner_crd, builder),
                    in_crd_outer: crdmap.get_receiver(in_outer_crd, builder),
                    in_val: valmap.get_receiver(in_val_id, builder),
                    out_crd_inner: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                    out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                };
                builder.add_child(Spacc1::new(spacc_data));
            }
            Op::ValWrite(op) => {
                let in_val_id = get_val_id(&op.input_val);
                let val_receiver = valmap.get_receiver(in_val_id, builder);
                builder.add_child(ValsWrScan::new(val_receiver));
            }
            Op::CoordMask(_) => unimplemented!("SAMML can't output coord mask op yet"),
            operation::Op::Func(_) => todo!(),
            Op::Root(op) => {
                let out_ref_id = get_ref_id(&op.output_ref);

                let root_sender = refmap.get_sender(out_ref_id, builder);
                builder.add_child(GeneratorContext::new(
                    || token_vec!(u32; u32; 0, "D").into_iter(),
                    root_sender,
                ));
            }
            Op::Fork(op) => match op.conn.as_ref().unwrap() {
                fork::Conn::Crd(in_crd) => {
                    let in_crd_id = in_crd.input.try_conv();
                    let out_crd_ids = in_crd.outputs.iter().map(|id| id.try_conv());
                    let receiver = crdmap.get_receiver(in_crd_id, builder);
                    let mut broadcast = Scatter::new(receiver);
                    out_crd_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(crdmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                fork::Conn::Ref(in_ref) => {
                    let in_ref_id = in_ref.input.try_conv();
                    let out_ref_ids = in_ref.outputs.iter().map(|id| id.try_conv());
                    let receiver = refmap.get_receiver(in_ref_id, builder);
                    let mut scatter = Scatter::new(receiver);
                    out_ref_ids
                        .into_iter()
                        .for_each(|id| scatter.add_target(refmap.get_sender(id, builder)));
                    builder.add_child(scatter);
                }
                fork::Conn::Val(in_val) => {
                    let in_val_id = in_val.input.try_conv();
                    let out_val_ids = in_val.outputs.iter().map(|id| id.try_conv());
                    let receiver = valmap.get_receiver(in_val_id, builder);
                    let mut broadcast = Scatter::new(receiver);
                    out_val_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(valmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                fork::Conn::Repsig(_) => {
                    panic!("Attempting to fork a repsig");
                }
            },
            Op::Join(op) => match op.conn.as_ref().unwrap() {
                join::Conn::Crd(in_crd) => {
                    let in_crd_id = in_crd.output.try_conv();
                    let sender = crdmap.get_sender(in_crd_id, builder);
                    let out_crd_ids = in_crd.inputs.iter().map(|id| id.try_conv());
                    let mut gather = Gather::new(sender);
                    out_crd_ids
                        .into_iter()
                        .for_each(|id| gather.add_target(crdmap.get_receiver(id, builder)));
                    builder.add_child(gather);
                }
                join::Conn::Ref(in_ref) => {
                    let in_ref_id = in_ref.output.try_conv();
                    let out_ref_ids = in_ref.inputs.iter().map(|id| id.try_conv());
                    let sender = refmap.get_sender(in_ref_id, builder);
                    let mut gather = Gather::new(sender);
                    out_ref_ids
                        .into_iter()
                        .for_each(|id| gather.add_target(refmap.get_receiver(id, builder)));
                    builder.add_child(gather);
                }
                join::Conn::Val(in_val) => {
                    let in_val_id = in_val.output.try_conv();
                    let out_val_ids = in_val.inputs.iter().map(|id| id.try_conv());
                    let sender = valmap.get_sender(in_val_id, builder);
                    let mut gather = Gather::new(sender);
                    out_val_ids
                        .into_iter()
                        .for_each(|id| gather.add_target(valmap.get_receiver(id, builder)));
                    builder.add_child(gather);
                }
                join::Conn::Repsig(_) => {
                    panic!("Attempting to join repsig");
                }
            },
            Op::Locate(op) => {
                let in_ref_id = get_ref_id(&op.input_ref);
                let in_crd_id = get_crd_id(&op.input_crd);
                let out_ref1_id = get_ref_id(&op.output_ref1);
                let out_ref2_id = get_ref_id(&op.output_ref2);
                let out_crd_id = get_crd_id(&op.output_crd);
                let locate = IterateLocate::new(
                    refmap.get_receiver(in_ref_id, builder),
                    crdmap.get_receiver(in_crd_id, builder),
                    refmap.get_sender(out_ref1_id, builder),
                    refmap.get_sender(out_ref2_id, builder),
                    crdmap.get_sender(out_crd_id, builder),
                );
                builder.add_child(locate);
            }
            _ => todo!(),
        }
    }
    if let Some(hbm) = hbm_ctx_opt {
        builder.add_child(hbm);
    }
}

// ============================================================================
// TRUE Block Sparse Mode Functions
// ============================================================================
// These functions use actual NxN dense Tensor blocks as values (BCSR format).
// Each coordinate in the sparse structure maps to a dense NxN block.

use super::templates::primitive::FinalReduce;

/// Parse proto graph with TRUE block size 16 (16x16 dense blocks)
pub fn parse_proto_true_block16<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
) -> ProgramBuilder<'a> {
    let mut builder = ProgramBuilder::default();
    build_from_proto_block16(
        comal_graph,
        base_path,
        sam_options,
        &mut builder,
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
    );
    builder
}

/// Parse proto graph with TRUE block size 32 (32x32 dense blocks)
pub fn parse_proto_true_block32<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
) -> ProgramBuilder<'a> {
    let mut builder = ProgramBuilder::default();
    build_from_proto_block32(
        comal_graph,
        base_path,
        sam_options,
        &mut builder,
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
    );
    builder
}

/// Parse proto graph with TRUE block size 64 (64x64 dense blocks)
pub fn parse_proto_true_block64<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
) -> ProgramBuilder<'a> {
    let mut builder = ProgramBuilder::default();
    build_from_proto_block64(
        comal_graph,
        base_path,
        sam_options,
        &mut builder,
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
        &mut Default::default(),
    );
    builder
}

/// Build function for TRUE block size 16 (16x16 dense blocks as values)
#[allow(clippy::too_many_arguments)]
pub fn build_from_proto_block16<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
    builder: &mut ProgramBuilder<'a>,
    refmap: &mut Channels<'a, Token<CT, ST>>,
    crdmap: &mut Channels<'a, Token<CT, ST>>,
    valmap: &mut Channels<'a, Token<VT16, ST>>,
    repmap: &mut Channels<'a, Repsiggen>,
) {
    const N: usize = 16;

    for operation in comal_graph.graph.unwrap().operators {
        match operation.op.expect("Error processing") {
            Op::Broadcast(op) => match op.conn.as_ref().unwrap() {
                broadcast::Conn::Crd(in_crd) => {
                    let in_crd_id = in_crd.input.try_conv();
                    let out_crd_ids = in_crd.outputs.iter().map(|id| id.try_conv());
                    let receiver = crdmap.get_receiver(in_crd_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_crd_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(crdmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Ref(in_ref) => {
                    let in_ref_id = in_ref.input.try_conv();
                    let out_ref_ids = in_ref.outputs.iter().map(|id| id.try_conv());
                    let receiver = refmap.get_receiver(in_ref_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_ref_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(refmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Val(in_val) => {
                    let in_val_id = in_val.input.try_conv();
                    let out_val_ids = in_val.outputs.iter().map(|id| id.try_conv());
                    let receiver = valmap.get_receiver(in_val_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_val_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(valmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Repsig(in_repsig) => {
                    let in_repsig_id = in_repsig.input.try_conv();
                    let out_repsig_ids = in_repsig.outputs.iter().map(|id| id.try_conv());
                    let receiver = repmap.get_receiver(in_repsig_id, builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    out_repsig_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(repmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
            },
            Op::Joiner(op) => {
                // Block sparse joiner - supports both ref and val streams
                let mut in_crds = Vec::new();
                let mut in_refs: Vec<Box<dyn RecvAdapter<Token<CT, ST>> + Send + Sync>> = Vec::new();
                let mut out_refs: Vec<Box<dyn SendAdapter<Token<CT, ST>> + Send + Sync>> = Vec::new();
                op.input_pairs.iter().for_each(|pair| {
                    let pair_crd = crdmap.get_receiver(get_crd_id(&pair.crd), builder);
                    match pair.in_ref.clone().unwrap().stream.as_ref().unwrap() {
                        joiner::payload::Stream::RefStream(ref_stream) => {
                            in_refs.push(Box::new(
                                refmap.get_receiver(get_ref_id(&Some(ref_stream.clone())), builder),
                            ));
                        }
                        joiner::payload::Stream::ValStream(val_stream) => {
                            in_refs.push(Box::new(
                                valmap.get_receiver(get_val_id(&Some(val_stream.clone())), builder),
                            ));
                        }
                    }
                    in_crds.push(pair_crd);
                });
                op.output_refs.iter().for_each(|output_ref| {
                    match output_ref.stream.as_ref().unwrap() {
                        joiner::payload::Stream::RefStream(ref_stream) => out_refs.push(Box::new(
                            refmap.get_sender(get_ref_id(&Some(ref_stream.clone())), builder),
                        )),
                        joiner::payload::Stream::ValStream(val_stream) => out_refs.push(Box::new(
                            valmap.get_sender(get_val_id(&Some(val_stream.clone())), builder),
                        )),
                    }
                });
                let joiner_data = NJoinerData {
                    in_crds,
                    in_refs,
                    out_refs,
                    out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder),
                };

                if let joiner::Type::Intersect = op.join_type() {
                    builder.add_child(NIntersect::new(joiner_data))
                } else {
                    builder.add_child(NUnion::new(joiner_data))
                };
            }
            Op::FiberLookup(op) => {
                let in_ref = refmap.get_receiver(get_ref_id(&op.input_ref), builder);

                let f_data = RdScanData {
                    in_ref,
                    out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder),
                    out_ref: refmap.get_sender(get_ref_id(&op.output_ref), builder),
                };
                if op.format == "compressed" {
                    let seg_filename =
                        base_path.join(format!("tensor_{}_mode_{}_seg", op.tensor, op.mode));
                    let crd_filename =
                        base_path.join(format!("tensor_{}_mode_{}_crd", op.tensor, op.mode));
                    let seg = read_inputs(&seg_filename);
                    let crd = read_inputs(&crd_filename);
                    let mut crs = CompressedCrdRdScan::new(f_data, seg, crd);
                    crs.set_timings(sam_options.compressed_read_config);
                    builder.add_child(crs);
                } else {
                    let shape_filename = base_path.join(format!("tensor_{}_mode_shape", op.tensor));
                    let shapes = read_inputs(&shape_filename);
                    let index: usize = op.mode.try_into().unwrap();
                    builder.add_child(UncompressedCrdRdScan::new(f_data, shapes[index.clone()]));
                }
            }
            Op::FiberWrite(op) => {
                let in_crd_id = get_crd_id(&op.input_crd);
                let receiver = crdmap.get_receiver(in_crd_id, builder);
                let wr = CompressedWrScan::new(receiver);
                builder.add_child(wr);
            }
            Op::Repeat(op) => {
                let (out_repsig, in_repsig) = builder.bounded(DEFAULT_CHAN_SIZE);
                match op.input_rep_sig {
                    Some(in_rep) => match in_rep {
                        repeat::InputRepSig::RepRef(rep_ref) => {
                            let in_rep_ref = get_ref_id(&Some(rep_ref));
                            let repsig_data = RepSigGenData {
                                input: refmap.get_receiver(in_rep_ref, builder),
                                out_repsig,
                            };
                            builder.add_child(RepeatSigGen::new(repsig_data));
                        }
                        repeat::InputRepSig::RepVal(rep_val) => {
                            let in_rep_val = get_val_id(&Some(rep_val));
                            let repsig_data = RepSigGenData {
                                input: valmap.get_receiver(in_rep_val, builder),
                                out_repsig,
                            };
                            builder.add_child(RepeatSigGen::new(repsig_data));
                        }
                    },
                    None => todo!(),
                }

                match op.input_ref {
                    Some(input_ref) => match input_ref {
                        repeat::InputRef::InRef(in_ref_stream) => {
                            let in_ref =
                                refmap.get_receiver(get_ref_id(&Some(in_ref_stream)), builder);

                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(out_ref_stream) => {
                                        let rep_data = RepeatData {
                                            in_ref,
                                            in_repsig,
                                            out_ref: refmap.get_sender(
                                                get_ref_id(&Some(out_ref_stream)),
                                                builder,
                                            ),
                                        };
                                        builder.add_child(Repeat::new(rep_data));
                                    }
                                    repeat::OutputRef::OutVal(_) => todo!(),
                                },
                                None => todo!(),
                            }
                        }
                        repeat::InputRef::InVal(in_val_stream) => {
                            let in_val =
                                valmap.get_receiver(get_val_id(&Some(in_val_stream)), builder);

                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(_) => todo!(),
                                    repeat::OutputRef::OutVal(out_val_stream) => {
                                        let rep_data = RepeatData {
                                            in_ref: in_val,
                                            in_repsig,
                                            out_ref: valmap.get_sender(
                                                get_val_id(&Some(out_val_stream)),
                                                builder,
                                            ),
                                        };
                                        builder.add_child(Repeat::new(rep_data));
                                    }
                                },
                                None => todo!(),
                            }
                        }
                    },
                    None => todo!(),
                }
            }
            Op::Repeatsig(op) => {
                let in_crd_id = get_crd_id(&op.input_crd);
                let repsig_data = RepSigGenData {
                    input: crdmap.get_receiver(in_crd_id, builder),
                    out_repsig: repmap.get_sender(get_repsig_id(&op.output_rep_sig), builder),
                };
                builder.add_child(RepeatSigGen::new(repsig_data));
            }
            Op::Alu(op) => {
                let mut in_val_ids = match op.conn.as_ref().unwrap() {
                    alu::Conn::Vals(val) => val
                        .inputs
                        .iter()
                        .map(|input_val| get_val_id(&Some(input_val.clone()))),
                    alu::Conn::Crds(_) => todo!(),
                };
                let out_val_id = match op.conn.as_ref().unwrap() {
                    alu::Conn::Vals(val) => get_val_id(&val.output),
                    alu::Conn::Crds(_) => todo!(),
                };
                assert!(in_val_ids.len() >= 1);
                let out_val_sender = valmap.get_sender(out_val_id, builder);
                if in_val_ids.len() == 2 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    let val_receiver2 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    // Block sparse binary ops with proper timing
                    let (latency, ii, binary_func): (usize, usize, fn(VT16, VT16) -> VT16) = match op.stages[0].op() {
                        alu::AluOp::Mul => (
                            2 * N - 1,  // Matrix multiply latency
                            N,          // Initiation interval
                            |val1: VT16, val2: VT16| -> VT16 { Tensor::new(val1.data.dot(&val2.data)) }
                        ),
                        alu::AluOp::Add => (
                            N * N,      // Element-wise latency
                            1,
                            |val1: VT16, val2: VT16| -> VT16 { val1 + val2 }
                        ),
                        alu::AluOp::Sub => (
                            N * N,
                            1,
                            |val1: VT16, val2: VT16| -> VT16 { val1 - val2 }
                        ),
                        alu::AluOp::Elemmul => (
                            N * N,
                            1,
                            |val1: VT16, val2: VT16| -> VT16 { val1 * val2 }
                        ),
                        _ => todo!("Unsupported binary op for block sparse"),
                    };
                    builder.add_child(Binary::new(
                        val_receiver1,
                        val_receiver2,
                        out_val_sender,
                        binary_func,
                        N.try_into().unwrap(),
                        latency.try_into().unwrap(),
                        ii.try_into().unwrap(),
                    ));
                } else if in_val_ids.len() == 1 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    match op.stages[0].op() {
                        alu::AluOp::Scalarmul => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: VT16| -> VT16 {
                                Tensor::new(val.data.map(|x| x * scalar).to_owned())
                            };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                N,
                            ));
                        }
                        alu::AluOp::Scalaradd => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: VT16| -> VT16 {
                                Tensor::new(val.data.map(|x| x + scalar).to_owned())
                            };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                N,
                            ));
                        }
                        _ => todo!("Unsupported unary op for block sparse"),
                    }
                }
            }
            Op::Reduce(op) => {
                let in_val_id = get_val_id(&op.input_val);
                match op.reduce_type() {
                    reduce::Type::Add => {
                        let reduce_data = ReduceData::<VT16, ST, N> {
                            in_val: valmap.get_receiver(in_val_id, builder),
                            out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                            sum: false,
                        };
                        builder.add_child(Reduce::<VT16, ST, N>::new(reduce_data));
                    }
                    reduce::Type::Max => {
                        let max_reduce_data = MaxReduceData::<VT16, ST> {
                            in_val: valmap.get_receiver(in_val_id, builder),
                            out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                        };
                        builder.add_child(MaxReduce::new(max_reduce_data, VT16::default()));
                    }
                    reduce::Type::Addsum => {
                        let reduce_data = ReduceData::<VT16, ST, N> {
                            in_val: valmap.get_receiver(in_val_id, builder),
                            out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                            sum: true,
                        };
                        builder.add_child(Reduce::<VT16, ST, N>::new(reduce_data));
                    }
                }
            }
            Op::CoordHold(op) => {
                let in_inner_crd = get_crd_id(&op.input_inner_crd);
                let in_outer_crd = get_crd_id(&op.input_outer_crd);

                let crd_hold_data = CrdManagerData {
                    in_crd_inner: crdmap.get_receiver(in_inner_crd, builder),
                    in_crd_outer: crdmap.get_receiver(in_outer_crd, builder),
                    out_crd_inner: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                    out_crd_outer: crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder),
                };
                builder.add_child(CrdHold::new(crd_hold_data));
            }
            Op::CoordDrop(op) => {
                let in_inner_crd = get_crd_id(&op.input_inner_crd);
                let in_outer_crd = get_crd_id(&op.input_outer_crd);

                let crd_drop_data = CrdManagerData {
                    in_crd_inner: crdmap.get_receiver(in_inner_crd, builder),
                    in_crd_outer: crdmap.get_receiver(in_outer_crd, builder),
                    out_crd_inner: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                    out_crd_outer: crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder),
                };
                builder.add_child(CrdDrop::new(crd_drop_data));
            }
            Op::Array(op) => {
                // TRUE block sparse: load NxN blocks using read_inputs_vectorized
                let in_ref_id = get_ref_id(&op.input_ref);
                let array_data = ArrayData {
                    in_ref: refmap.get_receiver(in_ref_id, builder),
                    out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                    block_size: N,
                };
                let val_filename = base_path.join(format!("tensor_{}_mode_vals", op.tensor));
                // Read NxN blocks using the Tensor adapter
                let vals: Vec<VT16> = read_inputs_vectorized(&val_filename, PrimitiveType::<VT16>::new());
                let arr = Array::new(array_data, vals);
                builder.add_child(arr);
            }
            Op::Spacc(op) => {
                let in_inner_crd = get_crd_id(&op.input_inner_crd);
                let in_outer_crd = op.input_outer_crds[0].try_conv();
                let in_val_id = get_val_id(&op.input_val);

                let spacc_data = Spacc1Data {
                    in_crd_inner: crdmap.get_receiver(in_inner_crd, builder),
                    in_crd_outer: crdmap.get_receiver(in_outer_crd, builder),
                    in_val: valmap.get_receiver(in_val_id, builder),
                    out_crd_inner: crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder),
                    out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                };
                builder.add_child(Spacc1::new(spacc_data));
            }
            Op::ValWrite(op) => {
                let in_val_id = get_val_id(&op.input_val);
                let val_receiver = valmap.get_receiver(in_val_id, builder);
                builder.add_child(ValsWrScan::new(val_receiver));
            }
            Op::Root(op) => {
                let out_ref_id = get_ref_id(&op.output_ref);

                let root_sender = refmap.get_sender(out_ref_id, builder);
                builder.add_child(GeneratorContext::new(
                    || token_vec!(u32; u32; 0, "D").into_iter(),
                    root_sender,
                ));
            }
            Op::Fork(op) => match op.conn.as_ref().unwrap() {
                fork::Conn::Crd(in_crd) => {
                    let in_crd_id = in_crd.input.try_conv();
                    let out_crd_ids = in_crd.outputs.iter().map(|id| id.try_conv());
                    let receiver = crdmap.get_receiver(in_crd_id, builder);
                    let mut broadcast = Scatter::new(receiver);
                    out_crd_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(crdmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                fork::Conn::Ref(in_ref) => {
                    let in_ref_id = in_ref.input.try_conv();
                    let out_ref_ids = in_ref.outputs.iter().map(|id| id.try_conv());
                    let receiver = refmap.get_receiver(in_ref_id, builder);
                    let mut scatter = Scatter::new(receiver);
                    out_ref_ids
                        .into_iter()
                        .for_each(|id| scatter.add_target(refmap.get_sender(id, builder)));
                    builder.add_child(scatter);
                }
                fork::Conn::Val(in_val) => {
                    let in_val_id = in_val.input.try_conv();
                    let out_val_ids = in_val.outputs.iter().map(|id| id.try_conv());
                    let receiver = valmap.get_receiver(in_val_id, builder);
                    let mut broadcast = Scatter::new(receiver);
                    out_val_ids
                        .into_iter()
                        .for_each(|id| broadcast.add_target(valmap.get_sender(id, builder)));
                    builder.add_child(broadcast);
                }
                fork::Conn::Repsig(_) => {
                    panic!("Attempting to fork a repsig");
                }
            },
            Op::Join(op) => match op.conn.as_ref().unwrap() {
                join::Conn::Crd(in_crd) => {
                    let in_crd_id = in_crd.output.try_conv();
                    let sender = crdmap.get_sender(in_crd_id, builder);
                    let out_crd_ids = in_crd.inputs.iter().map(|id| id.try_conv());
                    let mut gather = Gather::new(sender);
                    out_crd_ids
                        .into_iter()
                        .for_each(|id| gather.add_target(crdmap.get_receiver(id, builder)));
                    builder.add_child(gather);
                }
                join::Conn::Ref(in_ref) => {
                    let in_ref_id = in_ref.output.try_conv();
                    let out_ref_ids = in_ref.inputs.iter().map(|id| id.try_conv());
                    let sender = refmap.get_sender(in_ref_id, builder);
                    let mut gather = Gather::new(sender);
                    out_ref_ids
                        .into_iter()
                        .for_each(|id| gather.add_target(refmap.get_receiver(id, builder)));
                    builder.add_child(gather);
                }
                join::Conn::Val(in_val) => {
                    let in_val_id = in_val.output.try_conv();
                    let out_val_ids = in_val.inputs.iter().map(|id| id.try_conv());
                    let sender = valmap.get_sender(in_val_id, builder);
                    let mut gather = Gather::new(sender);
                    out_val_ids
                        .into_iter()
                        .for_each(|id| gather.add_target(valmap.get_receiver(id, builder)));
                    builder.add_child(gather);
                }
                join::Conn::Repsig(_) => {
                    panic!("Attempting to join repsig");
                }
            },
            Op::Locate(op) => {
                let in_ref_id = get_ref_id(&op.input_ref);
                let in_crd_id = get_crd_id(&op.input_crd);
                let out_ref1_id = get_ref_id(&op.output_ref1);
                let out_ref2_id = get_ref_id(&op.output_ref2);
                let out_crd_id = get_crd_id(&op.output_crd);
                let locate = IterateLocate::new(
                    refmap.get_receiver(in_ref_id, builder),
                    crdmap.get_receiver(in_crd_id, builder),
                    refmap.get_sender(out_ref1_id, builder),
                    refmap.get_sender(out_ref2_id, builder),
                    crdmap.get_sender(out_crd_id, builder),
                );
                builder.add_child(locate);
            }
            _ => todo!(),
        }
    }
}

/// Build function for TRUE block size 32 (32x32 dense blocks as values)
#[allow(clippy::too_many_arguments)]
pub fn build_from_proto_block32<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
    builder: &mut ProgramBuilder<'a>,
    refmap: &mut Channels<'a, Token<CT, ST>>,
    crdmap: &mut Channels<'a, Token<CT, ST>>,
    valmap: &mut Channels<'a, Token<VT32, ST>>,
    repmap: &mut Channels<'a, Repsiggen>,
) {
    const N: usize = 32;

    for operation in comal_graph.graph.unwrap().operators {
        match operation.op.expect("Error processing") {
            Op::Broadcast(op) => match op.conn.as_ref().unwrap() {
                broadcast::Conn::Crd(in_crd) => {
                    let receiver = crdmap.get_receiver(in_crd.input.try_conv(), builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    in_crd.outputs.iter().for_each(|id| broadcast.add_target(crdmap.get_sender(id.try_conv(), builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Ref(in_ref) => {
                    let receiver = refmap.get_receiver(in_ref.input.try_conv(), builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    in_ref.outputs.iter().for_each(|id| broadcast.add_target(refmap.get_sender(id.try_conv(), builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Val(in_val) => {
                    let receiver = valmap.get_receiver(in_val.input.try_conv(), builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    in_val.outputs.iter().for_each(|id| broadcast.add_target(valmap.get_sender(id.try_conv(), builder)));
                    builder.add_child(broadcast);
                }
                broadcast::Conn::Repsig(in_repsig) => {
                    let receiver = repmap.get_receiver(in_repsig.input.try_conv(), builder);
                    let mut broadcast = BroadcastContext::new(receiver);
                    in_repsig.outputs.iter().for_each(|id| broadcast.add_target(repmap.get_sender(id.try_conv(), builder)));
                    builder.add_child(broadcast);
                }
            },
            Op::Joiner(op) => {
                let mut in_crds = Vec::new();
                let mut in_refs: Vec<Box<dyn RecvAdapter<Token<CT, ST>> + Send + Sync>> = Vec::new();
                let mut out_refs: Vec<Box<dyn SendAdapter<Token<CT, ST>> + Send + Sync>> = Vec::new();
                op.input_pairs.iter().for_each(|pair| {
                    in_crds.push(crdmap.get_receiver(get_crd_id(&pair.crd), builder));
                    match pair.in_ref.clone().unwrap().stream.as_ref().unwrap() {
                        joiner::payload::Stream::RefStream(ref_stream) => {
                            in_refs.push(Box::new(refmap.get_receiver(get_ref_id(&Some(ref_stream.clone())), builder)));
                        }
                        joiner::payload::Stream::ValStream(val_stream) => {
                            in_refs.push(Box::new(valmap.get_receiver(get_val_id(&Some(val_stream.clone())), builder)));
                        }
                    }
                });
                op.output_refs.iter().for_each(|output_ref| {
                    match output_ref.stream.as_ref().unwrap() {
                        joiner::payload::Stream::RefStream(ref_stream) => {
                            out_refs.push(Box::new(refmap.get_sender(get_ref_id(&Some(ref_stream.clone())), builder)));
                        }
                        joiner::payload::Stream::ValStream(val_stream) => {
                            out_refs.push(Box::new(valmap.get_sender(get_val_id(&Some(val_stream.clone())), builder)));
                        }
                    }
                });
                let joiner_data = NJoinerData { in_crds, in_refs, out_refs, out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder) };
                if let joiner::Type::Intersect = op.join_type() { builder.add_child(NIntersect::new(joiner_data)) } else { builder.add_child(NUnion::new(joiner_data)) };
            }
            Op::FiberLookup(op) => {
                let f_data = RdScanData {
                    in_ref: refmap.get_receiver(get_ref_id(&op.input_ref), builder),
                    out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder),
                    out_ref: refmap.get_sender(get_ref_id(&op.output_ref), builder),
                };
                if op.format == "compressed" {
                    let seg = read_inputs(&base_path.join(format!("tensor_{}_mode_{}_seg", op.tensor, op.mode)));
                    let crd = read_inputs(&base_path.join(format!("tensor_{}_mode_{}_crd", op.tensor, op.mode)));
                    let mut crs = CompressedCrdRdScan::new(f_data, seg, crd);
                    crs.set_timings(sam_options.compressed_read_config);
                    builder.add_child(crs);
                } else {
                    let shapes = read_inputs(&base_path.join(format!("tensor_{}_mode_shape", op.tensor)));
                    builder.add_child(UncompressedCrdRdScan::new(f_data, shapes[op.mode as usize]));
                }
            }
            Op::FiberWrite(op) => {
                let receiver = crdmap.get_receiver(get_crd_id(&op.input_crd), builder);
                builder.add_child(CompressedWrScan::new(receiver));
            }
            Op::Repeat(op) => {
                let (out_repsig, in_repsig) = builder.bounded(DEFAULT_CHAN_SIZE);
                match op.input_rep_sig {
                    Some(in_rep) => match in_rep {
                        repeat::InputRepSig::RepRef(rep_ref) => {
                            let input = refmap.get_receiver(get_ref_id(&Some(rep_ref)), builder);
                            builder.add_child(RepeatSigGen::new(RepSigGenData { input, out_repsig }));
                        }
                        repeat::InputRepSig::RepVal(rep_val) => {
                            let input = valmap.get_receiver(get_val_id(&Some(rep_val)), builder);
                            builder.add_child(RepeatSigGen::new(RepSigGenData { input, out_repsig }));
                        }
                    },
                    None => todo!(),
                }
                match op.input_ref {
                    Some(input_ref) => match input_ref {
                        repeat::InputRef::InRef(in_ref_stream) => {
                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(out_ref_stream) => {
                                        let in_ref = refmap.get_receiver(get_ref_id(&Some(in_ref_stream)), builder);
                                        let out_ref = refmap.get_sender(get_ref_id(&Some(out_ref_stream)), builder);
                                        builder.add_child(Repeat::new(RepeatData { in_ref, in_repsig, out_ref }));
                                    }
                                    repeat::OutputRef::OutVal(_) => todo!(),
                                },
                                None => todo!(),
                            }
                        }
                        repeat::InputRef::InVal(in_val_stream) => {
                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(_) => todo!(),
                                    repeat::OutputRef::OutVal(out_val_stream) => {
                                        let in_ref = valmap.get_receiver(get_val_id(&Some(in_val_stream)), builder);
                                        let out_ref = valmap.get_sender(get_val_id(&Some(out_val_stream)), builder);
                                        builder.add_child(Repeat::new(RepeatData { in_ref, in_repsig, out_ref }));
                                    }
                                },
                                None => todo!(),
                            }
                        }
                    },
                    None => todo!(),
                }
            }
            Op::Repeatsig(op) => {
                let input = crdmap.get_receiver(get_crd_id(&op.input_crd), builder);
                let out_repsig = repmap.get_sender(get_repsig_id(&op.output_rep_sig), builder);
                builder.add_child(RepeatSigGen::new(RepSigGenData { input, out_repsig }));
            }
            Op::Alu(op) => {
                let mut in_val_ids = match op.conn.as_ref().unwrap() { alu::Conn::Vals(val) => val.inputs.iter().map(|input_val| get_val_id(&Some(input_val.clone()))), alu::Conn::Crds(_) => todo!() };
                let out_val_id = match op.conn.as_ref().unwrap() { alu::Conn::Vals(val) => get_val_id(&val.output), alu::Conn::Crds(_) => todo!() };
                let out_val_sender = valmap.get_sender(out_val_id, builder);
                if in_val_ids.len() == 2 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    let val_receiver2 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    let (latency, ii, binary_func): (usize, usize, fn(VT32, VT32) -> VT32) = match op.stages[0].op() {
                        alu::AluOp::Mul => (2 * N - 1, N, |val1: VT32, val2: VT32| -> VT32 { Tensor::new(val1.data.dot(&val2.data)) }),
                        alu::AluOp::Add => (N * N, 1, |val1: VT32, val2: VT32| -> VT32 { val1 + val2 }),
                        alu::AluOp::Sub => (N * N, 1, |val1: VT32, val2: VT32| -> VT32 { val1 - val2 }),
                        alu::AluOp::Elemmul => (N * N, 1, |val1: VT32, val2: VT32| -> VT32 { val1 * val2 }),
                        _ => todo!("Unsupported binary op for block sparse"),
                    };
                    builder.add_child(Binary::new(val_receiver1, val_receiver2, out_val_sender, binary_func, N.try_into().unwrap(), latency.try_into().unwrap(), ii.try_into().unwrap()));
                } else if in_val_ids.len() == 1 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    match op.stages[0].op() {
                        alu::AluOp::Scalarmul => { let scalar: f32 = op.scalar as f32; builder.add_child(Unary::new(val_receiver1, out_val_sender, move |val: VT32| -> VT32 { Tensor::new(val.data.map(|x| x * scalar).to_owned()) }, N)); }
                        alu::AluOp::Scalaradd => { let scalar: f32 = op.scalar as f32; builder.add_child(Unary::new(val_receiver1, out_val_sender, move |val: VT32| -> VT32 { Tensor::new(val.data.map(|x| x + scalar).to_owned()) }, N)); }
                        _ => todo!("Unsupported unary op for block sparse"),
                    }
                }
            }
            Op::Reduce(op) => {
                let in_val_id = get_val_id(&op.input_val);
                match op.reduce_type() {
                    reduce::Type::Add => {
                        let in_val = valmap.get_receiver(in_val_id, builder);
                        let out_val = valmap.get_sender(get_val_id(&op.output_val), builder);
                        builder.add_child(Reduce::<VT32, ST, N>::new(ReduceData { in_val, out_val, sum: false }));
                    }
                    reduce::Type::Max => {
                        let in_val = valmap.get_receiver(in_val_id, builder);
                        let out_val = valmap.get_sender(get_val_id(&op.output_val), builder);
                        builder.add_child(MaxReduce::new(MaxReduceData { in_val, out_val }, VT32::default()));
                    }
                    reduce::Type::Addsum => {
                        let in_val = valmap.get_receiver(in_val_id, builder);
                        let out_val = valmap.get_sender(get_val_id(&op.output_val), builder);
                        builder.add_child(Reduce::<VT32, ST, N>::new(ReduceData { in_val, out_val, sum: true }));
                    }
                }
            }
            Op::CoordHold(op) => {
                let in_crd_inner = crdmap.get_receiver(get_crd_id(&op.input_inner_crd), builder);
                let in_crd_outer = crdmap.get_receiver(get_crd_id(&op.input_outer_crd), builder);
                let out_crd_inner = crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder);
                let out_crd_outer = crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder);
                builder.add_child(CrdHold::new(CrdManagerData { in_crd_inner, in_crd_outer, out_crd_inner, out_crd_outer }));
            }
            Op::CoordDrop(op) => {
                let in_crd_inner = crdmap.get_receiver(get_crd_id(&op.input_inner_crd), builder);
                let in_crd_outer = crdmap.get_receiver(get_crd_id(&op.input_outer_crd), builder);
                let out_crd_inner = crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder);
                let out_crd_outer = crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder);
                builder.add_child(CrdDrop::new(CrdManagerData { in_crd_inner, in_crd_outer, out_crd_inner, out_crd_outer }));
            }
            Op::Array(op) => {
                let in_ref = refmap.get_receiver(get_ref_id(&op.input_ref), builder);
                let out_val = valmap.get_sender(get_val_id(&op.output_val), builder);
                let array_data = ArrayData { in_ref, out_val, block_size: N };
                let vals: Vec<VT32> = read_inputs_vectorized(&base_path.join(format!("tensor_{}_mode_vals", op.tensor)), PrimitiveType::<VT32>::new());
                builder.add_child(Array::new(array_data, vals));
            }
            Op::Spacc(op) => {
                let in_crd_inner = crdmap.get_receiver(get_crd_id(&op.input_inner_crd), builder);
                let in_crd_outer = crdmap.get_receiver(op.input_outer_crds[0].try_conv(), builder);
                let in_val = valmap.get_receiver(get_val_id(&op.input_val), builder);
                let out_crd_inner = crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder);
                let out_val = valmap.get_sender(get_val_id(&op.output_val), builder);
                builder.add_child(Spacc1::new(Spacc1Data { in_crd_inner, in_crd_outer, in_val, out_crd_inner, out_val }));
            }
            Op::ValWrite(op) => {
                let val_receiver = valmap.get_receiver(get_val_id(&op.input_val), builder);
                builder.add_child(ValsWrScan::new(val_receiver));
            }
            Op::Root(op) => {
                let root_sender = refmap.get_sender(get_ref_id(&op.output_ref), builder);
                builder.add_child(GeneratorContext::new(|| token_vec!(u32; u32; 0, "D").into_iter(), root_sender));
            }
            Op::Fork(op) => match op.conn.as_ref().unwrap() {
                fork::Conn::Crd(in_crd) => {
                    let receiver = crdmap.get_receiver(in_crd.input.try_conv(), builder);
                    let mut broadcast = Scatter::new(receiver);
                    in_crd.outputs.iter().for_each(|id| broadcast.add_target(crdmap.get_sender(id.try_conv(), builder)));
                    builder.add_child(broadcast);
                }
                fork::Conn::Ref(in_ref) => {
                    let receiver = refmap.get_receiver(in_ref.input.try_conv(), builder);
                    let mut scatter = Scatter::new(receiver);
                    in_ref.outputs.iter().for_each(|id| scatter.add_target(refmap.get_sender(id.try_conv(), builder)));
                    builder.add_child(scatter);
                }
                fork::Conn::Val(in_val) => {
                    let receiver = valmap.get_receiver(in_val.input.try_conv(), builder);
                    let mut broadcast = Scatter::new(receiver);
                    in_val.outputs.iter().for_each(|id| broadcast.add_target(valmap.get_sender(id.try_conv(), builder)));
                    builder.add_child(broadcast);
                }
                fork::Conn::Repsig(_) => panic!("fork repsig"),
            },
            Op::Join(op) => match op.conn.as_ref().unwrap() {
                join::Conn::Crd(in_crd) => {
                    let sender = crdmap.get_sender(in_crd.output.try_conv(), builder);
                    let mut gather = Gather::new(sender);
                    in_crd.inputs.iter().for_each(|id| gather.add_target(crdmap.get_receiver(id.try_conv(), builder)));
                    builder.add_child(gather);
                }
                join::Conn::Ref(in_ref) => {
                    let sender = refmap.get_sender(in_ref.output.try_conv(), builder);
                    let mut gather = Gather::new(sender);
                    in_ref.inputs.iter().for_each(|id| gather.add_target(refmap.get_receiver(id.try_conv(), builder)));
                    builder.add_child(gather);
                }
                join::Conn::Val(in_val) => {
                    let sender = valmap.get_sender(in_val.output.try_conv(), builder);
                    let mut gather = Gather::new(sender);
                    in_val.inputs.iter().for_each(|id| gather.add_target(valmap.get_receiver(id.try_conv(), builder)));
                    builder.add_child(gather);
                }
                join::Conn::Repsig(_) => panic!("join repsig"),
            },
            Op::Locate(op) => {
                let in_ref_r = refmap.get_receiver(get_ref_id(&op.input_ref), builder);
                let in_crd_r = crdmap.get_receiver(get_crd_id(&op.input_crd), builder);
                let out_ref1 = refmap.get_sender(get_ref_id(&op.output_ref1), builder);
                let out_ref2 = refmap.get_sender(get_ref_id(&op.output_ref2), builder);
                let out_crd = crdmap.get_sender(get_crd_id(&op.output_crd), builder);
                builder.add_child(IterateLocate::new(in_ref_r, in_crd_r, out_ref1, out_ref2, out_crd));
            }
            _ => todo!(),
        }
    }
}

/// Build function for TRUE block size 64 (64x64 dense blocks as values)
#[allow(clippy::too_many_arguments)]
pub fn build_from_proto_block64<'a>(
    comal_graph: ComalGraph,
    base_path: PathBuf,
    sam_options: SamOptions,
    builder: &mut ProgramBuilder<'a>,
    refmap: &mut Channels<'a, Token<CT, ST>>,
    crdmap: &mut Channels<'a, Token<CT, ST>>,
    valmap: &mut Channels<'a, Token<VT64, ST>>,
    repmap: &mut Channels<'a, Repsiggen>,
) {
    const N: usize = 64;

    for operation in comal_graph.graph.unwrap().operators {
        match operation.op.expect("Error processing") {
            Op::Broadcast(op) => match op.conn.as_ref().unwrap() {
                broadcast::Conn::Crd(in_crd) => { let r = crdmap.get_receiver(in_crd.input.try_conv(), builder); let mut b = BroadcastContext::new(r); in_crd.outputs.iter().for_each(|id| b.add_target(crdmap.get_sender(id.try_conv(), builder))); builder.add_child(b); }
                broadcast::Conn::Ref(in_ref) => { let r = refmap.get_receiver(in_ref.input.try_conv(), builder); let mut b = BroadcastContext::new(r); in_ref.outputs.iter().for_each(|id| b.add_target(refmap.get_sender(id.try_conv(), builder))); builder.add_child(b); }
                broadcast::Conn::Val(in_val) => { let r = valmap.get_receiver(in_val.input.try_conv(), builder); let mut b = BroadcastContext::new(r); in_val.outputs.iter().for_each(|id| b.add_target(valmap.get_sender(id.try_conv(), builder))); builder.add_child(b); }
                broadcast::Conn::Repsig(in_repsig) => { let r = repmap.get_receiver(in_repsig.input.try_conv(), builder); let mut b = BroadcastContext::new(r); in_repsig.outputs.iter().for_each(|id| b.add_target(repmap.get_sender(id.try_conv(), builder))); builder.add_child(b); }
            },
            Op::Joiner(op) => {
                let mut in_crds = Vec::new(); let mut in_refs: Vec<Box<dyn RecvAdapter<Token<CT, ST>> + Send + Sync>> = Vec::new(); let mut out_refs: Vec<Box<dyn SendAdapter<Token<CT, ST>> + Send + Sync>> = Vec::new();
                op.input_pairs.iter().for_each(|pair| { in_crds.push(crdmap.get_receiver(get_crd_id(&pair.crd), builder)); match pair.in_ref.clone().unwrap().stream.as_ref().unwrap() { joiner::payload::Stream::RefStream(ref_stream) => { in_refs.push(Box::new(refmap.get_receiver(get_ref_id(&Some(ref_stream.clone())), builder))); } joiner::payload::Stream::ValStream(val_stream) => { in_refs.push(Box::new(valmap.get_receiver(get_val_id(&Some(val_stream.clone())), builder))); } } });
                op.output_refs.iter().for_each(|output_ref| { match output_ref.stream.as_ref().unwrap() { joiner::payload::Stream::RefStream(ref_stream) => { out_refs.push(Box::new(refmap.get_sender(get_ref_id(&Some(ref_stream.clone())), builder))); } joiner::payload::Stream::ValStream(val_stream) => { out_refs.push(Box::new(valmap.get_sender(get_val_id(&Some(val_stream.clone())), builder))); } } });
                let joiner_data = NJoinerData { in_crds, in_refs, out_refs, out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder) };
                if let joiner::Type::Intersect = op.join_type() { builder.add_child(NIntersect::new(joiner_data)) } else { builder.add_child(NUnion::new(joiner_data)) };
            }
            Op::FiberLookup(op) => {
                let f_data = RdScanData { in_ref: refmap.get_receiver(get_ref_id(&op.input_ref), builder), out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), builder), out_ref: refmap.get_sender(get_ref_id(&op.output_ref), builder) };
                if op.format == "compressed" { let seg = read_inputs(&base_path.join(format!("tensor_{}_mode_{}_seg", op.tensor, op.mode))); let crd = read_inputs(&base_path.join(format!("tensor_{}_mode_{}_crd", op.tensor, op.mode))); let mut crs = CompressedCrdRdScan::new(f_data, seg, crd); crs.set_timings(sam_options.compressed_read_config); builder.add_child(crs); }
                else { let shapes = read_inputs(&base_path.join(format!("tensor_{}_mode_shape", op.tensor))); builder.add_child(UncompressedCrdRdScan::new(f_data, shapes[op.mode as usize])); }
            }
            Op::FiberWrite(op) => {
                let receiver = crdmap.get_receiver(get_crd_id(&op.input_crd), builder);
                builder.add_child(CompressedWrScan::new(receiver));
            }
            Op::Repeat(op) => {
                let (out_repsig, in_repsig) = builder.bounded(DEFAULT_CHAN_SIZE);
                match op.input_rep_sig {
                    Some(in_rep) => match in_rep {
                        repeat::InputRepSig::RepRef(rep_ref) => {
                            let input = refmap.get_receiver(get_ref_id(&Some(rep_ref)), builder);
                            builder.add_child(RepeatSigGen::new(RepSigGenData { input, out_repsig }));
                        }
                        repeat::InputRepSig::RepVal(rep_val) => {
                            let input = valmap.get_receiver(get_val_id(&Some(rep_val)), builder);
                            builder.add_child(RepeatSigGen::new(RepSigGenData { input, out_repsig }));
                        }
                    },
                    None => todo!()
                }
                match op.input_ref {
                    Some(input_ref) => match input_ref {
                        repeat::InputRef::InRef(in_ref_stream) => {
                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(out_ref_stream) => {
                                        let in_ref = refmap.get_receiver(get_ref_id(&Some(in_ref_stream)), builder);
                                        let out_ref = refmap.get_sender(get_ref_id(&Some(out_ref_stream)), builder);
                                        builder.add_child(Repeat::new(RepeatData { in_ref, in_repsig, out_ref }));
                                    }
                                    repeat::OutputRef::OutVal(_) => todo!()
                                },
                                None => todo!()
                            }
                        }
                        repeat::InputRef::InVal(in_val_stream) => {
                            match op.output_ref {
                                Some(out_ref) => match out_ref {
                                    repeat::OutputRef::OutRef(_) => todo!(),
                                    repeat::OutputRef::OutVal(out_val_stream) => {
                                        let in_ref = valmap.get_receiver(get_val_id(&Some(in_val_stream)), builder);
                                        let out_ref = valmap.get_sender(get_val_id(&Some(out_val_stream)), builder);
                                        builder.add_child(Repeat::new(RepeatData { in_ref, in_repsig, out_ref }));
                                    }
                                },
                                None => todo!()
                            }
                        }
                    },
                    None => todo!()
                }
            }
            Op::Repeatsig(op) => {
                let input = crdmap.get_receiver(get_crd_id(&op.input_crd), builder);
                let out_repsig = repmap.get_sender(get_repsig_id(&op.output_rep_sig), builder);
                builder.add_child(RepeatSigGen::new(RepSigGenData { input, out_repsig }));
            }
            Op::Alu(op) => {
                let mut in_val_ids = match op.conn.as_ref().unwrap() { alu::Conn::Vals(val) => val.inputs.iter().map(|input_val| get_val_id(&Some(input_val.clone()))), alu::Conn::Crds(_) => todo!() };
                let out_val_id = match op.conn.as_ref().unwrap() { alu::Conn::Vals(val) => get_val_id(&val.output), alu::Conn::Crds(_) => todo!() };
                let out_val_sender = valmap.get_sender(out_val_id, builder);
                if in_val_ids.len() == 2 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    let val_receiver2 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    let (latency, ii, binary_func): (usize, usize, fn(VT64, VT64) -> VT64) = match op.stages[0].op() {
                        alu::AluOp::Mul => (2 * N - 1, N, |val1: VT64, val2: VT64| -> VT64 { Tensor::new(val1.data.dot(&val2.data)) }),
                        alu::AluOp::Add => (N * N, 1, |val1: VT64, val2: VT64| -> VT64 { val1 + val2 }),
                        alu::AluOp::Sub => (N * N, 1, |val1: VT64, val2: VT64| -> VT64 { val1 - val2 }),
                        alu::AluOp::Elemmul => (N * N, 1, |val1: VT64, val2: VT64| -> VT64 { val1 * val2 }),
                        _ => todo!("Unsupported binary op for block sparse"),
                    };
                    builder.add_child(Binary::new(val_receiver1, val_receiver2, out_val_sender, binary_func, N.try_into().unwrap(), latency.try_into().unwrap(), ii.try_into().unwrap()));
                } else if in_val_ids.len() == 1 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    match op.stages[0].op() {
                        alu::AluOp::Scalarmul => { let scalar: f32 = op.scalar as f32; builder.add_child(Unary::new(val_receiver1, out_val_sender, move |val: VT64| -> VT64 { Tensor::new(val.data.map(|x| x * scalar).to_owned()) }, N)); }
                        alu::AluOp::Scalaradd => { let scalar: f32 = op.scalar as f32; builder.add_child(Unary::new(val_receiver1, out_val_sender, move |val: VT64| -> VT64 { Tensor::new(val.data.map(|x| x + scalar).to_owned()) }, N)); }
                        _ => todo!("Unsupported unary op for block sparse"),
                    }
                }
            }
            Op::Reduce(op) => {
                let in_val_id = get_val_id(&op.input_val);
                let in_val = valmap.get_receiver(in_val_id, builder);
                let out_val = valmap.get_sender(get_val_id(&op.output_val), builder);
                match op.reduce_type() {
                    reduce::Type::Add => { builder.add_child(Reduce::<VT64, ST, N>::new(ReduceData { in_val, out_val, sum: false })); }
                    reduce::Type::Max => { builder.add_child(MaxReduce::new(MaxReduceData { in_val, out_val }, VT64::default())); }
                    reduce::Type::Addsum => { builder.add_child(Reduce::<VT64, ST, N>::new(ReduceData { in_val, out_val, sum: true })); }
                }
            }
            Op::CoordHold(op) => {
                let in_crd_inner = crdmap.get_receiver(get_crd_id(&op.input_inner_crd), builder);
                let in_crd_outer = crdmap.get_receiver(get_crd_id(&op.input_outer_crd), builder);
                let out_crd_inner = crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder);
                let out_crd_outer = crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder);
                builder.add_child(CrdHold::new(CrdManagerData { in_crd_inner, in_crd_outer, out_crd_inner, out_crd_outer }));
            }
            Op::CoordDrop(op) => {
                let in_crd_inner = crdmap.get_receiver(get_crd_id(&op.input_inner_crd), builder);
                let in_crd_outer = crdmap.get_receiver(get_crd_id(&op.input_outer_crd), builder);
                let out_crd_inner = crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder);
                let out_crd_outer = crdmap.get_sender(get_crd_id(&op.output_outer_crd), builder);
                builder.add_child(CrdDrop::new(CrdManagerData { in_crd_inner, in_crd_outer, out_crd_inner, out_crd_outer }));
            }
            Op::Array(op) => {
                let array_data = ArrayData { in_ref: refmap.get_receiver(get_ref_id(&op.input_ref), builder), out_val: valmap.get_sender(get_val_id(&op.output_val), builder), block_size: N };
                let vals: Vec<VT64> = read_inputs_vectorized(&base_path.join(format!("tensor_{}_mode_vals", op.tensor)), PrimitiveType::<VT64>::new());
                builder.add_child(Array::new(array_data, vals));
            }
            Op::Spacc(op) => {
                let in_crd_inner = crdmap.get_receiver(get_crd_id(&op.input_inner_crd), builder);
                let in_crd_outer = crdmap.get_receiver(op.input_outer_crds[0].try_conv(), builder);
                let in_val = valmap.get_receiver(get_val_id(&op.input_val), builder);
                let out_crd_inner = crdmap.get_sender(get_crd_id(&op.output_inner_crd), builder);
                let out_val = valmap.get_sender(get_val_id(&op.output_val), builder);
                builder.add_child(Spacc1::new(Spacc1Data { in_crd_inner, in_crd_outer, in_val, out_crd_inner, out_val }));
            }
            Op::ValWrite(op) => {
                let receiver = valmap.get_receiver(get_val_id(&op.input_val), builder);
                builder.add_child(ValsWrScan::new(receiver));
            }
            Op::Root(op) => {
                let sender = refmap.get_sender(get_ref_id(&op.output_ref), builder);
                builder.add_child(GeneratorContext::new(|| token_vec!(u32; u32; 0, "D").into_iter(), sender));
            }
            Op::Fork(op) => match op.conn.as_ref().unwrap() {
                fork::Conn::Crd(in_crd) => { let mut b = Scatter::new(crdmap.get_receiver(in_crd.input.try_conv(), builder)); in_crd.outputs.iter().for_each(|id| b.add_target(crdmap.get_sender(id.try_conv(), builder))); builder.add_child(b); }
                fork::Conn::Ref(in_ref) => { let mut s = Scatter::new(refmap.get_receiver(in_ref.input.try_conv(), builder)); in_ref.outputs.iter().for_each(|id| s.add_target(refmap.get_sender(id.try_conv(), builder))); builder.add_child(s); }
                fork::Conn::Val(in_val) => { let mut b = Scatter::new(valmap.get_receiver(in_val.input.try_conv(), builder)); in_val.outputs.iter().for_each(|id| b.add_target(valmap.get_sender(id.try_conv(), builder))); builder.add_child(b); }
                fork::Conn::Repsig(_) => panic!("fork repsig"),
            },
            Op::Join(op) => match op.conn.as_ref().unwrap() {
                join::Conn::Crd(in_crd) => { let mut g = Gather::new(crdmap.get_sender(in_crd.output.try_conv(), builder)); in_crd.inputs.iter().for_each(|id| g.add_target(crdmap.get_receiver(id.try_conv(), builder))); builder.add_child(g); }
                join::Conn::Ref(in_ref) => { let mut g = Gather::new(refmap.get_sender(in_ref.output.try_conv(), builder)); in_ref.inputs.iter().for_each(|id| g.add_target(refmap.get_receiver(id.try_conv(), builder))); builder.add_child(g); }
                join::Conn::Val(in_val) => { let mut g = Gather::new(valmap.get_sender(in_val.output.try_conv(), builder)); in_val.inputs.iter().for_each(|id| g.add_target(valmap.get_receiver(id.try_conv(), builder))); builder.add_child(g); }
                join::Conn::Repsig(_) => panic!("join repsig"),
            },
            Op::Locate(op) => {
                let input_ref = refmap.get_receiver(get_ref_id(&op.input_ref), builder);
                let input_crd = crdmap.get_receiver(get_crd_id(&op.input_crd), builder);
                let output_ref1 = refmap.get_sender(get_ref_id(&op.output_ref1), builder);
                let output_ref2 = refmap.get_sender(get_ref_id(&op.output_ref2), builder);
                let output_crd = crdmap.get_sender(get_crd_id(&op.output_crd), builder);
                builder.add_child(IterateLocate::new(input_ref, input_crd, output_ref1, output_ref2, output_crd));
            }
            _ => todo!(),
        }
    }
}
