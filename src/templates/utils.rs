use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use dam::types::DAMType;

use super::tensor::Adapter;

fn set_tensor_path() {
    env::set_var("FROSTT_FORMATTED_PATH", "/home/rubensl/Documents/data");
}

pub fn read_inputs_vectorized<T>(file_path: &PathBuf, prim_type: impl Adapter<T>) -> Vec<T>
where
    T: DAMType,
{
    let file =
        File::open(file_path).unwrap_or_else(|_| panic!("file {:?} wasn't found", file_path));
    // prim_type.parse(BufReader::new(file).lines().flatten())
    prim_type.parse(BufReader::new(file).lines().flatten())
}

pub fn read_inputs<T>(file_path: &PathBuf) -> Vec<T>
where
    T: DAMType + std::str::FromStr,
{
    let file =
        File::open(file_path).unwrap_or_else(|_| panic!("file {:?} wasn't found.", file_path));
    let reader = BufReader::new(file);

    reader
        .lines()
        .flatten() // gets rid of Err from lines
        .flat_map(|line| line.parse::<T>()) // ignores Err variant from Result of str.parse
        .collect()
}
