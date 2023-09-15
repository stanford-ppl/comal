use std::{fs, path::Path};

use comal::templates::accumulator::{Reduce, ReduceData};

use comal::templates::repeat::{RepSigGenData, Repeat, RepeatData, RepeatSigGen};
use comal::token_vec;
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};

use dam_rs::context::broadcast_context::BroadcastContext;
use dam_rs::context::generator_context::GeneratorContext;

use comal::config::Data;

use comal::templates::alu::make_alu;
use comal::templates::array::{Array, ArrayData};

use comal::templates::joiner::{CrdJoinerData, Intersect};
use comal::templates::primitive::{Repsiggen, Token};
use comal::templates::rd_scanner::{CompressedCrdRdScan, RdScanData};

use comal::templates::utils::read_inputs;
use dam_rs::simulation::Program;

use comal::templates::wr_scanner::{CompressedWrScan, ValsWrScan};
use dam_rs::templates::ops::ALUMulOp;

/*
let q0_seg = read_inputs::<u32>(&q0_seg_filename);
    let q0_crd = read_inputs::<u32>(&q0_crd_filename);
    let q1_seg = read_inputs::<u32>(&q1_seg_filename);
    let q1_crd = read_inputs::<u32>(&q1_crd_filename);
    let q2_seg = read_inputs::<u32>(&q2_seg_filename);
    let q2_crd = read_inputs::<u32>(&q2_crd_filename);
    let q3_seg = read_inputs::<u32>(&q3_seg_filename);
    let q3_crd = read_inputs::<u32>(&q3_crd_filename);
    let q_vals = read_inputs::<f32>(&q_vals_filename);

    let k0_seg = read_inputs::<u32>(&k0_seg_filename);
    let k0_crd = read_inputs::<u32>(&k0_crd_filename);
    let k1_seg = read_inputs::<u32>(&k1_seg_filename);
    let k1_crd = read_inputs::<u32>(&k1_crd_filename);
    let k2_seg = read_inputs::<u32>(&k2_seg_filename);
    let k2_crd = read_inputs::<u32>(&k2_crd_filename);
    let k3_seg = read_inputs::<u32>(&k3_seg_filename);
    let k3_crd = read_inputs::<u32>(&k3_crd_filename);
    let k_vals = read_inputs::<f32>(&k_vals_filename);

    let v0_seg = read_inputs::<u32>(&v0_seg_filename);
    let v0_crd = read_inputs::<u32>(&v0_crd_filename);
    let v1_seg = read_inputs::<u32>(&v1_seg_filename);
    let v1_crd = read_inputs::<u32>(&v1_crd_filename);
    let v2_seg = read_inputs::<u32>(&v2_seg_filename);
    let v2_crd = read_inputs::<u32>(&v2_crd_filename);
    let v3_seg = read_inputs::<u32>(&v3_seg_filename);
    let v3_crd = read_inputs::<u32>(&v3_crd_filename);
    let v_vals = read_inputs::<f32>(&v_vals_filename);
*/

#[derive(Clone)]
struct TestData {
    b0_seg: Vec<u32>,
    b0_crd: Vec<u32>,
    b1_seg: Vec<u32>,
    b1_crd: Vec<u32>,
    b_vals: Vec<f32>,

    c0_seg: Vec<u32>,
    c0_crd: Vec<u32>,
    c1_seg: Vec<u32>,
    c1_crd: Vec<u32>,
    c_vals: Vec<f32>,
}

fn load_data(test_name: &str) -> TestData {
    let filename = home::home_dir().unwrap().join("sam_config.toml");
    let contents = fs::read_to_string(filename).unwrap();
    let data: Data = toml::from_str(&contents).unwrap();
    let formatted_dir = data.sam_config.sam_path;
    let base_path = Path::new(&formatted_dir).join(&test_name);
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

    TestData {
        b0_seg: read_inputs(&b0_seg_filename),
        b0_crd: read_inputs(&b0_crd_filename),
        b1_seg: read_inputs(&b1_seg_filename),
        b1_crd: read_inputs(&b1_crd_filename),
        b_vals: read_inputs(&b_vals_filename),

        c0_seg: read_inputs(&c0_seg_filename),
        c0_crd: read_inputs(&c0_crd_filename),
        c1_seg: read_inputs(&c1_seg_filename),
        c1_crd: read_inputs(&c1_crd_filename),
        c_vals: read_inputs(&c_vals_filename),
    }
}

