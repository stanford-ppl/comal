#![allow(dead_code)]

use std::{fs, path::Path, time::Instant};

use argparse::{ArgumentParser, Store, StoreFalse};
use comal::config::Data;
use prost::Message;
use proto_driver::{parse_proto, proto_headers::tortilla::ComalGraph};

mod proto_driver;
mod templates;

fn main() {
    let mut data_dir_name = "".to_string();
    let mut proto_filename = "".to_string();
    let mut with_flavor = true;

    {
        let mut ap = ArgumentParser::new();
        ap.refer(&mut data_dir_name).add_option(
            &["--data_dir", "-d"],
            Store,
            "Directory with SAM data files",
        );
        ap.refer(&mut with_flavor).add_option(
            &["--no-inference"],
            StoreFalse,
            "Run without flavor inference",
        );
        ap.refer(&mut proto_filename)
            .add_option(&["--proto_file", "-p"], Store, "Proto bin file");
        ap.parse_args_or_exit();
    }

    let config_file = home::home_dir().unwrap().join("sam_config.toml");
    let contents = fs::read_to_string(config_file).unwrap();
    let data: Data = toml::from_str(&contents).unwrap();
    let formatted_dir = data.sam_config.sam_path;
    let base_path = Path::new(&formatted_dir).join(&data_dir_name);

    let comal_contents = fs::read(proto_filename).unwrap();
    let comal_graph = ComalGraph::decode(comal_contents.as_slice()).unwrap();

    let mut parent = parse_proto(comal_graph, base_path);

    parent.set_inference(with_flavor);
    let now = Instant::now();
    parent.init();
    let elapsed = now.elapsed();
    println!("Init took: {:.2?}", elapsed);
    let now = Instant::now();
    parent.run();
    let elapsed = now.elapsed();
    println!("Run took: {:.2?}", elapsed);
}
