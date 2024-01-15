use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Data {
    pub sam_config: Config,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub sam_path: String,
    pub fiberlookup_latency: u64,
    pub fiberlookup_initial: u64,
    pub fiberlookup_ii: u64,
    pub fiberlookup_starting: u64,
    pub fiberlookup_stop_latency: u64,
    pub fiberlookup_factor: f64,
    pub fiberwrite_latency: u64,
    pub fiberwrite_ii: u64,
    pub bump: u64,
    pub stop_bump: u64,
    pub done_bump: u64,
    pub empty_bump: u64,
}
