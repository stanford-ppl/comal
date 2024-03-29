use dam::types::DAMType;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::tensor::Adapter;

fn set_tensor_path() {
    env::set_var("FROSTT_FORMATTED_PATH", "/home/rubensl/Documents/data");
}

pub fn read_inputs_vectorized<T>(file_path: &PathBuf, _prim_type: impl Adapter<T>) -> Vec<T>
where
    T: DAMType,
{
    let _file =
        File::open(file_path).unwrap_or_else(|_| panic!("file {:?} wasn't found", file_path));
    // prim_type.parse(BufReader::new(file).lines().flatten())
    todo!()
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

pub fn write_output<T>(file_path: &PathBuf, output_data: Arc<Mutex<Vec<T>>>)
where
    T: DAMType + std::str::FromStr,
{
    // get the lock for the output data
    let output_data_locked = output_data.lock().unwrap();
    let file = File::create(file_path).expect("Unable to open file");
    let mut writer = BufWriter::new(file);
    let mut it = output_data_locked.iter().peekable();
    while let Some(data) = it.next() {
        if it.peek().is_none() {
            write!(writer, "{:?}", data).expect("Unabel to wrtie data");
        } else {
            write!(writer, "{:?}\n", data).expect("Unable to write data");
        }
    }
}
