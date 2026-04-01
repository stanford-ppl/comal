/// Full end-to-end Flash Sliding Window Attention with dot product pipeline.
///
/// Dataflow per query row i attending to W key positions:
///
///   compressed_scan(window CSR, q_ref=i) → k_pos for each non-zero K position j:
///     dense_scan(head_dim) → d = 0,1,...,head_dim-1
///       array_val(Q, i*head_dim+d) → q_val
///       array_val(K, j*head_dim+d) → k_val
///       alu(mul, q_val, k_val) → prod
///     reduce(sum, prod) → raw_score
///     alu(mul, raw_score, 1/sqrt(d_k)) → scaled_score
///     array_val(V, j) → v_val (scalar, first element)
///     flash_softmax_accum(scaled_score, v_val) → [accumulates online]
///   Stop → flash_softmax_accum emits O[i] / l
///
/// This test wires the FULL pipeline in Comal/DAM and measures
/// cycle-accurate elapsed time including all pipeline stalls.

use dam::simulation::*;
use dam::utility_contexts::*;
use dam::templates::ops::*;

use comal::templates::accumulator::{
    FlashSoftmaxAccum, FlashSoftmaxAccumData, Reduce, ReduceData,
};
use comal::templates::alu::{make_alu, make_unary_alu};
use comal::templates::array::{Array, ArrayData};
use comal::templates::primitive::{Token, ALUExpOp};
use comal::templates::rd_scanner::{CompressedCrdRdScan, RdScanData, UncompressedCrdRdScan};
use comal::templates::repeat::{RepSigGenData, Repeat, RepeatData, RepeatSigGen};
use comal::token_vec;

