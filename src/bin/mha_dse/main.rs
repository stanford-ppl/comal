mod mha_impl;

use std::time::Instant;

use clap::Parser;
use comal::cli_common::DamOptions;

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

    /// Print timing breakdowns
    #[arg(long, default_value_t = false)]
    breakdowns: bool,

    #[command(flatten)]
    dam_opts: DamOptions,
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

    let initialized = builder.initialize(args.dam_opts.into()).unwrap();
    let initialized_time = Instant::now();
    if args.breakdowns {
        println!("Initialization Time: {:?}", initialized_time - start);
    }

    let executed = initialized.run(args.dam_opts.into());
    if args.breakdowns {
        println!("Execution Time: {:?}", initialized_time.elapsed());
    }
    println!("Elapsed Cycles: {}", executed.elapsed_cycles().unwrap());
}
