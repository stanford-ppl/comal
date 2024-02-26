#![allow(dead_code)]

use std::{fs, time::Instant};

use cli_common::{DamOptions, SamOptionFiles};
use prost::Message;
use proto_driver::{parse_proto, proto_headers::tortilla::ComalGraph};

mod cli_common;
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

    /// Print timing breakdowns
    #[arg(long)]
    breakdowns: bool,

    #[command(flatten)]
    dam_opts: DamOptions,

    #[command(flatten)]
    sam_opts: SamOptionFiles,
}

fn main() {
    let start = Instant::now();
    let args = Cli::parse();
    let comal_graph = {
        let file_contents = fs::read(&args.proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };
    let program_builder = parse_proto(comal_graph, args.data.into(), (&args.sam_opts).into());
    let end_parse = Instant::now();
    if args.breakdowns {
        println!("Parse Time: {:?}", end_parse - start);
    }
    let initialized = program_builder.initialize(args.dam_opts.into()).unwrap();
    let initialized_time = Instant::now();
    if args.breakdowns {
        println!("Initialization Time: {:?}", initialized_time - end_parse);
    }

    let executed = initialized.run(args.dam_opts.into());
    if args.breakdowns {
        println!("Execution Time: {:?}", initialized_time.elapsed());
    }
    println!("Elapsed Cycles: {}", executed.elapsed_cycles().unwrap());
}
