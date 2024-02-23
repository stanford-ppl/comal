use clap::Args;
use dam::{
    shim::RunMode,
    simulation::{
        InitializationOptions, InitializationOptionsBuilder, RunOptions, RunOptionsBuilder,
    },
};

use crate::config::rd_scanner::CompressedCrdRdScanConfig;

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

#[derive(Args, Debug, Clone, Default)]
pub struct SamOptionFiles {
    /// TOML file containing a [[CompressedRdScanConfig]]
    #[arg(long)]
    compressed_read_config: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SamOptions {
    pub compressed_read_config: CompressedCrdRdScanConfig,
}

// Defining a read_or_default conversion from SamOptionFiles to SamOptions
impl Into<SamOptions> for &SamOptionFiles {
    fn into(self) -> SamOptions {
        SamOptions {
            compressed_read_config: self.try_into().unwrap(),
        }
    }
}

macro_rules! config_type {
    ($id:ident, $type: ty) => {
        impl TryInto<$type> for &SamOptionFiles {
            type Error = anyhow::Error;

            fn try_into(self) -> Result<$type, Self::Error> {
                match &self.$id {
                    Some(config) => {
                        let file_contents = std::fs::read_to_string(config)?;
                        let parsed = toml::from_str(&file_contents)?;
                        Ok(parsed)
                    }
                    None => Ok(Default::default()),
                }
            }
        }
    };
}

config_type!(compressed_read_config, CompressedCrdRdScanConfig);
