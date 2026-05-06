# Tiling support for comal-samml-cosim

This branch (`samml-cosim-tiling`, off `samml-cosim`) adds tile-aware execution
semantics to the comal dataflow simulator. The goal is to support multi-level
tiled SpMM/matmul/GNN workloads where the iteration nest is split-and-reordered
into outer (tile-bucket) and inner (within-tile) loops, with cycle-counts that
reflect real streaming-tile pipelining rather than a flat-graph approximation.

## Why this is small (the headline)

Most comal templates are **depth-agnostic** — they pass `Stop(k)` tokens through
unchanged regardless of the value of `k`. Pipelining across tile iterations
falls out automatically from streaming dataflow: tile-token N+1 can flow as
soon as tile-token N has been emitted, and all the joiners / ALUs / Arrays /
WrScans handle deeper Stop nesting without code changes.

Only **three node families** are depth-aware and need explicit audit:

| Node | File | Hardcoded behavior | Fix |
|------|------|---------------------|-----|
| `Reduce` | `src/templates/accumulator.rs:176` | `Stop(stkn-1)` always decrements | Conditional on `stkn < tile_depth` |
| `Spacc1` / `Spacc2` | `src/templates/accumulator.rs:330,369-370 / 524-750` | Flushes on every Stop | Flush only when `stkn < tile_depth` |
| `*CrdRdScan` | `src/templates/rd_scanner.rs:260, 740` | `Stop(stop_tkn+1)` always increments | No code change — relies on the data-gen reshape (`mode_shape = [N/T, T]`) so the +1 produces the right depth automatically |

The hard part is the **per-token Stop-depth bookkeeping** at the tile boundary.
Get it right once in Spacc/Reduce, and the rest of the graph pipelines for
free.

## Token model recap (from `src/templates/primitive.rs:14-19`)

```rust
pub enum Token<ValType, StopType> {
    Val(ValType),
    Stop(StopType),     // u32 depth-from-innermost: Stop(0) = innermost-fiber-end
    Empty,              // pipeline placeholder
    Done,               // stream terminator
}
```

`Stop(k)` semantics: `k=0` is the innermost fiber boundary; larger `k` is
progressively outer. **Adding a tile level introduces an extra Stop level
above all existing ones**, so every original `Stop(k)` from the inner subgraph
becomes effectively a fiber-internal token, and a new `Stop(tile_depth)`
appears at tile boundaries.

Three observed depth behaviors today:

- **Increment on emit** (+1): `UncompressedCrdRdScan`, `CompressedCrdRdScan`,
  `Repeat`. The scanner introduces a level so its output Stop is one deeper
  than its input parent.
- **Decrement on emit** (-1): `Reduce`. Collapses a level (sum reduction
  produces one Val per fiber, no Stop at that level).
- **Pass-through**: everything else. `Spacc1/2` emits unchanged Stops but
  flushes its accumulator state on every Stop — that's the load-bearing
  semantic mismatch with tiling.

## Phased plan

### Phase 2a — Proto schema additions, default-zero behavior unchanged

**Status: TODO**

1. Add fields to `tortilla/proto/ops.proto`:
   ```protobuf
   message Reduce {
       // existing fields ...
       uint32 tile_depth = N;     // 0 = pre-tiling behavior (every Stop decrements)
   }
   message Spacc {
       // existing fields ...
       uint32 tile_depth = N;     // 0 = pre-tiling behavior (every Stop flushes)
   }
   message FiberLookup {
       // existing fields ...
       uint64 tile_size = N;      // 0 = no tile metadata
       uint32 tile_role = N;      // 0=None, 1=Outer, 2=Inner
   }
   ```
2. Update `tortilla/src/proto_driver/mod.rs` to read the new fields and pass them
   to the corresponding template constructors.
3. **Default 0 preserves all existing tests verbatim.** This is the regression
   gate.
4. Acceptance: `cargo test` passes; existing graphs round-trip identically.

**Cost:** ~80 LoC, ~2 days.

### Phase 2b — `Spacc1` / `Spacc2` / `Reduce` tile-aware logic

**Status: TODO** — load-bearing for cycle correctness on tiled workloads.

Code shape for `Spacc1` (`accumulator.rs:251-421`, mirror for `Spacc2`):

