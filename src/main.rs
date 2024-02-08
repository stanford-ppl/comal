#![allow(dead_code)]

use std::{fs, time::Instant};

use dam::simulation::{InitializationOptionsBuilder, RunMode, RunOptionsBuilder};
use prost::Message;
use proto_driver::{parse_proto, proto_headers::tortilla::ComalGraph};

mod config;
mod proto_driver;
mod templates;
mod utils;

use clap::Parser;

#[derive(Parser, Debug)]
struct Cli {
    /// Protobuffer containing a tortilla graph
    #[arg(long)]
    proto: String,

    /// Data directory for the graph
    #[arg(long)]
    data: String,

    /// Run flavor inference
    #[arg(long, default_value_t = false)]
    inference: bool,

    /// Number of worker threads
    #[arg(long)]
    workers: Option<usize>,

    /// Print timing breakdowns
    #[arg(long, default_value_t = false)]
    breakdowns: bool,
}

fn main() {
    let start = Instant::now();
    let args = Cli::parse();
    let comal_graph = {
        let file_contents = fs::read(&args.proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto(comal_graph, args.data.into());
    let end_parse = Instant::now();
    if args.breakdowns {
        println!("Parse Time: {:?}", end_parse - start);
    }
    let initialized = program_builder
        .initialize(
            InitializationOptionsBuilder::default()
                .run_flavor_inference(args.inference)
                .build()
                .unwrap(),
        )
        .unwrap();
    let initialized_time = Instant::now();
    if args.breakdowns {
        println!("Initialization Time: {:?}", initialized_time - end_parse);
    }

    let run_opts = match args.workers {
        Some(workers) => RunOptionsBuilder::default()
            .mode(RunMode::Constrained(workers))
            .build()
            .unwrap(),
        None => Default::default(),
    };
    let executed = initialized.run(run_opts);
    if args.breakdowns {
        println!("Execution Time: {:?}", initialized_time.elapsed());
    }
    println!("Elapsed Cycles: {}", executed.elapsed_cycles().unwrap());
}

// fn main() {

//     {

//         ap.refer(&mut data_dir_name).add_option(
//             &["--data_dir", "-d"],
//             Store,
//             "Directory with SAM data files",

//         ap.refer(&mut with_flavor).add_option(
//             &["--no-inference"],
//             StoreFalse,
//             "Run without flavor inference",

//         ap.refer(&mut run_dse)

//         ap.refer(&mut par_factor)

//         ap.refer(&mut outer_par_factor).add_option(
//             &["--outer_par_factor", "-o"],
//             Store,
//             "Outer par factor",

//         ap.refer(&mut proto_filename)

//     }

//     let parent = if run_dse {
//         run_mha(par_factor, outer_par_factor, base_path)
//     } else {

//         parse_proto(comal_graph, base_path)

//     let initialized = parent
//         .initialize(
//             InitializationOptionsBuilder::default()
//                 .run_flavor_inference(true)
//                 .build()
//                 .unwrap(),
//         )

//     let executed = initialized.run(
//         RunOptionsBuilder::default()
//             .mode(RunMode::Simple)
//             .build()
//             .unwrap(),

// }
