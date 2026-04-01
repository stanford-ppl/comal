#![allow(dead_code)]

use std::{fs, time::Instant};

use cli_common::{DamOptions, SamOptionFiles};
use comal::templates::array::ArrayLog;
use dam::{logging::LogEvent, simulation::*};
use prost::Message;
use proto_driver::{parse_proto, parse_proto_vec16, proto_headers::tortilla::ComalGraph};

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
        default_value = "/home/rubensl/samml-artifact/data/gcn_unfused/adj_linear1_mul"
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
    let program_builder = if std::env::var("COMAL_VECTOR_MODE").map(|v| v == "16").unwrap_or(false) {
        parse_proto_vec16(comal_graph, args.data.into(), (&args.sam_opts).into())
    } else {
        parse_proto(comal_graph, args.data.into(), (&args.sam_opts).into())
    };
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

    // let run_options = run_options.logging(LoggingOptions::Mongo(
    //     MongoOptionsBuilder::default()
    //         .db("joiner_log".to_string())
    //         .uri("mongodb://127.0.0.1:27017".to_string())
    //         .build()
    //         .unwrap(),
    // ));

    let initialized = program_builder.initialize(args.dam_opts.into()).unwrap();
    println!("{}", initialized.to_dot_string());

    let initialized_time = Instant::now();
    if args.breakdowns {
        println!("Initialization Time: {:?}", initialized_time - end_parse);
    }

    // let executed = initialized.run(run_options.build().unwrap());
    let executed = initialized.run(RunOptionsBuilder::default().mode(RunMode::Simple).build().unwrap());
    if args.breakdowns {
        println!("Execution Time: {:?}", initialized_time.elapsed());
    }
    println!("Elapsed Cycles: {}", executed.elapsed_cycles().unwrap());

    // Per-node utilization: parse elapsed times from dot string tooltips
    if std::env::var("COMAL_NODE_TIMING").map(|v| v != "0").unwrap_or(false) {
        let dot = executed.to_dot_string();
        let mut entries: Vec<(String, u64)> = Vec::new();
        for line in dot.lines() {
            // Parse: Node_N[shape="rectangle",label="Name(ID_N)",tooltip="Elapsed: 12345"]
            if let (Some(label_start), Some(tooltip_start)) = (line.find("label=\""), line.find("tooltip=\"Elapsed: ")) {
                let label = &line[label_start+7..];
                let label_end = label.find('"').unwrap_or(0);
                let name = &label[..label_end];
                let elapsed_str = &line[tooltip_start+18..];
                let elapsed_end = elapsed_str.find('"').unwrap_or(0);
                if let Ok(cycles) = elapsed_str[..elapsed_end].parse::<u64>() {
                    entries.push((name.to_string(), cycles));
                }
            }
        }
        entries.sort_by_key(|e| std::cmp::Reverse(e.1));
        let max_cyc = executed.elapsed_cycles().unwrap() as f64;
        println!("\n=== Per-Node Timing (sorted by end cycle) ===");
        println!("{:<50} {:>10} {:>8}", "Node", "End Cycle", "% Max");
        println!("{}", "-".repeat(70));
        for (name, end) in &entries {
            let pct = (*end as f64 / max_cyc) * 100.0;
            let bar_len = (pct / 2.0) as usize;
            println!("{:<50} {:>10} {:>6.1}% {}", name, end, pct, "#".repeat(bar_len));
        }
    }
}