```rust
// Existing:
Token::Stop(stkn) => {
    flush_and_emit_pairs();
    emit Token::Stop(stkn);
}

// Proposed:
Token::Stop(stkn) if stkn < self.tile_depth => {
    // Reduction-fiber-end: flush state, emit accumulated (crd, val) pairs
    flush_and_emit_pairs();
    emit Token::Stop(stkn);
}
Token::Stop(stkn) /* if stkn >= self.tile_depth */ => {
    // Tile boundary: state persists across tiles; just forward
    emit Token::Stop(stkn);
}
```

Code shape for `Reduce` (`accumulator.rs:142-188`, replacing the
unconditional `Stop(stkn-1)` at line 176):

```rust
match token {
    Token::Val(v) => self.sum += v,
    Token::Stop(stkn) if stkn < self.tile_depth => {
        emit Token::Val(self.sum); self.sum = 0;
        emit Token::Stop(stkn - 1);   // legitimate level-collapse
    }
    Token::Stop(stkn) => {
        emit Token::Stop(stkn);        // tile boundary — pass through, no flush
    }
    ...
}
```

`tile_depth = 0` (default) → every Stop has `stkn < 0` false (since `stkn >= 0`),
so **all Stops fall into the second branch**... no, wait — `stkn < 0` is never
true. Need to flip the default:

```rust
// tile_depth == 0 sentinel = "untiled, every Stop is a reduction Stop"
let is_tile_boundary = self.tile_depth > 0 && stkn >= self.tile_depth;
```

That's the right semantics. Default `tile_depth=0` → `is_tile_boundary` is
always false → preserves untiled behavior exactly.

Tests to add:
- Spacc reduction-only (untiled): existing tests at `accumulator.rs:1008+`
  must pass byte-identically.
- Spacc with `tile_depth=2`, feed `Val Val Stop(0) Val Val Stop(0) Stop(1)
  Stop(2)`: assert flush at the two `Stop(0)` events (reduction-fiber-ends),
  pass-through at `Stop(1)` (tile-internal), pass-through at `Stop(2)`
  (tile-boundary).
- Reduce same patterns; assert decrement happens only on Stop(0)/Stop(1),
  not on Stop(2).
- Pipelining test: feed two consecutive tiles' worth of tokens. Verify the
  outer scanner can emit tile_id=1's tokens before tile_id=0's Spacc has
  finished emitting its accumulator pairs (i.e., back-pressure only at the
  Spacc itself, not at every node upstream).

**Cost:** ~200 LoC + ~200 LoC tests, ~1 week.

### Phase 2c — Compiler-side wiring (samml repo)

**Status: depends on Phase 2a + 2b landing here**

In `samml/lib/Target/ProtoEmitter/`:
1. Populate `tile_size` / `tile_role` on `FiberLookup` proto from
   `sam.fiber_lookup` op attrs (already on the op via Phase 1 of the samml-side
   work, commit `c9ea419`).
2. Compute `tile_depth` for each `Spacc` / `Reduce` proto: the depth is the
   number of tile-introduced Stop levels between the reduction var and the
   outermost stream. Concretely: `tile_depth = (number of tile-vars that come
   AFTER this op's reduction var in the loop order)`.
3. Data-gen (`autosparse/tensor_data_generator.py`): when a dense tensor's
   arg has `samml.tile_sizes`, write `mode_shape` as `[N/T, T]` instead of
   `[N]`. The actual values bytes are unchanged (row-major reshape).

**Cost:** ~150 LoC, ~3 days.

### Phase 3 — End-to-end integration test

**Status: depends on 2a-2c**

A standalone test in `tests/test_tiled_spmm.rs` (or a comal-side fixture):
1. Hand-build a proto for a tiled SpMM with `tile_depth` set on Spacc.
2. Feed in tensor data with the reshaped `mode_shape`.
3. Run comal, assert cycle count is *less than or equal to* the untiled
   equivalent.
4. Cycle count should also be *roughly equal* on a workload where pipelining
   gives no benefit (e.g., serial reductions).

The Phase 3 acceptance is more nuanced than a single-number compare — we want:
- Untiled cycles ≈ tiled-with-tile_depth=0 cycles (regression check).
- Tiled-with-correct-tile_depth on a parallel-dim-tile workload: shows
  measurable cycle reduction proportional to pipelining-window size.
- Tiled-with-incorrect-tile_depth (e.g., off-by-one): correctness fails (wrong
  output values), surfacing the bug at the first integration test.

