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
    pub fiberlookup_numerator_factor: u64,
    pub fiberlookup_denominator_factor: u64,
    pub fiberwrite_latency: u64,
    pub fiberwrite_ii: u64,
}

#[cfg(test)]
mod tests {

    use super::Config;

    #[test]
    fn get_path() {
        let config: Config = toml::from_str("sam_path = '$HOME'").unwrap();
        dbg!(config);
    }
}
