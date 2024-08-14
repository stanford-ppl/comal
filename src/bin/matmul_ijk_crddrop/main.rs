use std::{env, fs, path::PathBuf};

use dam::utility_contexts::*;

use comal::templates::accumulator::{Reduce, ReduceData};
use comal::templates::alu::make_alu;
use comal::templates::array::{Array, ArrayData};

use comal::templates::crd_manager::{CrdDrop, CrdDropData};
use comal::templates::joiner::{CrdJoinerData, Intersect};
use comal::templates::primitive::{Repsiggen, Token};
use comal::templates::rd_scanner::{CompressedCrdRdScan, RdScanData};
use comal::templates::repeat::{RepSigGenData, Repeat, RepeatData, RepeatSigGen};
use comal::templates::stkn_dropper::StknDrop;
use comal::templates::val_dropper::{ValDrop, ValDropData};

use comal::config::Data;
use comal::templates::utils::{read_inputs, write_output};
use dam::simulation::*;
use dam::templates::ops::*;

use comal::templates::wr_scanner::{CompressedWrScan, ValsWrScan};
use comal::token_vec;

use indicatif::ProgressBar;

type VT = i32;

fn test_matmul_ijk(base_path: &PathBuf) -> Option<u64> {
    let b0_seg_filename = base_path.join("tensor_B_mode_0_seg");
    let b0_crd_filename = base_path.join("tensor_B_mode_0_crd");
    let b1_seg_filename = base_path.join("tensor_B_mode_1_seg");
    let b1_crd_filename = base_path.join("tensor_B_mode_1_crd");
    let b_vals_filename = base_path.join("tensor_B_mode_vals");
    let c0_seg_filename = base_path.join("tensor_C_mode_1_seg");
    let c0_crd_filename = base_path.join("tensor_C_mode_1_crd");
    let c1_seg_filename = base_path.join("tensor_C_mode_0_seg");
    let c1_crd_filename = base_path.join("tensor_C_mode_0_crd");
    let c_vals_filename = base_path.join("tensor_C_mode_vals");
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

    let chan_size = 32784;

    let mut parent = ProgramBuilder::default();

    let _mk_bounded = || parent.bounded::<Token<u32, u32>>(chan_size);
    let _mk_boundedf = || parent.bounded::<Token<VT, u32>>(chan_size);
    let _mk_rsiggen_bounded = || parent.bounded::<Repsiggen>(chan_size);

    // fiberlookup_bi
    let (bi_out_ref_sender, bi_out_ref_receiver) = parent.bounded(chan_size);
    let (bi_out_crd_sender, bi_out_crd_receiver) = parent.bounded(chan_size);
    let (bi_in_ref_sender, bi_in_ref_receiver) = parent.bounded(chan_size);
    let b_gen = GeneratorContext::new(
        || token_vec!(u32; u32; 0, "D").into_iter(),
        bi_in_ref_sender,
    );
    let bi_data = RdScanData::<u32, u32> {
        in_ref: bi_in_ref_receiver,
        out_ref: bi_out_ref_sender,
        out_crd: bi_out_crd_sender,
    };

    let bi_rdscanner = CompressedCrdRdScan::new(bi_data, b0_seg, b0_crd);

    // repeatsiggen
    let (bc_bi_out_ref_sender, bc_bi_out_ref_receiver) = parent.bounded(chan_size);
    let (bc1_bi_out_ref_sender, bc1_bi_out_ref_receiver) = parent.bounded(chan_size);
    let mut broadcast = BroadcastContext::new(bi_out_ref_receiver);
    broadcast.add_target(bc_bi_out_ref_sender);
    broadcast.add_target(bc1_bi_out_ref_sender);

    let (out_repsig_sender, out_repsig_receiver) = parent.bounded(chan_size);
    let repsig_data = RepSigGenData::<u32, u32> {
        input: bc_bi_out_ref_receiver,
        out_repsig: out_repsig_sender,
    };
    let repsig_i = RepeatSigGen::new(repsig_data);

    // repeat
    let (ci_in_ref_sender, ci_in_ref_receiver) = parent.bounded(chan_size);
    let c_gen = GeneratorContext::new(
        || token_vec!(u32; u32; 0, "D").into_iter(),
        ci_in_ref_sender,
    );
    let (out_repeat_sender, out_repeat_receiver) = parent.bounded(chan_size);
    let ci_repeat_data = RepeatData::<u32, u32> {
        in_ref: ci_in_ref_receiver,
        in_repsig: out_repsig_receiver,
        out_ref: out_repeat_sender,
    };
    let ci_repeat = Repeat::new(ci_repeat_data);

    // fiberlookup_cj
    let (cj_out_crd_sender, cj_out_crd_receiver) = parent.bounded(chan_size);
    let (cj_out_ref_sender, cj_out_ref_receiver) = parent.bounded(chan_size);
    let cj_data = RdScanData::<u32, u32> {
        in_ref: out_repeat_receiver,
        out_ref: cj_out_ref_sender,
        out_crd: cj_out_crd_sender,
    };
    let cj_rdscanner = CompressedCrdRdScan::new(cj_data, c0_seg, c0_crd);

    let (bc_cj_out_ref_sender, bc_cj_out_ref_receiver) = parent.bounded(chan_size);
    let (bc1_cj_out_ref_sender, bc1_cj_out_ref_receiver) = parent.bounded(chan_size);
    let mut broadcast1 = BroadcastContext::new(cj_out_ref_receiver);
    broadcast1.add_target(bc_cj_out_ref_sender);
    broadcast1.add_target(bc1_cj_out_ref_sender);

    // fiberlookup_ck
    let (ck_out_crd_sender, ck_out_crd_receiver) = parent.bounded(chan_size);
    let (ck_out_ref_sender, ck_out_ref_receiver) = parent.bounded(chan_size);
    let ck_data = RdScanData::<u32, u32> {
        in_ref: bc_cj_out_ref_receiver,
        out_ref: ck_out_ref_sender,
        out_crd: ck_out_crd_sender,
    };
    let ck_rdscanner = CompressedCrdRdScan::new(ck_data, c1_seg, c1_crd);

    // repeatsiggen
    let (out_repsig_j_sender, out_repsig_j_receiver) = parent.bounded(chan_size);
    let repsig_j_data = RepSigGenData::<u32, u32> {
        input: bc1_cj_out_ref_receiver,
        out_repsig: out_repsig_j_sender,
    };
    let repsig_j = RepeatSigGen::new(repsig_j_data);

    // repeat
    let (out_repeat_bj_sender, out_repeat_bj_receiver) = parent.bounded(chan_size);
    let bj_repeat_data = RepeatData::<u32, u32> {
        in_ref: bc1_bi_out_ref_receiver,
        in_repsig: out_repsig_j_receiver,
        out_ref: out_repeat_bj_sender,
    };
    let bj_repeat = Repeat::new(bj_repeat_data);

    // fiberlookup_bk
    let (bk_out_crd_sender, bk_out_crd_receiver) = parent.bounded(chan_size);
    let (bk_out_ref_sender, bk_out_ref_receiver) = parent.bounded(chan_size);
    let bk_data = RdScanData::<u32, u32> {
        in_ref: out_repeat_bj_receiver,
        out_ref: bk_out_ref_sender,
        out_crd: bk_out_crd_sender,
    };
    let bk_rdscanner = CompressedCrdRdScan::new(bk_data, b1_seg, b1_crd);

    let (intersectk_out_ref1_sender, intersectk_out_ref1_receiver) = parent.bounded(chan_size);
    let (intersectk_out_ref2_sender, intersectk_out_ref2_receiver) = parent.bounded(chan_size);
    let intersectk_data = CrdJoinerData::<u32, u32> {
        in_crd1: bk_out_crd_receiver,
        in_ref1: bk_out_ref_receiver,
        in_crd2: ck_out_crd_receiver,
        in_ref2: ck_out_ref_receiver,
        out_crd: parent.void(),
        out_ref1: intersectk_out_ref1_sender,
        out_ref2: intersectk_out_ref2_sender,
    };
    let intersect_k = Intersect::new(intersectk_data);

    // arrayvals_b
    let (b_out_val_sender, b_out_val_receiver) = parent.bounded(chan_size);
    let arrayvals_b_data = ArrayData::<u32, VT, u32> {
        in_ref: intersectk_out_ref1_receiver,
        out_val: b_out_val_sender,
    };
    let arrayvals_b = Array::<u32, VT, u32>::new(arrayvals_b_data, b_vals);

    // arrayvals_c
    let (c_out_val_sender, c_out_val_receiver) = parent.bounded(chan_size);
    let arrayvals_c_data = ArrayData::<u32, VT, u32> {
        in_ref: intersectk_out_ref2_receiver,
        out_val: c_out_val_sender,
    };
    let arrayvals_c = Array::<u32, VT, u32>::new(arrayvals_c_data, c_vals);

    // mul ALU
    let (mul_out_sender, mul_out_receiver) = parent.bounded(chan_size);
    let mul = make_alu(
        b_out_val_receiver,
        c_out_val_receiver,
        mul_out_sender,
        ALUMulOp(),
    );

    let (crddrop_valj_inner_sender, crddrop_valj_inner_receiver) = parent.bounded(chan_size);
    let (crddrop_valj_outer_sender, crddrop_valj_outer_receiver) = parent.bounded(chan_size);
    let crddrop_valj_data = CrdDropData::<VT, u32, u32> {
        in_crd_inner: mul_out_receiver,
        in_crd_outer: cj_out_crd_receiver,
        out_crd_inner: crddrop_valj_inner_sender,
        out_crd_outer: crddrop_valj_outer_sender,
    };
    let crddrop_valj = CrdDrop::new(crddrop_valj_data);
    let (stkn_drop_val_out_val_sender, stkn_drop_val_out_val_receiver) = parent.bounded(chan_size);
    let stkn_drop_val = StknDrop::new(crddrop_valj_inner_receiver, stkn_drop_val_out_val_sender);

    let (out_val_sender, out_val_receiver) = parent.bounded(chan_size);
    let reduce_data = ReduceData::<VT, u32> {
        in_val: stkn_drop_val_out_val_receiver,
        out_val: out_val_sender,
    };
    let red = Reduce::new(reduce_data);

    // fiberwrite_Xvals
    let xvals = ValsWrScan::<VT, u32>::new(out_val_receiver);

    let (crddrop_ij_inner_sender, crddrop_ij_inner_receiver) = parent.bounded(chan_size);
    let (crddrop_ij_outer_sender, crddrop_ij_outer_receiver) = parent.bounded(chan_size);
    let crddrop_ij_data = CrdDropData::<u32, u32, u32> {
        in_crd_inner: crddrop_valj_outer_receiver,
        in_crd_outer: bi_out_crd_receiver,
        out_crd_inner: crddrop_ij_inner_sender,
        out_crd_outer: crddrop_ij_outer_sender,
    };
    let crddrop_ij = CrdDrop::new(crddrop_ij_data);

    let (stkn_drop_j_out_val_sender, stkn_drop_j_out_val_receiver) = parent.bounded(chan_size);
    let stkn_drop_j = StknDrop::new(crddrop_ij_inner_receiver, stkn_drop_j_out_val_sender);

    // fiberwrite_x1
    // TODO: fix this
    let x1_wrscanner = CompressedWrScan::new(stkn_drop_j_out_val_receiver);

    // fiberwrite_X0
    let x0_wrscanner = CompressedWrScan::new(crddrop_ij_outer_receiver);

    let x_val = xvals.out_val.clone();
    let x1_crd = x1_wrscanner.crd_arr.clone();
    let x1_seg = x1_wrscanner.seg_arr.clone();
    let x0_crd = x0_wrscanner.crd_arr.clone();
    let x0_seg = x0_wrscanner.seg_arr.clone();

    parent.add_child(b_gen);
    parent.add_child(broadcast);
    parent.add_child(broadcast1);
    parent.add_child(c_gen);
    parent.add_child(bi_rdscanner);
    parent.add_child(repsig_i);
    parent.add_child(repsig_j);
    parent.add_child(ci_repeat);
    parent.add_child(ck_rdscanner);
    parent.add_child(cj_rdscanner);
    parent.add_child(bj_repeat);
    parent.add_child(bk_rdscanner);
    parent.add_child(intersect_k);
    parent.add_child(x0_wrscanner);
    parent.add_child(x1_wrscanner);
    parent.add_child(arrayvals_b);
    parent.add_child(arrayvals_c);
    parent.add_child(mul);
    parent.add_child(red);
    parent.add_child(xvals);
    parent.add_child(crddrop_valj);
    parent.add_child(stkn_drop_val);
    parent.add_child(crddrop_ij);
    parent.add_child(stkn_drop_j);

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

    return executed.elapsed_cycles();
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
        total_cycles += test_matmul_ijk(&subtile_abs_path).unwrap();
    }
    bar.finish();
    println!("Total cycles: {}", total_cycles);
}
