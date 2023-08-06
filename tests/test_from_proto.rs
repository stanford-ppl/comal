use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::{fs, path::Path};

use comal::config::Data;
use comal::templates::joiner::{CrdJoinerData, Intersect, Union};
use comal::templates::primitive::{Repsiggen, Token};
use comal::templates::rd_scanner::{CompressedCrdRdScan, RdScanData, UncompressedCrdRdScan};
use comal::templates::repeat::{RepSigGenData, Repeat, RepeatData};
use comal::templates::utils::read_inputs;
use comal::templates::wr_scanner::CompressedWrScan;
use comal::tortilla::operation::Op;
use comal::tortilla::{
    ComalGraph, CrdStream, Operation, ProgramGraph, RefStream, RepSigStream, ValStream,
};
use dam_rs::channel::{Receiver, Sender};
use dam_rs::simulation::Program;
use frunk::labelled::chars::I;
use prost;
use prost::Message;

type VT = f32;
type CT = u32;
type ST = u32;
type CoordType = Token<CT, ST>;
type RefType = Token<CT, ST>;
type ValType = Token<VT, ST>;

fn get_crd_id(stream: &Option<CrdStream>) -> u64 {
    return stream
        .as_ref()
        .expect("Undefined crdstream")
        .id
        .as_ref()
        .expect("Error getting id")
        .id;
}

fn get_ref_id(stream: &Option<RefStream>) -> u64 {
    return stream
        .as_ref()
        .expect("Undefined crdstream")
        .id
        .as_ref()
        .expect("Error getting id")
        .id;
}

fn get_val_id(stream: &Option<ValStream>) -> u64 {
    return stream
        .as_ref()
        .expect("Undefined crdstream")
        .id
        .as_ref()
        .expect("Error getting id")
        .id;
}

fn get_repsig_id(stream: &Option<RepSigStream>) -> u64 {
    return stream
        .as_ref()
        .expect("Undefined crdstream")
        .id
        .as_ref()
        .expect("Error getting id")
        .id;
}

enum ChannelType<T: Clone> {
    SendType(Sender<T>),
    ReceiverType(Receiver<T>),
}

