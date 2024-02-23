#![allow(dead_code)]

use std::{fs, time::Instant};

use comal::{
    cli_common::{DamOptions, SamOptionFiles},
    proto_driver::{parse_proto, proto_headers::tortilla::ComalGraph},
};

use clap::Parser;
use prost::Message;

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

    /// Pre-defined stream values, used in place of Root nodes
    /// These are prefixed with a type () followed by comma-delimited tokens, each of which is either:
    /// 1. A Float/Integer value
    /// 2. A Stop token
    /// 3. An empty token
    /// 4. A Done token
    #[arg(short, long)]
    channel: Vec<String>,

    #[command(flatten)]
    dam_opts: DamOptions,

    #[command(flatten)]
    sam_opts: SamOptionFiles,
}

fn parse_channel_data() {}

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
