pub mod proto_headers;
pub mod util;

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::PathBuf;
// use ndarray::Array;

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
use super::templates::utils::read_inputs;
use super::templates::wr_scanner::{CompressedWrScan, ValsWrScan};
use super::token_vec;
use crate::cli_common::SamOptions;
use crate::proto_driver::util::{get_crd_id, get_ref_id, get_val_id};
use crate::templates::accumulator::MaxReduce;
use crate::templates::binary::Binary;
use crate::templates::joiner::{NIntersect, NJoinerData, NUnion};
use crate::templates::new_alu::{ALUAdd, ALUMul};
use crate::templates::primitive::ALUMaxOp;
use crate::templates::scatter_gather::{Gather, Scatter};
use crate::templates::tensor::{PrimitiveType, Tensor};
use crate::templates::unary::Unary;
use crate::templates::utils::read_inputs_vectorized;

use super::templates::{alu::make_unary_alu, primitive::ALUExpOp};
use dam::channel::adapters::{RecvAdapter, SendAdapter};
use dam::context_tools::*;
use dam::simulation::ProgramBuilder;
use dam::templates::ops::*;
use dam::utility_contexts::{BroadcastContext, GeneratorContext};

use ndarray::{CowArray, Ix1, Ix2};
// use joiner::Payload;
use proto_headers::tortilla::*;

// type VT = f32;
type VT = Tensor<'static, f32, Ix2, 64>;
type CT = u32;
type ST = u32;

enum ChannelType<T: DAMType> {
    SendType(Sender<T>),
    ReceiverType(Receiver<T>),
}

