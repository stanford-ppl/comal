
use std::{fs, path::Path};

use comal::templates::tensor::{PrimitiveType, Tensor};
use dam::utility_contexts::*;

use comal::templates::alu::make_alu;
use comal::templates::array::{Array, ArrayData};

use comal::templates::joiner::{CrdJoinerData, Union};
use comal::templates::primitive::Token;
use comal::templates::rd_scanner::{CompressedCrdRdScan, RdScanData};

use comal::config::Data;
use comal::templates::utils::{read_inputs, read_inputs_vectorized};
use dam::simulation::*;
use dam::templates::ops::*;

use comal::templates::wr_scanner::{CompressedWrScan, ValsWrScan};
use comal::token_vec;
use ndarray::Ix1;

type VT = Tensor<'static, f32, Ix1, 2>;
// type VT = f32;
// type CT = u32;
// type ST = u32;
// type CoordType = Token<CT, ST>;
// type RefType = Token<CT, ST>;
// type ValType = Token<VT, ST>;

#[test]
fn test_mat_elemadd() {
    let test_name = "mat_elemadd_vec";
    let filename = home::home_dir().unwrap().join("sam_config.toml");
    let contents = fs::read_to_string(filename).unwrap();
    let data: Data = toml::from_str(&contents).unwrap();
    let formatted_dir = data.sam_config.sam_path;
    let base_path = Path::new(&formatted_dir).join(test_name);
    let b0_seg_filename = base_path.join("tensor_B_mode_0_seg");
    let b0_crd_filename = base_path.join("tensor_B_mode_0_crd");
    let b1_seg_filename = base_path.join("tensor_B_mode_1_seg");
    let b1_crd_filename = base_path.join("tensor_B_mode_1_crd");
    let b_vals_filename = base_path.join("tensor_B_mode_vals");
    let c0_seg_filename = base_path.join("tensor_C_mode_0_seg");
    let c0_crd_filename = base_path.join("tensor_C_mode_0_crd");
    let c1_seg_filename = base_path.join("tensor_C_mode_1_seg");
    let c1_crd_filename = base_path.join("tensor_C_mode_1_crd");
    let c_vals_filename = base_path.join("tensor_C_mode_vals");

    let b0_seg = read_inputs::<u32>(&b0_seg_filename);
    let b0_crd = read_inputs::<u32>(&b0_crd_filename);
    let b1_seg = read_inputs::<u32>(&b1_seg_filename);
    let b1_crd = read_inputs::<u32>(&b1_crd_filename);
    // let b_vals = read_inputs::<VT>(&b_vals_filename);
    let b_vals = read_inputs_vectorized(&b_vals_filename, PrimitiveType::<VT>::new());
    let c0_seg = read_inputs::<u32>(&c0_seg_filename);
    let c0_crd = read_inputs::<u32>(&c0_crd_filename);
    let c1_seg = read_inputs::<u32>(&c1_seg_filename);
    let c1_crd = read_inputs::<u32>(&c1_crd_filename);
    // let c_vals = read_inputs::<VT>(&c_vals_filename);
    let c_vals = read_inputs_vectorized(&c_vals_filename, PrimitiveType::<VT>::new());

    let chan_size = 4096;

    let mut parent = ProgramBuilder::default();

    // fiberlookup_bi
    let (bi_out_ref_sender, bi_out_ref_receiver) = parent.bounded(chan_size);
    let (bi_out_crd_sender, bi_out_crd_receiver) = parent.bounded(chan_size);
    let (bi_in_ref_sender, bi_in_ref_receiver) = parent.bounded(chan_size);
    let bi_data = RdScanData::<u32, u32> {
        in_ref: bi_in_ref_receiver,
        out_ref: bi_out_ref_sender,
        out_crd: bi_out_crd_sender,
    };

    let b_gen = GeneratorContext::new(
        || token_vec!(u32; u32; 0, "D").into_iter(),
        bi_in_ref_sender,
    );
    let bi_rdscanner = CompressedCrdRdScan::new(bi_data, b0_seg, b0_crd);

    // fiberlookup_ci
    let (ci_out_crd_sender, ci_out_crd_receiver) = parent.bounded(chan_size);
    let (ci_out_ref_sender, ci_out_ref_receiver) = parent.bounded(chan_size);
    let (ci_in_ref_sender, ci_in_ref_receiver) = parent.bounded(chan_size);
    let ci_data = RdScanData::<u32, u32> {
        in_ref: ci_in_ref_receiver,
        out_ref: ci_out_ref_sender,
        out_crd: ci_out_crd_sender,
    };
    let c_gen = GeneratorContext::new(
        || token_vec!(u32; u32; 0, "D").into_iter(),
        ci_in_ref_sender,
    );
    let ci_rdscanner = CompressedCrdRdScan::new(ci_data, c0_seg, c0_crd);

    // union_i
    let (unioni_out_crd_sender, unioni_out_crd_receiver) = parent.bounded(chan_size);
    let (unioni_out_ref1_sender, unioni_out_ref1_receiver) = parent.bounded(chan_size);
    let (unioni_out_ref2_sender, unioni_out_ref2_receiver) = parent.bounded(chan_size);
    let unioni_data = CrdJoinerData::<u32, u32> {
        in_crd1: bi_out_crd_receiver,
        in_ref1: bi_out_ref_receiver,
        in_crd2: ci_out_crd_receiver,
        in_ref2: ci_out_ref_receiver,
        out_crd: unioni_out_crd_sender,
        out_ref1: unioni_out_ref1_sender,
        out_ref2: unioni_out_ref2_sender,
    };
    let union_i = Union::new(unioni_data);

    // fiberwrite_X0
    let x0_wrscanner = CompressedWrScan::new(unioni_out_crd_receiver);

    // fiberlookup_bj
    let (bj_out_crd_sender, bj_out_crd_receiver) = parent.bounded(chan_size);
    let (bj_out_ref_sender, bj_out_ref_receiver) = parent.bounded(chan_size);
    let bj_data = RdScanData::<u32, u32> {
        in_ref: unioni_out_ref1_receiver,
        out_ref: bj_out_ref_sender,
        out_crd: bj_out_crd_sender,
    };
    let bj_rdscanner = CompressedCrdRdScan::new(bj_data, b1_seg, b1_crd);

    // fiberlookup_cj
    let (cj_out_crd_sender, cj_out_crd_receiver) = parent.bounded(chan_size);
    let (cj_out_ref_sender, cj_out_ref_receiver) = parent.bounded(chan_size);
    let cj_data = RdScanData::<u32, u32> {
        in_ref: unioni_out_ref2_receiver,
        out_ref: cj_out_ref_sender,
        out_crd: cj_out_crd_sender,
    };
    let cj_rdscanner = CompressedCrdRdScan::new(cj_data, c1_seg, c1_crd);

    // union_j
    let (unionj_out_crd_sender, unionj_out_crd_receiver) = parent.bounded(chan_size);
    let (unionj_out_ref1_sender, unionj_out_ref1_receiver) = parent.bounded(chan_size);
    let (unionj_out_ref2_sender, unionj_out_ref2_receiver) = parent.bounded(chan_size);
    let unionj_data = CrdJoinerData::<u32, u32> {
        in_crd1: bj_out_crd_receiver,
        in_ref1: bj_out_ref_receiver,
        in_crd2: cj_out_crd_receiver,
        in_ref2: cj_out_ref_receiver,
        out_crd: unionj_out_crd_sender,
        out_ref1: unionj_out_ref1_sender,
        out_ref2: unionj_out_ref2_sender,
    };
    let union_j = Union::new(unionj_data);

    // fiberwrite_x1
    let x1_wrscanner = CompressedWrScan::new(unionj_out_crd_receiver);

    // arrayvals_b
    let (b_out_val_sender, b_out_val_receiver) = parent.bounded::<Token<VT, u32>>(chan_size);
    let arrayvals_b_data = ArrayData::<u32, VT, u32> {
        in_ref: unionj_out_ref1_receiver,
        out_val: b_out_val_sender,
        // out_val: parent.void(),
    };
    let arrayvals_b = Array::<u32, VT, u32>::new(arrayvals_b_data, b_vals);

    // arrayvals_c
    let (c_out_val_sender, c_out_val_receiver) = parent.bounded::<Token<VT, u32>>(chan_size);
    let arrayvals_c_data = ArrayData::<u32, VT, u32> {
        in_ref: unionj_out_ref2_receiver,
        // out_val: parent.void(),
        out_val: c_out_val_sender,
    };
    let arrayvals_c = Array::<u32, VT, u32>::new(arrayvals_c_data, c_vals);

    // Add ALU
    let (add_out_sender, add_out_receiver) = parent.bounded::<Token<VT, u32>>(chan_size);
    let add = make_alu(
        b_out_val_receiver,
        c_out_val_receiver,
        add_out_sender,
        ALUAddOp(),
    );
    parent.add_child(add);

    // fiberwrite_Xvals
    let xvals = ValsWrScan::<VT, u32>::new(add_out_receiver);
    parent.add_child(xvals);

    parent.add_child(b_gen);
    parent.add_child(c_gen);
    parent.add_child(bi_rdscanner);
    parent.add_child(bj_rdscanner);
    parent.add_child(union_i);
    parent.add_child(x0_wrscanner);
    parent.add_child(ci_rdscanner);
    parent.add_child(cj_rdscanner);
    parent.add_child(union_j);
    parent.add_child(x1_wrscanner);
    parent.add_child(arrayvals_b);
    parent.add_child(arrayvals_c);

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
            .mode(RunMode::Simple).logging(LoggingOptions::Mongo(
                    MongoOptionsBuilder::default()
                        .db("matadd".to_string())
                        .uri("mongodb://127.0.0.1:27017".to_string())
                        .build()
                        .unwrap(),
                ))
            .build()
            .unwrap(),
    );
    println!("Elapsed cycles: {:?}", executed.elapsed_cycles());
}