fn matmul<'a>(test_data: TestData, chan_size: usize, with_flavor: bool) -> Program<'a> {
    let mut parent = Program::default();

    let b0_crd = test_data.b0_crd;
    let b0_seg = test_data.b0_seg;
    let b1_crd = test_data.b1_crd;
    let b1_seg = test_data.b1_seg;
    let b_vals = test_data.b_vals;

    let c0_crd = test_data.c0_crd;
    let c0_seg = test_data.c0_seg;
    let c1_crd = test_data.c1_crd;
    let c1_seg = test_data.c1_seg;
    let c_vals = test_data.c_vals;

    // let mut parent = Program::default();

    let _mk_bounded = || parent.bounded::<Token<u32, u32>>(chan_size);
    let _mk_boundedf = || parent.bounded::<Token<f32, u32>>(chan_size);
    let _mk_rsiggen_bounded = || parent.bounded::<Repsiggen>(chan_size);

    // fiberlookup_bi
    let (bi_out_ref_sender, bi_out_ref_receiver) = parent.bounded(chan_size);
    let (bi_out_crd_sender, bi_out_crd_receiver) = parent.bounded(chan_size);
    let (bi_in_ref_sender, bi_in_ref_receiver) = parent.bounded(chan_size);
    // let (_bc_bi_in_ref_sender, _bc_bi_in_ref_receiver) = parent.bounded(chan_size);
    // let (_bc1_bi_in_ref_sender, _bc1_bi_in_ref_receiver) =
    //     parent.bounded(chan_size);

    let b_gen = GeneratorContext::new(
        || token_vec!(u32; u32; 0, "D").into_iter(),
        bi_in_ref_sender,
    );
    let bi_data = RdScanData::<u32, u32> {
        // in_ref: bc_bi_in_ref_receiver,
        in_ref: bi_in_ref_receiver,
        out_ref: bi_out_ref_sender,
        out_crd: bi_out_crd_sender,
    };

    let bi_rdscanner = CompressedCrdRdScan::new(bi_data, b0_seg, b0_crd);

    // fiberwrite_X0
    let x0_wrscanner = CompressedWrScan::new(bi_out_crd_receiver);

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
    let cj_rdscanner = CompressedCrdRdScan::new(cj_data, c1_seg, c1_crd);

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
    let ck_rdscanner = CompressedCrdRdScan::new(ck_data, c0_seg, c0_crd);

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

    // interset_i
    // let (intersecti_out_crd_sender, _intersecti_out_crd_receiver) =
    //     parent.bounded(chan_size);
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

    // fiberwrite_x1
    let x1_wrscanner = CompressedWrScan::new(cj_out_crd_receiver);

    // arrayvals_b
    let (b_out_val_sender, b_out_val_receiver) = parent.bounded(chan_size);
    let arrayvals_b_data = ArrayData::<u32, f32, u32> {
        in_ref: intersectk_out_ref1_receiver,
        out_val: b_out_val_sender,
    };
    let arrayvals_b = Array::<u32, f32, u32>::new(arrayvals_b_data, b_vals);

    // arrayvals_c
    let (c_out_val_sender, c_out_val_receiver) = parent.bounded(chan_size);
    let arrayvals_c_data = ArrayData::<u32, f32, u32> {
        in_ref: intersectk_out_ref2_receiver,
        out_val: c_out_val_sender,
    };
    let arrayvals_c = Array::<u32, f32, u32>::new(arrayvals_c_data, c_vals);

    // mul ALU
    let (mul_out_sender, mul_out_receiver) = parent.bounded(chan_size);
    let mul = make_alu(
        b_out_val_receiver,
        c_out_val_receiver,
        mul_out_sender,
        ALUMulOp(),
    );

    let (out_val_sender, out_val_receiver) = parent.bounded(chan_size);
    let reduce_data = ReduceData::<f32, u32> {
        in_val: mul_out_receiver,
        out_val: out_val_sender,
    };
    let red = Reduce::new(reduce_data);

    // fiberwrite_Xvals
    let xvals = ValsWrScan::<f32, u32>::new(out_val_receiver);

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

    parent.set_inference(with_flavor);
    parent.init();
    parent.run();

    parent
}

// pub fn mat_elemadd_benchmark_large(c: &mut Criterion) {
//     const CHAN_SIZE: usize = 1 << 10;
//     let data = load_data("mat_elemadd2");
//     let mut group = c.benchmark_group("mat_elemadd");
//     group.sample_size(10).bench_function("mat_elemadd", |b| {
//         b.iter(|| test_mat_elemadd(data.clone(), CHAN_SIZE));
//     });
// }

pub fn matmul_sweep(c: &mut Criterion) {
    let mut group = c.benchmark_group("matmul_ijk");
    let with_flavor = true;
    let dir_lst = vec![
        "matmul_100",
        "matmul_200",
        "matmul_300",
        "matmul_400",
        "matmul_500",
    ];
    for dir in dir_lst {
        // let chan_size = 1 << chan_factor;
        let data = load_data(dir);
        group.sample_size(10).bench_with_input(
            BenchmarkId::from_parameter(dir),
            &data,
            |b, data| {
                b.iter_batched(
                    || data.clone(),
                    |_cp| matmul(data.clone(), 2048, with_flavor),
                    BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

pub fn matmul_sweep_flavor(c: &mut Criterion) {
    let mut group = c.benchmark_group("matmul_flavor");
    let data = load_data("matmul_500");
    let flavors = vec![false, true];
    for with_flavor in flavors {
        // let chan_size = 1 << chan_factor;
        group.sample_size(10).bench_with_input(
            BenchmarkId::from_parameter(with_flavor),
            &with_flavor,
            |b, &with_flavor| {
                b.iter_batched(
                    || data.clone(),
                    |_cp| matmul(data.clone(), 2048, with_flavor),
                    BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

criterion_group!(matmul_benches, matmul_sweep,);
criterion_main!(matmul_benches);
