use std::{env, fs, path::PathBuf};

use dam::utility_contexts::*;

use comal::templates::alu::make_alu;
use comal::templates::array::{Array, ArrayData};

use comal::templates::joiner::{CrdJoinerData, Intersect};
use comal::templates::primitive::Token;
use comal::templates::rd_scanner::{CompressedCrdRdScan, RdScanData};

use comal::config::Data;
use comal::templates::utils::read_inputs;
use dam::simulation::*;
use dam::templates::ops::*;

use comal::templates::wr_scanner::{CompressedWrScan, ValsWrScan};
use comal::token_vec;

use indicatif::ProgressBar;

type VT = f32;

fn test_mat_elemadd(base_path: &PathBuf) -> u64 {
    let b0_seg_filename = base_path.join("tensor_A_mode_0_seg");
    let b0_crd_filename = base_path.join("tensor_A_mode_0_crd");
    let b1_seg_filename = base_path.join("tensor_A_mode_1_seg");
    let b1_crd_filename = base_path.join("tensor_A_mode_1_crd");
    let b_vals_filename = base_path.join("tensor_A_mode_vals");
    let c0_seg_filename = base_path.join("tensor_B_mode_0_seg");
    let c0_crd_filename = base_path.join("tensor_B_mode_0_crd");
    let c1_seg_filename = base_path.join("tensor_B_mode_1_seg");
    let c1_crd_filename = base_path.join("tensor_B_mode_1_crd");
    let c_vals_filename = base_path.join("tensor_B_mode_vals");
    let b0_seg = read_inputs::<u32>(&b0_seg_filename);
    let b0_crd = read_inputs::<u32>(&b0_crd_filename);
    let b1_seg = read_inputs::<u32>(&b1_seg_filename);
    let b1_crd = read_inputs::<u32>(&b1_crd_filename);
    let b_vals = read_inputs::<VT>(&b_vals_filename);
    let c0_seg = read_inputs::<u32>(&c0_seg_filename);
    let c0_crd = read_inputs::<u32>(&c0_crd_filename);
    let c1_seg = read_inputs::<u32>(&c1_seg_filename);
    let c1_crd = read_inputs::<u32>(&c1_crd_filename);
    let c_vals = read_inputs::<VT>(&c_vals_filename);

    let chan_size = 8;

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

    // intersect_i
    let (intersecti_out_crd_sender, intersecti_out_crd_receiver) = parent.bounded(chan_size);
    let (intersecti_out_ref1_sender, intersecti_out_ref1_receiver) = parent.bounded(chan_size);
    let (intersecti_out_ref2_sender, intersecti_out_ref2_receiver) = parent.bounded(chan_size);
    let intersecti_data = CrdJoinerData::<u32, u32> {
        in_crd1: bi_out_crd_receiver,
        in_ref1: bi_out_ref_receiver,
        in_crd2: ci_out_crd_receiver,
        in_ref2: ci_out_ref_receiver,
        out_crd: intersecti_out_crd_sender,
        out_ref1: intersecti_out_ref1_sender,
        out_ref2: intersecti_out_ref2_sender,
    };
    let intersect_i = Intersect::new(intersecti_data);

    // fiberwrite_X0
    let x0_wrscanner = CompressedWrScan::new(intersecti_out_crd_receiver);

    // fiberlookup_bj
    let (bj_out_crd_sender, bj_out_crd_receiver) = parent.bounded(chan_size);
    let (bj_out_ref_sender, bj_out_ref_receiver) = parent.bounded(chan_size);
    let bj_data = RdScanData::<u32, u32> {
        in_ref: intersecti_out_ref1_receiver,
        out_ref: bj_out_ref_sender,
        out_crd: bj_out_crd_sender,
    };
    let bj_rdscanner = CompressedCrdRdScan::new(bj_data, b1_seg, b1_crd);

    // fiberlookup_cj
    let (cj_out_crd_sender, cj_out_crd_receiver) = parent.bounded(chan_size);
    let (cj_out_ref_sender, cj_out_ref_receiver) = parent.bounded(chan_size);
    let cj_data = RdScanData::<u32, u32> {
        in_ref: intersecti_out_ref2_receiver,
        out_ref: cj_out_ref_sender,
        out_crd: cj_out_crd_sender,
    };
    let cj_rdscanner = CompressedCrdRdScan::new(cj_data, c1_seg, c1_crd);

    // intersect_j
    let (intersectj_out_crd_sender, intersectj_out_crd_receiver) = parent.bounded(chan_size);
    let (intersectj_out_ref1_sender, intersectj_out_ref1_receiver) = parent.bounded(chan_size);
    let (intersectj_out_ref2_sender, intersectj_out_ref2_receiver) = parent.bounded(chan_size);
    let intersectj_data = CrdJoinerData::<u32, u32> {
        in_crd1: bj_out_crd_receiver,
        in_ref1: bj_out_ref_receiver,
        in_crd2: cj_out_crd_receiver,
        in_ref2: cj_out_ref_receiver,
        out_crd: intersectj_out_crd_sender,
        out_ref1: intersectj_out_ref1_sender,
        out_ref2: intersectj_out_ref2_sender,
    };
    let intersect_j = Intersect::new(intersectj_data);

    // fiberwrite_x1
    let x1_wrscanner = CompressedWrScan::new(intersectj_out_crd_receiver);

    // arrayvals_b
    let (b_out_val_sender, b_out_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
    let arrayvals_b_data = ArrayData::<u32, f32, u32> {
        in_ref: intersectj_out_ref1_receiver,
        out_val: b_out_val_sender,
    };
    let arrayvals_b = Array::<u32, f32, u32>::new(arrayvals_b_data, b_vals);

    // arrayvals_c
    let (c_out_val_sender, c_out_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
    let arrayvals_c_data = ArrayData::<u32, f32, u32> {
        in_ref: intersectj_out_ref2_receiver,
        out_val: c_out_val_sender,
    };
    let arrayvals_c = Array::<u32, f32, u32>::new(arrayvals_c_data, c_vals);

    // Add ALU
    let (add_out_sender, add_out_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
    let add = make_alu(
        b_out_val_receiver,
        c_out_val_receiver,
        add_out_sender,
        ALUMulOp(),
    );

    // fiberwrite_Xvals
    let xvals = ValsWrScan::<f32, u32>::new(add_out_receiver);

    parent.add_child(b_gen);
    parent.add_child(c_gen);
    parent.add_child(bi_rdscanner);
    parent.add_child(bj_rdscanner);
    parent.add_child(intersect_i);
    parent.add_child(x0_wrscanner);
    parent.add_child(ci_rdscanner);
    parent.add_child(cj_rdscanner);
    parent.add_child(intersect_j);
    parent.add_child(x1_wrscanner);
    parent.add_child(arrayvals_b);
    parent.add_child(arrayvals_c);
    parent.add_child(add);
    parent.add_child(xvals);

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
            .mode(RunMode::Simple)
            .build()
            .unwrap(),
    );
    return executed.elapsed_cycles().expect("Cycle is None");
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
    let mut total_cycles: u64 = 0;
    for item in formatted_dir.iter() {
        bar.inc(1);
        let path: PathBuf = PathBuf::from(item.clone());
        let subtile_abs_path = subtile_dir.join(path);
        total_cycles += test_mat_elemadd(&subtile_abs_path);
    }
    bar.finish();
    println!("Total Runtime: {:?}", total_cycles);
}