fn run_full_flash_attention(seq_len: usize, head_dim: usize, window: usize) -> u64 {
    let scale = 1.0 / (head_dim as f32).sqrt();

    // Generate Q, K, V flat arrays
    let q_flat: Vec<f32> = (0..seq_len * head_dim)
        .map(|i| ((i % 127) as f32) * 0.01)
        .collect();
    let k_flat: Vec<f32> = (0..seq_len * head_dim)
        .map(|i| (((i + 3) % 131) as f32) * 0.01)
        .collect();
    // V: full head_dim per row (same layout as K)
    let v_flat: Vec<f32> = (0..seq_len * head_dim)
        .map(|i| (((i + 7) % 139) as f32) * 0.01)
        .collect();

    // Sliding window CSR
    let mut seg: Vec<u32> = vec![0];
    let mut crd: Vec<u32> = Vec::new();
    for i in 0..seq_len {
        let lo = if i >= window { i - window + 1 } else { 0 };
        for j in lo..=i {
            crd.push(j as u32);
        }
        seg.push(crd.len() as u32);
    }
    let total_nnz = crd.len();

    let chan_size = total_nnz * head_dim * 2 + 10000;
    let mut parent = ProgramBuilder::default();

    // ═══════════════════════════════════════════
    // Root → dense scan over query rows (d0)
    // ═══════════════════════════════════════════
    let (gen_snd, gen_rcv) = parent.bounded(chan_size);
    let gen = GeneratorContext::new(
        || token_vec!(u32; u32; 0, "D").into_iter(),
        gen_snd,
    );

    let (qrow_ref_snd, qrow_ref_rcv) = parent.bounded(chan_size);
    let (qrow_crd_snd, qrow_crd_rcv) = parent.bounded(chan_size);
    let qrow_scan = UncompressedCrdRdScan::new(
        RdScanData { in_ref: gen_rcv, out_ref: qrow_ref_snd, out_crd: qrow_crd_snd },
        seq_len as u32,
    );

    // ═══════════════════════════════════════════
    // Compressed scan over K positions (d1, sliding window)
    // ═══════════════════════════════════════════
    let (kpos_ref_snd, kpos_ref_rcv) = parent.bounded(chan_size);
    let (kpos_crd_snd, kpos_crd_rcv) = parent.bounded(chan_size);
    let kpos_scan = CompressedCrdRdScan::new(
        RdScanData { in_ref: qrow_ref_rcv, out_ref: kpos_ref_snd, out_crd: kpos_crd_snd },
        seg, crd,
    );

    // Broadcast kpos_crd to: (1) V inner scan, (2) repsig for Q repeat, (3) K inner scan
    let (bc_kc1_snd, bc_kc1_rcv) = parent.bounded(chan_size); // V inner scan base
    let (bc_kc2_snd, bc_kc2_rcv) = parent.bounded(chan_size); // repsig for Q repeat
    let (bc_kc3_snd, bc_kc3_rcv) = parent.bounded(chan_size); // K inner scan base
    let mut bc_kcrd = BroadcastContext::new(kpos_crd_rcv);
    bc_kcrd.add_target(bc_kc1_snd);
    bc_kcrd.add_target(bc_kc2_snd);
    bc_kcrd.add_target(bc_kc3_snd);

    // ═══════════════════════════════════════════
    // Dense inner scan (d2, head_dim) for K
    // Each K position j → emit refs j*head_dim+0, j*head_dim+1, ..., j*head_dim+(head_dim-1)
    // ═══════════════════════════════════════════
    let (k_inner_ref_snd, k_inner_ref_rcv) = parent.bounded(chan_size);
    let (k_inner_crd_snd, k_inner_crd_rcv) = parent.bounded(chan_size);
    let k_inner_scan = UncompressedCrdRdScan::new(
        RdScanData { in_ref: bc_kc3_rcv, out_ref: k_inner_ref_snd, out_crd: k_inner_crd_snd },
        head_dim as u32,
    );

    // Array val for K: reads k_flat[ref]
    let (k_val_snd, k_val_rcv) = parent.bounded(chan_size);
    let k_array = Array::new(
        ArrayData { in_ref: k_inner_ref_rcv, out_val: k_val_snd, block_size: 1 },
        k_flat,
    );

    // ═══════════════════════════════════════════
    // RepeatSigGen from K inner crd → drives Q repeat
    // ═══════════════════════════════════════════
    let (repsig_snd, repsig_rcv) = parent.bounded(chan_size);
    let repsig = RepeatSigGen::new(RepSigGenData {
        input: bc_kc2_rcv,
        out_repsig: repsig_snd,
    });

    // Broadcast repsig to: (1) Q repeat
    // (repsig is consumed by repeat, only need 1 copy)

    // Repeat Q row ref across K positions
    let (q_rep_ref_snd, q_rep_ref_rcv) = parent.bounded(chan_size);
    let q_repeat = Repeat::new(RepeatData {
        in_ref: qrow_crd_rcv,  // q row index
        in_repsig: repsig_rcv,
        out_ref: q_rep_ref_snd,
    });

    // Dense inner scan for Q (same head_dim)
    let (q_inner_ref_snd, q_inner_ref_rcv) = parent.bounded(chan_size);
    let (q_inner_crd_snd, q_inner_crd_rcv) = parent.bounded(chan_size);
    let q_inner_scan = UncompressedCrdRdScan::new(
        RdScanData { in_ref: q_rep_ref_rcv, out_ref: q_inner_ref_snd, out_crd: q_inner_crd_snd },
        head_dim as u32,
    );

    // Array val for Q
    let (q_val_snd, q_val_rcv) = parent.bounded(chan_size);
    let q_array = Array::new(
        ArrayData { in_ref: q_inner_ref_rcv, out_val: q_val_snd, block_size: 1 },
        q_flat,
    );

    // ═══════════════════════════════════════════
    // Dot product: alu(mul, q_val, k_val) → reduce(sum) → score
    // ═══════════════════════════════════════════
    let (mul_out_snd, mul_out_rcv) = parent.bounded(chan_size);
    let mul = make_alu(q_val_rcv, k_val_rcv, mul_out_snd, ALUMulOp());

    let (score_snd, score_rcv) = parent.bounded(chan_size);
    let reduce = Reduce::<f32, u32, 1>::new(ReduceData {
        in_val: mul_out_rcv,
        out_val: score_snd,
        sum: false,
    });

    // ═══════════════════════════════════════════
    // V lookup: inner dimension scan (same structure as K)
    // V[j] has head_dim elements: v_flat[j*head_dim .. (j+1)*head_dim]
    // ═══════════════════════════════════════════
    let (v_inner_ref_snd, v_inner_ref_rcv) = parent.bounded(chan_size);
    let (v_inner_crd_snd, v_inner_crd_rcv) = parent.bounded(chan_size);
    let v_inner_scan = UncompressedCrdRdScan::new(
        RdScanData { in_ref: bc_kc1_rcv, out_ref: v_inner_ref_snd, out_crd: v_inner_crd_snd },
        head_dim as u32,
    );

    let (v_val_snd, v_val_rcv) = parent.bounded(chan_size);
    let v_array = Array::new(
        ArrayData { in_ref: v_inner_ref_rcv, out_val: v_val_snd, block_size: 1 },
        v_flat,
    );

    // ═══════════════════════════════════════════
    // FlashSoftmaxAccum(score, v_val) → output
    // Two-rate protocol:
    //   in_score: 1 score per K position (after reduce)
    //   in_val: head_dim V elements per K position (with inner Stops between K positions)
    // The accum consumes 1 score, then head_dim V elements, updates online softmax once
    // per score and accumulates O element-wise.
    // ═══════════════════════════════════════════
    let (out_snd, out_rcv) = parent.bounded(chan_size);
    let flash = FlashSoftmaxAccum::with_head_dim(
        FlashSoftmaxAccumData {
            in_score: score_rcv,
            in_val: v_val_rcv,
            out_val: out_snd,
        },
        head_dim,
    );

    // Drain outputs and unused streams
    let drain_out = ConsumerContext::new(out_rcv);
    let drain_k_crd = ConsumerContext::new(k_inner_crd_rcv);
    let drain_q_crd = ConsumerContext::new(q_inner_crd_rcv);
    let drain_v_crd = ConsumerContext::new(v_inner_crd_rcv);
    let drain_kpos_ref = ConsumerContext::new(kpos_ref_rcv);

    // ═══════════════════════════════════════════
    // Wire everything
    // ═══════════════════════════════════════════
    parent.add_child(gen);
    parent.add_child(qrow_scan);
    parent.add_child(kpos_scan);
    parent.add_child(bc_kcrd);
    parent.add_child(k_inner_scan);
    parent.add_child(k_array);
    parent.add_child(repsig);
    parent.add_child(q_repeat);
    parent.add_child(q_inner_scan);
    parent.add_child(q_array);
    parent.add_child(mul);
    parent.add_child(reduce);
    parent.add_child(v_inner_scan);
    parent.add_child(v_array);
    parent.add_child(flash);
    parent.add_child(drain_out);
    parent.add_child(drain_k_crd);
    parent.add_child(drain_q_crd);
    parent.add_child(drain_v_crd);
    parent.add_child(drain_kpos_ref);

    let executed = parent
        .initialize(InitializationOptions::default())
        .unwrap()
        .run(RunOptionsBuilder::default()
            .mode(RunMode::Simple)
            .build()
            .unwrap());

    executed.elapsed_cycles().unwrap()
}

