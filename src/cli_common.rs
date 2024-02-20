use clap::Args;
use dam::{
    shim::RunMode,
    simulation::{
        InitializationOptions, InitializationOptionsBuilder, RunOptions, RunOptionsBuilder,
    },
};

#[derive(Args, Debug, Clone, Copy)]
pub struct DamOptions {
    /// Run flavor inference
    #[arg(long, default_value_t = false)]
    inference: bool,

    /// Number of worker threads
    #[arg(long)]
    workers: Option<usize>,
}

impl Into<InitializationOptions> for DamOptions {
    fn into(self) -> InitializationOptions {
        InitializationOptionsBuilder::default()
            .run_flavor_inference(self.inference)
            .build()
            .unwrap()
    }
}

impl Into<RunOptions> for DamOptions {
    fn into(self) -> RunOptions {
        match self.workers {
            Some(num) => RunOptionsBuilder::default()
                .mode(RunMode::Constrained(num))
                .build()
                .unwrap(),
            None => RunOptions::default(),
        }
    }
}
