use serde::Deserialize;
use strum::EnumString;

/// Describes a channel stored in a toml file

#[derive(Deserialize, Debug, Clone)]
pub struct ChannelFile {
    pub id: usize,
    pub tp: String,
    pub payload: String,
}

#[derive(Copy, Clone, Debug, EnumString)]
pub enum ChannelType {
    Value,
    Coordinate,
    Reference,
    Repeat,
}

impl ChannelFile {
    pub fn parse_payload<TKType, ConvType>(&self, conv: ConvType) -> Vec<TKType>
    where
        ConvType: Fn(&str) -> TKType,
    {
        self.payload
            .split(',')
            .map(|s| s.trim())
            .map(conv)
            .collect()
    }
}