**Cost:** ~300 LoC tests, ~3-5 days.

### Phase 4 — Outer-tile fusion across phases (`isShared`)

**Status: depends on Phase 3 stable**

Compiler-side (samml) emits a single shared outer `tile_i` fiber_lookup whose
output ref drives BOTH phases of an spmm-then-spmm chain. This is what the
factory's `isShared = true` flag in `tilingInfo` is for. Streamingly, no
intermediate-tensor materialization to memory under the shared tile.

Simulator-side: ensure the WrScan + ArrayVal of the intermediate tensor handle
the shared outer-Stop correctly (the Spacc tile_depth from Phase 2b already
covers Spacc; ArrayVal is depth-agnostic and should pass through; WrScan's
fiber-segment writes need to honor the shared tile boundary).

**Cost:** ~250 LoC sim + factory wiring, ~3 days.

### Phase 5 — Format-tiled sparse (BCSR) — separate, larger effort

**Status: future**

1. Activate `TileRdScan` (`src/templates/rd_scanner.rs:348-492`, currently a
   complete struct but not gated in `proto_driver/mod.rs`).
2. Data-gen produces tile-row-pointer + within-tile-CSR arrays.
3. Proto: new `tile_format: "block_csr"` on FiberLookup or new
   `BlockSparseFiberLookup` message.

**Cost:** ~600 LoC sim + significant data-gen, ~2 weeks.

## Time bombs to defuse

| Location | Concern |
|----------|---------|
| `accumulator.rs:176` | `Stop(stkn-1)` literal — fixed in Phase 2b. |
| `accumulator.rs:330, 369-370` | "any Stop = flush" in Spacc1 — fixed in Phase 2b. |
| `accumulator.rs:524-750` | Spacc2 same — fixed in Phase 2b. |
| `accumulator.rs:1008-1549` | Test fixtures hardcode `Stop(0)`/`Stop(1)`. Add new tile-aware tests; leave originals as untiled regression. |
| `rd_scanner.rs:233-244` | `COMAL_VECTOR_MODE` env var skips innermost Stops. Composition order with tiling: vector is innermost, tile is outermost. Document the depth math: `effective_depth = nominal_depth + tile_levels - vector_skip`. |

## Open questions

1. **Multiple reduction levels in one Spacc.** If a future workload has
   nested reductions (e.g., reduce j inside reduce m), `tile_depth` is not
   sufficient — need a per-Spacc `reduction_depth_set`. Defer until needed.
2. **`tile_depth` with format-tiled sparse (BCSR).** When the outer scanner
   is a `TileRdScan` rather than a reshaped UncompressedCrdRdScan, the Stop
   semantics may differ. Audit when activating in Phase 5.
3. **`isShared` semantics across more than two phases.** Current factory
   handles linear phase chains. Multi-fork dataflow (one tensor consumed by
   multiple downstream ops) with shared tiles may need refinement.

## Validation strategy summary

- **Per-node unit tests** (Phase 2b): hand-crafted token streams with explicit
  Stop depths; assert correct flush/pass-through behavior.
- **Default-zero regression**: every existing `cargo test` passes byte-identically
  with new code paths gated by `tile_depth > 0`.
- **Integration test on real SpMM** (Phase 3): tiled vs untiled cycle counts
  agree on the untiled-tile-depth=0 case; tiled-with-correct-depth shows
  measurable speedup on parallel-dim tiling.
- **No silent corruption**: Phase 3's correctness assertion (compare output
  values, not just cycles) catches off-by-one Stop-depth bugs.

## Working tree

Branch `samml-cosim-tiling` off `samml-cosim` (latest:
`b4f0616 SAMML cosimulation: flash attention, vector tokens, parallel writes`).

The samml-side work that complements this:
- Phase 0 (TilingVisitor in factory backend): commit `8982866` on
  `samml/autosparse-new`.
- Phase 1 (`tile_size`/`tile_role` on `sam.fiber_lookup`): commit `c9ea419`
  on `samml/autosparse-new`.
- ProtoEmitter wiring + data-gen reshape: TODO (samml-side Phase 2c above).

Run order across the two repos for a full end-to-end:
1. comal: Phase 2a + 2b + tests pass.
2. samml: Phase 2c (ProtoEmitter populates new fields) + data-gen reshape.
3. End-to-end integration test (Phase 3) on a tiled SpMM MLIR fixture.