#[test]
fn test_full_flash_small() {
    println!("\n=== Full Flash Attention (small) ===");
    let cycles = run_full_flash_attention(8, 4, 4);
    println!("  seq=8, head_dim=4, window=4: {} cycles", cycles);
    assert!(cycles > 0);
}

#[test]
fn test_full_flash_scaling() {
    println!("\n=== Full Flash Attention Scaling ===");
    let head_dim = 16; // keep small for test speed

    println!("  {:>6} {:>6} {:>8} {:>10} {:>12} {:>10}",
             "Seq", "HdDim", "Window", "NNZ", "Cycles", "Cyc/NNZ");

    for &(seq, window) in &[
        (32, 16),
        (64, 32),
        (128, 64),
        (256, 128),
    ] {
        let cycles = run_full_flash_attention(seq, head_dim, window);
        let nnz: usize = (0..seq).map(|i| {
            let lo = if i >= window { i - window + 1 } else { 0 };
            i - lo + 1
        }).sum();
        let cyc_per_nnz = cycles as f64 / nnz as f64;
        println!("  {:>6} {:>6} {:>8} {:>10} {:>12} {:>10.1}",
                 seq, head_dim, window, nnz, cycles, cyc_per_nnz);
    }
    println!("\n  Expected cyc/nnz ≈ head_dim + overhead (dot product dominates)");
    println!("  head_dim={}: expect ~{} cyc/nnz", head_dim, head_dim + 5);
}

#[test]
fn test_full_flash_dense_vs_sparse() {
    let seq = 64;
    let head_dim = 8;
    let window = 16;

    let dense_cycles = run_full_flash_attention(seq, head_dim, seq);
    let sparse_cycles = run_full_flash_attention(seq, head_dim, window);

    let dense_nnz = seq * (seq + 1) / 2;
    let sparse_nnz: usize = (0..seq).map(|i| {
        let lo = if i >= window { i - window + 1 } else { 0 };
        i - lo + 1
    }).sum();

    println!("\n=== Full Dense vs Sparse Flash Attention ===");
    println!("  seq={}, head_dim={}, window={}", seq, head_dim, window);
    println!("  Dense:  {} nnz, {} cycles, {:.1} cyc/nnz",
             dense_nnz, dense_cycles, dense_cycles as f64 / dense_nnz as f64);
    println!("  Sparse: {} nnz, {} cycles, {:.1} cyc/nnz",
             sparse_nnz, sparse_cycles, sparse_cycles as f64 / sparse_nnz as f64);
    println!("  NNZ reduction:   {:.1}x", dense_nnz as f64 / sparse_nnz as f64);
    println!("  Cycle reduction: {:.1}x", dense_cycles as f64 / sparse_cycles as f64);
}

#[test]
fn test_full_flash_head128() {
    let head_dim = 128; // Mistral-7B head dimension
    println!("\n=== Full Flash Attention: head_dim=128 (Mistral-7B) ===");
    println!("  {:>6} {:>6} {:>8} {:>10} {:>12} {:>10}",
             "Seq", "HdDim", "Window", "NNZ", "Cycles", "Cyc/NNZ");

    for &(seq, window) in &[
        (32, 16),
        (64, 32),
        (128, 64),
        (256, 128),
    ] {
        let cycles = run_full_flash_attention(seq, head_dim, window);
        let nnz: usize = (0..seq).map(|i| {
            let lo = if i >= window { i - window + 1 } else { 0 };
            i - lo + 1
        }).sum();
        let cyc_per_nnz = cycles as f64 / nnz as f64;
        println!("  {:>6} {:>6} {:>8} {:>10} {:>12} {:>10.1}",
                 seq, head_dim, window, nnz, cycles, cyc_per_nnz);
    }
    println!("\n  Expected: ~129 cyc/nnz (128 dot product + 1 flash accum)");
}
