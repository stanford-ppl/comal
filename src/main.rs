#![allow(dead_code)]

use std::{fs, time::Instant};

use cli_common::{DamOptions, SamOptionFiles};
use comal::templates::array::ArrayLog;
use dam::{logging::LogEvent, simulation::*};
use prost::Message;
use proto_driver::{parse_proto, proto_headers::tortilla::ComalGraph};

mod cli_common;
mod config;
mod proto_driver;
mod templates;
mod utils;

use clap::Parser;
use templates::{
    accumulator::{ReduceLog, SpaccLog},
    joiner::JoinerLog,
    rd_scanner::LSLog,
    repeat::RepeatLog,
};

#[derive(Parser, Debug)]
struct Cli {
    /// Protobuffer containing a tortilla graph
    #[arg(long, default_value = "/tmp/op.bin")]
    proto: String,

    /// Data directory for the graph
    #[arg(
        long,
        // default_value = "/home/rubensl/Documents/repos/samml-artifact/data/misc/sparse_softmax_tmp"
        default_value = "/home/rubensl/Documents/repos/samml-artifact/data/models/graphsage"
    )]
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

    let run_options = RunOptionsBuilder::default().log_filter(LogFilterKind::Blanket(
        dam::logging::LogFilter::Some(
            [
                "JoinerLog".to_owned(),
                "RepeatLog".to_owned(),
                "RepsiggenLog".to_owned(),
                // SpaccLog::NAME.to_owned(),
                // LSLog::NAME.to_owned(),
                // ArrayLog::NAME.to_owned(),
                // ReduceLog::NAME.to_owned(),
            ]
            .into(),
        ),
    ));

    let run_options = run_options.logging(LoggingOptions::Mongo(
        MongoOptionsBuilder::default()
            .db("joiner_log".to_string())
            .uri("mongodb://127.0.0.1:27017".to_string())
            .build()
            .unwrap(),
    ));

    let initialized = program_builder.initialize(args.dam_opts.into()).unwrap();
    println!("{}", initialized.to_dot_string());

    let initialized_time = Instant::now();
    if args.breakdowns {
        println!("Initialization Time: {:?}", initialized_time - end_parse);
    }

    let executed = initialized.run(run_options.build().unwrap());
    if args.breakdowns {
        println!("Execution Time: {:?}", initialized_time.elapsed());
    }
    println!("Elapsed Cycles: {}", executed.elapsed_cycles().unwrap());
}
