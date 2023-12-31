#![allow(dead_code)]

use std::{fs, path::Path, time::Instant};

use argparse::{ArgumentParser, Store, StoreFalse};
use comal::config::Data;
use dam::simulation::{InitializationOptionsBuilder, RunMode, RunOptionsBuilder};
use prost::Message;
use proto_driver::{parse_proto, proto_headers::tortilla::ComalGraph};

mod proto_driver;
mod templates;

fn main() {
    let mut data_dir_name = "gcn2".to_string();
    let mut proto_filename = "gcn.bin".to_string();
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

    dbg!(base_path.join(proto_filename.clone()));
    let comal_contents = fs::read(base_path.join(proto_filename)).unwrap();
    let comal_graph = ComalGraph::decode(comal_contents.as_slice()).unwrap();

    let parent = parse_proto(comal_graph, base_path);

    let init_start = Instant::now();
    let initialized = parent
        .initialize(
            InitializationOptionsBuilder::default()
                .run_flavor_inference(true)
                .build()
                .unwrap(),
        )
        .unwrap();
    let init_end = Instant::now();
    println!("Init took: {:.2?}", init_end - init_start);

    let executed = initialized.run(
        RunOptionsBuilder::default()
            .mode(RunMode::Simple)
            .build()
            .unwrap(),
    );
    let finish = Instant::now();
    println!("Run took: {:.2?}", finish - init_end);
    println!("Elapsed cycles: {:?}", executed.elapsed_cycles());
}
