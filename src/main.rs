use anyhow::Result;
use client::{Client, GeminiResponse};
use url::Url;

mod client;

use crossterm::event::{self, Event};
use ratatui::{DefaultTerminal, Frame};

fn main() -> Result<()> {
    let terminal = ratatui::init();
    let client = Client::new(true);
    let response = client.request(Url::parse("gemini://geminiprotocol.net")?)?;
    let mut text = String::new();
    if let GeminiResponse::Success { mime, body } = response {
        if mime.starts_with("text/") {
            text = String::from_utf8_lossy(&body).into_owned();
        }
    } else {
    }
    let result = run(terminal, text);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal, text: String) -> Result<()> {
    loop {
        terminal.draw(|frame: &mut Frame| {
            frame.render_widget(&text, frame.area());
        })?;
        if matches!(event::read()?, Event::Key(_)) {
            break Ok(());
        }
    }
}
