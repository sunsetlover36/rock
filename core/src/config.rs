use std::{
    fs::File,
    io::{self, BufRead},
};

use color_eyre::eyre::Result;

#[derive(Debug, Default)]
pub struct ServerConfig {
    pub gamemode_name: String,
    pub max_players: Option<u32>,
    pub farcaster_key: Option<String>,
}
impl ServerConfig {
    pub fn filename() -> &'static str {
        "config.cfg"
    }

    pub fn new() -> Result<Self> {
        let file = File::open(ServerConfig::filename())?;
        let reader = io::BufReader::new(file);

        let mut config = ServerConfig::default();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            if let Some((key, value)) = line.split_once(" is ") {
                let value = value.trim();

                match key.trim() {
                    "gamemode name" => config.gamemode_name = value.to_string(),
                    "max players" => config.max_players = Some(value.parse()?),
                    "farcaster key" => config.farcaster_key = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        Ok(config)
    }
}
