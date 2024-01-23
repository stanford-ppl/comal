use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct CompressedCrdRdScanConfig {
    /// A "warmup" delay at the very start of the pipeline
    pub startup_delay: u64,

    /// A multiplier accounting for how long it takes to get the segment/coord arrays loaded initially
    pub data_load_factor: f64,

    /// Pause after seeing a new value in a scanner
    /// This represents an unavoidable bubble in the pipeline
    pub initial_delay: u64,

    /// Latency from a crd/seg read miss
    pub miss_latency: u64,

    /// Determines whether to increment on just crd read miss, seg read miss, or both
    pub incr_type: u64,

    /// Latency before emitting new values in a scanner
    /// This is different from the delay in that it does not affect the base time; this can be pipelined against the next delay.
    pub output_latency: u64,

    /// Initiation interval of the output pipeline -- the delay between consecutive outputs in the same output block
    pub sequential_interval: u64,
}

impl Default for CompressedCrdRdScanConfig {
    fn default() -> Self {
        Self {
            startup_delay: 10,
            data_load_factor: 1.0,
            initial_delay: 0,
            miss_latency: 20,
            incr_type: 2,
            output_latency: 1,
            sequential_interval: 1,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CalibrationData {
    pub calibration_params: CompressedCrdRdScanConfig,
}
