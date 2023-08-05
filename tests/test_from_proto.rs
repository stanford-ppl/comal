use std::collections::HashMap;
use std::{fs, path::Path};

use comal::config::Data;
use comal::templates::joiner::{CrdJoinerData, Intersect, Union};
use comal::templates::primitive::{Repsiggen, Token};
use comal::templates::rd_scanner::RdScanData;
use comal::tortilla::operation::Op;
use comal::tortilla::{CrdStream, Operation, ProgramGraph, RefStream, RepSigStream, ValStream};
use dam_rs::channel::Receiver;
use dam_rs::simulation::Program;
use prost;
use prost::Message;

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

type VT = f32;
type CT = u32;
type ST = u32;
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
    let msg = ProgramGraph::decode(txt.as_slice()).unwrap();

    let mut parent = Program::default();

    let mut crd_hash = HashMap::<u64, Receiver<Token<CT, ST>>>::new();
    let mut ref_hash = HashMap::<u64, Receiver<Token<CT, ST>>>::new();
    let mut val_hash = HashMap::<u64, Receiver<Token<VT, ST>>>::new();
    let mut repsig_hash = HashMap::<u64, Receiver<Repsiggen>>::new();

    for operation in msg.operators {
        match operation.op.expect("Error processing") {
            Op::Broadcast(_) => todo!(),
            Op::Joiner(op) => {
                assert!(op.input_pairs.len() == 2);
                let mut input_channels = op.input_pairs.iter().map(|pair| {
                    let pair_crd = crd_hash
                        .remove(&get_crd_id(&pair.crd))
                        .expect("Undefined crd");
                    let pair_ref = ref_hash
                        .remove(&get_ref_id(&pair.r#ref))
                        .expect("Undefined ref");
                    (pair_crd, pair_ref)
                });
                let (in_crd1, in_ref1) = input_channels.next().unwrap();
                let (in_crd2, in_ref2) = input_channels.next().unwrap();

                let (out_ref1, out_ref1_receiver) = parent.bounded(chan_size);
                let (out_ref2, out_ref2_receiver) = parent.bounded(chan_size);
                let (out_crd, out_crd_receiver) = parent.bounded(chan_size);
                let joiner_data = CrdJoinerData {
                    in_crd1,
                    in_ref1,
                    in_crd2,
                    in_ref2,
                    out_ref1,
                    out_ref2,
                    out_crd,
                };

                let out_ref1_id = get_ref_id(&op.output_ref1);
                let out_ref2_id = get_ref_id(&op.output_ref2);
                let out_crd_id = get_crd_id(&op.output_crd);
                ref_hash.insert(out_ref1_id, out_ref1_receiver);
                ref_hash.insert(out_ref2_id, out_ref2_receiver);
                crd_hash.insert(out_crd_id, out_crd_receiver);
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
                let (out_ref, out_ref_receiver) = parent.bounded(chan_size);
                let (out_crd, out_crd_receiver) = parent.bounded(chan_size);
                let f_data = RdScanData {
                    in_ref: ref_hash.remove(&in_ref_id).unwrap(),
                    out_crd,
                    out_ref,
                };
            }
            Op::FiberWrite(_) => todo!(),
            Op::Repeat(_) => todo!(),
            Op::Repeatsig(_) => todo!(),
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
