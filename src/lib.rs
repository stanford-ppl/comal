#![allow(dead_code)]

use pyo3::prelude::*;

use std::{fs};

use dam::{simulation::*};
use prost::Message;
use proto_driver::{parse_proto, proto_headers::tortilla::ComalGraph};

pub mod cli_common;
pub mod config;
pub mod proto_driver;
pub mod templates;
pub mod utils;

/// Runs proto graph given data and returns elapsed cycles
#[pyfunction]
fn run_graph(proto : String, data : String) -> PyResult<u64> {
    let comal_graph = {
        let file_contents = fs::read(proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto(comal_graph, data.into(), Default::default());
    let initialized = program_builder.initialize(Default::default()).unwrap();
    println!("{}", initialized.to_dot_string());
    let executed = initialized.run(Default::default());
    let cycles = executed.elapsed_cycles().unwrap();
    println!("Elapsed Cycles: {}", cycles);
    return Ok(cycles)
}

/// A Python module implemented in Rust.
#[pymodule]
fn comal(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_graph, m)?)?;
    Ok(())
}

