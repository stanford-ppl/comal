use std::time::Instant;
use std::{fs, path::Path};

use comal::templates::joiner::Union;

use comal::token_vec;
use criterion::{criterion_group, criterion_main, Criterion};

use dam_rs::context::generator_context::GeneratorContext;

use comal::config::Data;

use comal::templates::alu::make_alu;
use comal::templates::array::{Array, ArrayData};

use comal::templates::joiner::CrdJoinerData;
use comal::templates::primitive::Token;
use comal::templates::rd_scanner::{CompressedCrdRdScan, RdScanData};

use comal::templates::utils::read_inputs;
use dam_rs::simulation::Program;
use dam_rs::templates::ops::ALUAddOp;

use comal::templates::wr_scanner::{CompressedWrScan, ValsWrScan};

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

fn test_mat_elemadd<'a>(test_data: TestData, chan_size: usize) -> ProgramBuilder<'a> {
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
    let (b_out_val_sender, b_out_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
    let arrayvals_b_data = ArrayData::<u32, f32, u32> {
        in_ref: unionj_out_ref1_receiver,
        out_val: b_out_val_sender,
    };
    let arrayvals_b = Array::<u32, f32, u32>::new(arrayvals_b_data, b_vals);

    // arrayvals_c
    let (c_out_val_sender, c_out_val_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
    let arrayvals_c_data = ArrayData::<u32, f32, u32> {
        in_ref: unionj_out_ref2_receiver,
        out_val: c_out_val_sender,
    };
    let arrayvals_c = Array::<u32, f32, u32>::new(arrayvals_c_data, c_vals);

    // Add ALU
    let (add_out_sender, add_out_receiver) = parent.bounded::<Token<f32, u32>>(chan_size);
    let add = make_alu(
        b_out_val_receiver,
        c_out_val_receiver,
        add_out_sender,
        ALUAddOp(),
    );

    // fiberwrite_Xvals
    let xvals = ValsWrScan::<f32, u32>::new(add_out_receiver);

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
    parent.add_child(add);
    parent.add_child(xvals);

    let now = Instant::now();
    parent.set_inference(true);
    parent.print_graph();
    parent.init();
    let elapsed = now.elapsed();
    println!("Elapsed: {:.2?}", elapsed);
    parent.run();

    parent
}

pub fn mat_elemadd_benchmark_large(c: &mut Criterion) {
    const CHAN_SIZE: usize = 1 << 10;
    let data = load_data("mat_elemadd2");
    let mut group = c.benchmark_group("mat_elemadd");
    group.sample_size(10).bench_function("mat_elemadd", |b| {
        b.iter(|| test_mat_elemadd(data.clone(), CHAN_SIZE));
    });
}

// pub fn mha_par_benchmark_channels(c: &mut Criterion) {
//     let mut group = c.benchmark_group("MHA_chan_sweep");
//     let data = load_data("mat_elemadd2");
//     for chan_factor in 5..12 {
//         let chan_size = 1 << chan_factor;
//         group.bench_with_input(
//             BenchmarkId::from_parameter(chan_size),
//             &chan_size,
//             |b, &chan_size| {
//                 b.iter_batched(
//                     || data.clone(),
//                     |cp| test_par_multihead_attention(cp, chan_size),
//                     BatchSize::LargeInput,
//                 );
//             },
//         );
//     }
//     group.finish();
// }

criterion_group!(elemadd_benches, mat_elemadd_benchmark_large,);
criterion_main!(elemadd_benches);
