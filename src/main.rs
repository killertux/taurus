use std::fs::{read_to_string, File};

use anyhow::Result;
use app::App;
use serde::Deserialize;
use tracing::Level;

mod app;
mod client;
mod gemtext;

#[derive(Deserialize)]
struct Config {
    cert_file: String,
    key_file: String,
}

fn main() -> Result<()> {
    let writer = File::create("taurus.log")?;
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_line_number(true)
        .with_file(true)
        .with_max_level(Level::DEBUG)
        .init();

    let config_contents = read_to_string("Config.toml");
    let config: Option<Config> = if let Ok(contents) = config_contents {
        Some(toml::from_str(&contents)?)
    } else {
        None
    };
    tracing::info!("Started taurus");
    let app = App::new(config);
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
