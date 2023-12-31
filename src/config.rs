use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Data {
    pub sam_config: Config,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub sam_path: String,
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