struct Channels<'a, T>
where
    T: Clone + 'a,
{
    map: HashMap<u64, ChannelType<T>>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> Channels<'a, T>
where
    T: Clone + 'a,
{
    pub fn new() -> Self {
        Self {
            map: Default::default(),
            _marker: Default::default(),
        }
    }

    fn new_channel(parent: &mut Program<'a>, _id: u64) -> (Sender<T>, Receiver<T>) {
        parent.bounded(1024)
    }

    pub fn get_sender(&mut self, id: u64, parent: &mut Program<'a>) -> Sender<T> {
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
    pub fn get_receiver(&mut self, id: u64, parent: &mut Program<'a>) -> Receiver<T> {
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
}

#[test]
fn test_matmul_proto() {
    let test_name = "mat_elemadd";
    let filename = home::home_dir().unwrap().join("sam_config.toml");
    let contents = fs::read_to_string(filename).unwrap();
    let data: Data = toml::from_str(&contents).unwrap();
    let formatted_dir = data.sam_config.sam_path;
    let base_path = Path::new(&formatted_dir).join(&test_name);

    // Set default channel size
    let mut chan_size = 1024;

    let txt = fs::read("sam.bin").unwrap();
    // let msg = ProgramGraph::
    let msg = ComalGraph::decode(txt.as_slice()).unwrap();

    let mut parent = Program::default();

    let mut refmap: Channels<CoordType> = Channels::new();
    let mut crdmap: Channels<CoordType> = Channels::new();
    let mut valmap: Channels<ValType> = Channels::new();
    let mut repmap: Channels<Repsiggen> = Channels::new();

    for operation in msg.graph.unwrap().operators {
        match operation.op.expect("Error processing") {
            Op::Broadcast(_) => todo!(),
            Op::Joiner(op) => {
                assert!(op.input_pairs.len() == 2);
                let mut input_channels = op.input_pairs.iter().map(|pair| {
                    let pair_crd = crdmap.get_receiver(get_crd_id(&pair.crd), &mut parent);
                    let pair_ref = refmap.get_receiver(get_ref_id(&pair.r#ref), &mut parent);
                    (pair_crd, pair_ref)
                });
                let (in_crd1, in_ref1) = input_channels.next().unwrap();
                let (in_crd2, in_ref2) = input_channels.next().unwrap();

                let joiner_data = CrdJoinerData {
                    in_crd1,
                    in_ref1,
                    in_crd2,
                    in_ref2,
                    out_ref1: refmap.get_sender(get_ref_id(&op.output_ref1), &mut parent),
                    out_ref2: refmap.get_sender(get_ref_id(&op.output_ref2), &mut parent),
                    out_crd: refmap.get_sender(get_crd_id(&op.output_crd), &mut parent),
                };

                // let joiner =
                match op.join_type() {
                    comal::tortilla::joiner::Type::Intersect => {
                        parent.add_child(Intersect::new(joiner_data))
                    }
                    comal::tortilla::joiner::Type::Union => {
                        parent.add_child(Union::new(joiner_data))
                    }
                };
            }
            Op::FiberLookup(op) => {
                let in_ref_id = get_ref_id(&op.input_ref);
                let f_data = RdScanData {
                    in_ref: refmap.get_receiver(in_ref_id, &mut parent),
                    out_crd: crdmap.get_sender(get_crd_id(&op.output_crd), &mut parent),
                    out_ref: refmap.get_sender(get_ref_id(&op.output_ref), &mut parent),
                };

                if op.format == "compressed" {
                    let seg_filename =
                        base_path.join(format!("tensor_{}_mode_{}_seg", op.tensor, op.mode));
                    let crd_filename =
                        base_path.join(format!("tensor_{}_mode_{}_crd", op.tensor, op.mode));
                    let seg = read_inputs(&seg_filename);
                    let crd = read_inputs(&crd_filename);
                    parent.add_child(CompressedCrdRdScan::new(f_data, seg, crd));
                } else {
                    let shape_filename = base_path.join(format!("tensor_{}_mode_shape", op.tensor));
                    let shapes = read_inputs(&shape_filename);
                    let index: usize = op.mode.try_into().unwrap();
                    parent.add_child(UncompressedCrdRdScan::new(f_data, shapes[index]));
                }
            }
            Op::FiberWrite(op) => {
                let in_crd_id = get_crd_id(&op.input_crd);
                let receiver = crdmap.get_receiver(in_crd_id, &mut parent);
                parent.add_child(CompressedWrScan::new(receiver));
            }
            Op::Repeat(op) => {
                let in_ref_id = get_ref_id(&op.input_ref);
                let in_repsig_id = get_repsig_id(&op.input_rep_sig);
                let rep_data = RepeatData {
                    in_ref: refmap.get_receiver(in_ref_id, &mut parent),
                    in_repsig: repmap.get_receiver(in_repsig_id, &mut parent),
                    out_ref: refmap.get_sender(get_ref_id(&op.output_ref), &mut parent),
                };
                parent.add_child(Repeat::new(rep_data));
            }
            Op::Repeatsig(op) => {
                let in_crd_id = get_crd_id(&op.input_crd);
                let repsig_data = RepSigGenData {
                    input: crdmap.get_receiver(in_crd_id, &mut parent),
                    out_repsig: repmap.get_sender(get_repsig_id(&op.output_rep_sig), &mut parent),
                };
            }
            Op::Alu(_) => todo!(),
            Op::Reduce(_) => todo!(),
            Op::CoordHold(_) => todo!(),
            Op::CoordMask(_) => todo!(),
            Op::CoordDrop(_) => todo!(),
            Op::Array(_) => todo!(),
            Op::Spacc(_) => todo!(),
            Op::ValWrite(_) => todo!(),
        }
    }

    // let proto = Operation::operation;
    let proto = Op::Broadcast;
}