const DEFAULT_CHAN_SIZE: usize = 102400000;

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
    let mut block_size = None;
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
                    builder.add_child(crs);
                } else {
                    let shape_filename = base_path.join(format!("tensor_{}_mode_shape", op.tensor));
                    let shapes = read_inputs(&shape_filename);
                    let index: usize = op.mode.try_into().unwrap();
                    builder.add_child(UncompressedCrdRdScan::new(f_data, shapes[index]));
                }
            }
            Op::FiberWrite(op) => {
                let in_crd_id = get_crd_id(&op.input_crd);
                let receiver = crdmap.get_receiver(in_crd_id, builder);
                builder.add_child(CompressedWrScan::new(receiver));
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
                    // builder.add_child(make_alu(
                    //     val_receiver1,
                    //     val_receiver2,
                    //     out_val_sender,
                    //     match op.stages[0].op() {
                    //         alu::AluOp::Add => ALUAddOp(),
                    //         alu::AluOp::Sub => ALUAddOp(),
                    //         alu::AluOp::Mul => ALUMulOp(),
                    //         alu::AluOp::Div => ALUMulOp(),
                    //         _ => todo!(),
                    //     },
                    // ));
                    let binary_func = match op.stages[0].op() {
                        alu::AluOp::Add => |val1: VT, val2: VT| -> VT { val1 + val2 },
                        alu::AluOp::Sub => |val1: VT, val2: VT| -> VT { val1 - val2 },
                        alu::AluOp::Mul => |val1: VT, val2: VT| -> VT { val1 * val2 },
                        alu::AluOp::Div => |val1: VT, val2: VT| -> VT { val1 / val2 },
                        _ => todo!(),
                    };
                    builder.add_child(Binary::new(
                        val_receiver1,
                        val_receiver2,
                        out_val_sender,
                        binary_func,
                        block_size.unwrap(),
                    ));
                } else if in_val_ids.len() == 1 {
                    let val_receiver1 = valmap.get_receiver(in_val_ids.next().unwrap(), builder);
                    match op.stages[0].op() {
                        alu::AluOp::Exp => {
                            let unary_func = |val: VT| -> VT { val };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size.unwrap(),
                            ));
                        }
                        // alu::AluOp::Sin => {
                        //     let unary_func = |val: f32| -> f32 { val.sin() };
                        //     builder.add_child(Unary::new(
                        //         val_receiver1,
                        //         out_val_sender,
                        //         unary_func,
                        //     ));
                        // }
                        // alu::AluOp::Cos => {
                        //     let unary_func = |val: f32| -> f32 { val.cos() };
                        //     builder.add_child(Unary::new(
                        //         val_receiver1,
                        //         out_val_sender,
                        //         unary_func,
                        //     ));
                        // }
                        alu::AluOp::Max => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: VT| -> VT { val };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size.unwrap(),
                            ));
                        }
                        // alu::AluOp::Scalaradd => {
                        //     let scalar: f32 = op.scalar as f32;
                        //     let unary_func = move |val: f32| -> f32 { val + scalar };
                        //     builder.add_child(Unary::new(
                        //         val_receiver1,
                        //         out_val_sender,
                        //         unary_func,
                        //     ));
                        // }
                        alu::AluOp::Scalarmul => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: VT| -> VT { val };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size.unwrap(),
                            ));
                        }
                        alu::AluOp::Scalardiv => {
                            let scalar: f32 = op.scalar as f32;
                            let unary_func = move |val: VT| -> VT { val };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size.unwrap(),
                            ));
                        }
                        // alu::AluOp::Rsqrt => {
                        //     let unary_func = |val: f32| -> f32 { 1.0 / val.sqrt() };
                        //     builder.add_child(Unary::new(
                        //         val_receiver1,
                        //         out_val_sender,
                        //         unary_func,
                        //     ));
                        // }
                        alu::AluOp::Sigmoid => {
                            // let unary_func = |val: VT| -> VT { 1.0 / (1.0 + f32::exp(-val)) };
                            let unary_func = |val: VT| -> VT { val };
                            builder.add_child(Unary::new(
                                val_receiver1,
                                out_val_sender,
                                unary_func,
                                block_size.unwrap(),
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
                let reduce_data = ReduceData {
                    in_val: valmap.get_receiver(in_val_id, builder),
                    out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                    block_size: block_size.unwrap(),
                };
                let min_val = Tensor::<'static, f32, Ix1, 16> {
                    data: CowArray::from(ndarray::Array::from_vec(vec![f32::MIN; block_size.unwrap()])),
                };
                match op.reduce_type() {
                    reduce::Type::Add => builder.add_child(Reduce::new(reduce_data)),
                    // reduce::Type::Max => builder.add_child(MaxReduce::new(reduce_data, min_val)),
                    reduce::Type::Max => builder.add_child(Reduce::new(reduce_data)),
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
                let blocked = op.blocked;
                let stream_shape = op.stream_shape as usize;
                const N: usize = 64;
                let in_ref_id = get_ref_id(&op.input_ref);
                let val_filename = base_path.join(format!("tensor_{}_mode_vals", op.tensor));
                if blocked {
                    let array_data = ArrayData {
                        in_ref: refmap.get_receiver(in_ref_id, builder),
                        out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                        block_size: stream_shape,
                    };
                    let vals = read_inputs_vectorized(&val_filename, PrimitiveType::<VT>::new());
                    block_size = Some(stream_shape);
                    builder.add_child(Array::new(array_data, vals));
                } else {
                    let array_data = ArrayData {
                        in_ref: refmap.get_receiver(in_ref_id, builder),
                        out_val: valmap.get_sender(get_val_id(&op.output_val), builder),
                        block_size: stream_shape,
                    };
                    let vals = read_inputs_vectorized(&val_filename, PrimitiveType::<VT>::new());
                    builder.add_child(Array::new(array_data, vals));
                }
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
                    block_size: block_size.unwrap(),
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
                // root_receiver
            }
            // Op::Fork(op) => match op.conn.as_ref().unwrap() {
            //     // panic!("not supported");
            //     // fork::Conn::Crd(in_crd) => {
            //     //     let in_crd_id = in_crd.input.try_conv();
            //     //     let out_crd_ids = in_crd.outputs.iter().map(|id| id.try_conv());
            //     //     let receiver = crdmap.get_receiver(in_crd_id, builder);
            //     //     let mut broadcast = Scatter::new(receiver);
            //     //     out_crd_ids
            //     //         .into_iter()
            //     //         .for_each(|id| broadcast.add_target(crdmap.get_sender(id, builder)));
            //     //     builder.add_child(broadcast);
            //     // }
            //     // fork::Conn::Ref(in_ref) => {
            //     //     let in_ref_id = in_ref.input.try_conv();
            //     //     let out_ref_ids = in_ref.outputs.iter().map(|id| id.try_conv());
            //     //     let receiver = refmap.get_receiver(in_ref_id, builder);
            //     //     let mut scatter = Scatter::new(receiver);
            //     //     out_ref_ids
            //     //         .into_iter()
            //     //         .for_each(|id| scatter.add_target(refmap.get_sender(id, builder)));
            //     //     builder.add_child(scatter);
            //     // }
            //     // fork::Conn::Val(in_val) => {
            //     //     let in_val_id = in_val.input.try_conv();
            //     //     let out_val_ids = in_val.outputs.iter().map(|id| id.try_conv());
            //     //     let receiver = valmap.get_receiver(in_val_id, builder);
            //     //     let mut broadcast = Scatter::new(receiver);
            //     //     out_val_ids
            //     //         .into_iter()
            //     //         .for_each(|id| broadcast.add_target(valmap.get_sender(id, builder)));
            //     //     builder.add_child(broadcast);
            //     // }
            //     // fork::Conn::Repsig(_) => {
            //     //     panic!("Attempting to fork a repsig");
            //     // }
            // },
            // Op::Join(op) => match op.conn.as_ref().unwrap() {
            //     join::Conn::Crd(in_crd) => {
            //         let in_crd_id = in_crd.output.try_conv();
            //         let sender = crdmap.get_sender(in_crd_id, builder);
            //         let out_crd_ids = in_crd.inputs.iter().map(|id| id.try_conv());
            //         let mut gather = Gather::new(sender);
            //         out_crd_ids
            //             .into_iter()
            //             .for_each(|id| gather.add_target(crdmap.get_receiver(id, builder)));
            //         builder.add_child(gather);
            //     }
            //     join::Conn::Ref(in_ref) => {
            //         let in_ref_id = in_ref.output.try_conv();
            //         let out_ref_ids = in_ref.inputs.iter().map(|id| id.try_conv());
            //         let sender = refmap.get_sender(in_ref_id, builder);
            //         let mut gather = Gather::new(sender);
            //         out_ref_ids
            //             .into_iter()
            //             .for_each(|id| gather.add_target(refmap.get_receiver(id, builder)));
            //         builder.add_child(gather);
            //     }
            //     join::Conn::Val(in_val) => {
            //         let in_val_id = in_val.output.try_conv();
            //         let out_val_ids = in_val.inputs.iter().map(|id| id.try_conv());
            //         let sender = valmap.get_sender(in_val_id, builder);
            //         let mut gather = Gather::new(sender);
            //         out_val_ids
            //             .into_iter()
            //             .for_each(|id| gather.add_target(valmap.get_receiver(id, builder)));
            //         builder.add_child(gather);
            //     }
            //     join::Conn::Repsig(_) => {
            //         panic!("Attempting to join repsig");
            //     }
            // },
            _ => todo!(),
        }
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
