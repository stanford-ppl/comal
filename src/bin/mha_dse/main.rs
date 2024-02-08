mod mha_impl;

use std::time::Instant;

use clap::Parser;
use dam::simulation::{InitializationOptionsBuilder, RunMode, RunOptionsBuilder};

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long)]
    data: String,

    #[arg(long, default_value_t = 1)]
    inner_par: usize,

    #[arg(long, default_value_t = 1)]
    outer_par: usize,

    #[arg(short, long, default_value_t = 64)]
    short_chan_size: usize,

    #[arg(short, long)]
    long_chan_size: usize,

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
    let args = Cli::parse();
    assert!(args.inner_par >= 1);
    assert!(args.outer_par >= 1);

    let builder = mha_impl::run_mha(
        args.inner_par,
        args.outer_par,
        args.data.into(),
        args.short_chan_size,
        args.long_chan_size,
    );

    let start = Instant::now();

    let initialized = builder
        .initialize(
            InitializationOptionsBuilder::default()
                .run_flavor_inference(args.inference)
                .build()
                .unwrap(),
        )
        .unwrap();
    let initialized_time = Instant::now();
    if args.breakdowns {
        println!("Initialization Time: {:?}", initialized_time - start);
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
