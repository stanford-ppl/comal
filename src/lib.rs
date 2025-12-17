#![allow(dead_code)]

use pyo3::{prelude::*, types::PyTuple};

use std::{fs, panic};

use dam::simulation::*;
use prost::Message;
use proto_driver::{
    parse_proto, parse_proto_block16, parse_proto_block32, parse_proto_block64,
    parse_proto_true_block16, parse_proto_true_block32, parse_proto_true_block64,
    proto_headers::tortilla::ComalGraph
};

pub mod cli_common;
pub mod config;
pub mod proto_driver;
pub mod templates;
pub mod utils;


/// Runs proto graph given data and returns elapsed cycles (scalar mode, block_size=1)
#[pyfunction]
fn run_graph(proto: String, data: String) -> PyResult<(bool, u64)> {
    let comal_graph = {
        let file_contents = fs::read(proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto(comal_graph, data.into(), Default::default());
    let initialized = program_builder.initialize(Default::default()).unwrap();
    println!("{}", initialized.to_dot_string());
    let executed = initialized.run(Default::default());
    let cycles = executed.elapsed_cycles().unwrap();
    let passed = executed.passed();
    let return_tuple = (passed, cycles);
    println!("Elapsed Cycles: {}", cycles);
    return Ok(return_tuple);
}

/// Runs proto graph with block_size=16 (for block sparse mode)
#[pyfunction]
fn run_graph_block16(proto: String, data: String) -> PyResult<(bool, u64)> {
    let comal_graph = {
        let file_contents = fs::read(proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto_block16(comal_graph, data.into(), Default::default());
    let initialized = program_builder.initialize(Default::default()).unwrap();
    println!("{}", initialized.to_dot_string());
    let executed = initialized.run(Default::default());
    let cycles = executed.elapsed_cycles().unwrap();
    let passed = executed.passed();
    let return_tuple = (passed, cycles);
    println!("Elapsed Cycles (block16): {}", cycles);
    return Ok(return_tuple);
}

/// Runs proto graph with block_size=32 (for block sparse mode)
#[pyfunction]
fn run_graph_block32(proto: String, data: String) -> PyResult<(bool, u64)> {
    let comal_graph = {
        let file_contents = fs::read(proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto_block32(comal_graph, data.into(), Default::default());
    let initialized = program_builder.initialize(Default::default()).unwrap();
    println!("{}", initialized.to_dot_string());
    let executed = initialized.run(Default::default());
    let cycles = executed.elapsed_cycles().unwrap();
    let passed = executed.passed();
    let return_tuple = (passed, cycles);
    println!("Elapsed Cycles (block32): {}", cycles);
    return Ok(return_tuple);
}

/// Runs proto graph with block_size=64 (for block sparse mode)
#[pyfunction]
fn run_graph_block64(proto: String, data: String) -> PyResult<(bool, u64)> {
    let comal_graph = {
        let file_contents = fs::read(proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto_block64(comal_graph, data.into(), Default::default());
    let initialized = program_builder.initialize(Default::default()).unwrap();
    println!("{}", initialized.to_dot_string());
    let executed = initialized.run(Default::default());
    let cycles = executed.elapsed_cycles().unwrap();
    let passed = executed.passed();
    let return_tuple = (passed, cycles);
    println!("Elapsed Cycles (block64): {}", cycles);
    return Ok(return_tuple);
}

/// Runs proto graph with true block sparse mode (16x16 dense blocks, BCSR format)
#[pyfunction]
fn run_graph_true_block16(proto: String, data: String) -> PyResult<(bool, u64)> {
    let comal_graph = {
        let file_contents = fs::read(proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto_true_block16(comal_graph, data.into(), Default::default());
    let initialized = program_builder.initialize(Default::default()).unwrap();
    println!("{}", initialized.to_dot_string());
    let executed = initialized.run(Default::default());
    let cycles = executed.elapsed_cycles().unwrap();
    let passed = executed.passed();
    let return_tuple = (passed, cycles);
    println!("Elapsed Cycles (true_block16): {}", cycles);
    return Ok(return_tuple);
}

/// Runs proto graph with true block sparse mode (32x32 dense blocks, BCSR format)
#[pyfunction]
fn run_graph_true_block32(proto: String, data: String) -> PyResult<(bool, u64)> {
    let comal_graph = {
        let file_contents = fs::read(proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto_true_block32(comal_graph, data.into(), Default::default());
    let initialized = program_builder.initialize(Default::default()).unwrap();
    println!("{}", initialized.to_dot_string());
    let executed = initialized.run(Default::default());
    let cycles = executed.elapsed_cycles().unwrap();
    let passed = executed.passed();
    let return_tuple = (passed, cycles);
    println!("Elapsed Cycles (true_block32): {}", cycles);
    return Ok(return_tuple);
}

/// Runs proto graph with true block sparse mode (64x64 dense blocks, BCSR format)
#[pyfunction]
fn run_graph_true_block64(proto: String, data: String) -> PyResult<(bool, u64)> {
    let comal_graph = {
        let file_contents = fs::read(proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto_true_block64(comal_graph, data.into(), Default::default());
    let initialized = program_builder.initialize(Default::default()).unwrap();
    println!("{}", initialized.to_dot_string());
    let executed = initialized.run(Default::default());
    let cycles = executed.elapsed_cycles().unwrap();
    let passed = executed.passed();
    let return_tuple = (passed, cycles);
    println!("Elapsed Cycles (true_block64): {}", cycles);
    return Ok(return_tuple);
}

/// A Python module implemented in Rust.
#[pymodule]
fn comal(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_graph, m)?)?;
    m.add_function(wrap_pyfunction!(run_graph_block16, m)?)?;
    m.add_function(wrap_pyfunction!(run_graph_block32, m)?)?;
    m.add_function(wrap_pyfunction!(run_graph_block64, m)?)?;
    m.add_function(wrap_pyfunction!(run_graph_true_block16, m)?)?;
    m.add_function(wrap_pyfunction!(run_graph_true_block32, m)?)?;
    m.add_function(wrap_pyfunction!(run_graph_true_block64, m)?)?;
    m.add("PanicError", py.get_type::<pyo3::panic::PanicException>())?;
    Ok(())
}
