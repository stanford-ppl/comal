use std::{env, fs, path::PathBuf};

use dam::utility_contexts::*;

use comal::templates::alu::make_alu;
use comal::templates::array::{Array, ArrayData};

use comal::templates::joiner::{CrdJoinerData, Union};
use comal::templates::primitive::{Repsiggen, Token};
use comal::templates::rd_scanner::{RdScanData, UncompressedCrdRdScan};

use comal::config::Data;
use comal::templates::utils::{read_inputs, write_output};
use dam::simulation::*;
use dam::templates::ops::*;

use comal::templates::wr_scanner::{CompressedWrScan, ValsWrScan};
use comal::token_vec;

use indicatif::ProgressBar;

type VT = f32;

fn test_dense_mat_elemadd(base_path: &PathBuf) {
    let b0_seg_filename = base_path.join("tensor_B_mode_0_seg");
    let b1_seg_filename = base_path.join("tensor_B_mode_1_seg");
    let b_vals_filename = base_path.join("tensor_B_mode_vals");
    let c0_seg_filename = base_path.join("tensor_C_mode_0_seg");
    let c1_seg_filename = base_path.join("tensor_C_mode_1_seg");
    let c_vals_filename = base_path.join("tensor_C_mode_vals");
    let b0_seg = read_inputs::<u32>(&b0_seg_filename);
    let b1_seg = read_inputs::<u32>(&b1_seg_filename);
    let b_vals = read_inputs::<VT>(&b_vals_filename);
    let c0_seg = read_inputs::<u32>(&c0_seg_filename);
    let c1_seg = read_inputs::<u32>(&c1_seg_filename);
    let c_vals = read_inputs::<VT>(&c_vals_filename);

    let mut c_shape = Vec::new();
    let mut b_shape = Vec::new();

    // the shape output from Lego is always 30, 30
    // for dense matrix, we can use the first element of the seg array
    // as the shape of the dimension
    c_shape.push(c0_seg[1]);
    c_shape.push(c1_seg[1]);
    b_shape.push(b0_seg[1]);
    b_shape.push(b1_seg[1]);
    let chan_size = 32784;

    let mut parent = ProgramBuilder::default();

    let _mk_bounded = || parent.bounded::<Token<u32, u32>>(chan_size);
    let _mk_boundedf = || parent.bounded::<Token<VT, u32>>(chan_size);
    let _mk_rsiggen_bounded = || parent.bounded::<Repsiggen>(chan_size);

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
    let bi_rdscanner = UncompressedCrdRdScan::new(bi_data, b_shape[0]);

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
    let ci_rdscanner = UncompressedCrdRdScan::new(ci_data, c_shape[0]);

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

    // fiberlookup_bj
    let (bj_out_crd_sender, bj_out_crd_receiver) = parent.bounded(chan_size);
    let (bj_out_ref_sender, bj_out_ref_receiver) = parent.bounded(chan_size);
    let bj_data = RdScanData::<u32, u32> {
        in_ref: unioni_out_ref1_receiver,
        out_ref: bj_out_ref_sender,
        out_crd: bj_out_crd_sender,
    };
    let bj_rdscanner = UncompressedCrdRdScan::new(bj_data, b_shape[1]);

    // fiberlookup_cj
    let (cj_out_crd_sender, cj_out_crd_receiver) = parent.bounded(chan_size);
    let (cj_out_ref_sender, cj_out_ref_receiver) = parent.bounded(chan_size);
    let cj_data = RdScanData::<u32, u32> {
        in_ref: unioni_out_ref2_receiver,
        out_ref: cj_out_ref_sender,
        out_crd: cj_out_crd_sender,
    };
    let cj_rdscanner = UncompressedCrdRdScan::new(cj_data, c_shape[1]);

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

    // fiberwrite_X0
    let x0_wrscanner = CompressedWrScan::new(unioni_out_crd_receiver);

    // fiberwrite_x1
    let x1_wrscanner = CompressedWrScan::new(unionj_out_crd_receiver);

    // fiberwrite_Xvals
    let xvals = ValsWrScan::<VT, u32>::new(add_out_receiver);

    let x_val = xvals.out_val.clone();
    let x1_crd = x1_wrscanner.crd_arr.clone();
    let x1_seg = x1_wrscanner.seg_arr.clone();
    let x0_crd = x0_wrscanner.crd_arr.clone();
    let x0_seg = x0_wrscanner.seg_arr.clone();

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

    let _executed = initialized.run(
        RunOptionsBuilder::default()
            .mode(RunMode::Simple)
            .build()
            .unwrap(),
    );
    // println!("Elapsed cycles: {:?}", executed.elapsed_cycles());
    // println!("Checking Results");

    let x0_seg_filename = base_path.join("tensor_X_mode_0_seg");
    let x0_crd_filename = base_path.join("tensor_X_mode_0_crd");
    let x1_seg_filename = base_path.join("tensor_X_mode_1_seg");
    let x1_crd_filename = base_path.join("tensor_X_mode_1_crd");
    let x_vals_filename = base_path.join("tensor_X_mode_vals");

    write_output::<u32>(&x0_seg_filename, x0_seg);
    write_output::<u32>(&x0_crd_filename, x0_crd);
    write_output::<u32>(&x1_seg_filename, x1_seg);
    write_output::<u32>(&x1_crd_filename, x1_crd);
    write_output::<VT>(&x_vals_filename, x_val);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let subtile_paths_file = PathBuf::from(&args[1]);
    let contents = fs::read_to_string(subtile_paths_file.clone()).unwrap();
    let data: Data = toml::from_str(&contents).unwrap();
    let formatted_dir = data.sam_config.sam_path;
    // assume this file structure
    // sparse-ml-kernel/
    // ├─ subtile_paths_file.toml
    // ├─ subtile_files/
    // the paths in subtile_paths_file are relative to the subtile_path_file
    let mut subtile_dir = subtile_paths_file.clone();
    subtile_dir.pop();

    let bar = ProgressBar::new(formatted_dir.len() as u64);
    for item in formatted_dir.iter() {
        bar.inc(1);
        let path: PathBuf = PathBuf::from(item.clone());
        let subtile_abs_path = subtile_dir.join(path);
        test_dense_mat_elemadd(&subtile_abs_path);
    }
    bar.finish();
}
