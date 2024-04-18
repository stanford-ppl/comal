use serde::Deserialize;

#[derive(Debug, Deserialize, Copy, Clone)]
pub struct IntersectConfig {
    /// A "warmup" delay at the very start of the pipeline
    pub startup_delay: u64,

    /// Pause after seeing a new value in a scanner
    /// This represents an unavoidable bubble in the pipeline
    pub stop_latency: u64,

    /// Latency before emitting new values in a scanner
    /// this can be pipelined against the next delay.
    pub output_latency: u64,

    /// Initiation interval of the output pipeline -- the delay between consecutive outputs in the same output block
    pub sequential_interval: u64,

    /// Pipeline bubble size when starting a new row when reading from memory
    pub val_stop_delay: u64,

    /// Pipeline bubble size when starting a new row when reading from memory
    pub val_advance_delay: u64,
}

impl Default for IntersectConfig {
    fn default() -> Self {
        Self {
            startup_delay: 0,
            stop_latency: 0,
            output_latency: 1,
            sequential_interval: 1,
            val_stop_delay: 0,
            val_advance_delay: 1,
        }
    }
}