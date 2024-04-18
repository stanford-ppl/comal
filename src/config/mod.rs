pub mod rd_scanner;
pub mod joiner;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Data {
    pub sam_config: Config,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub sam_path: String,
}
