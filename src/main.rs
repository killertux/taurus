use std::time::Duration;

use anyhow::Result;
use client::{Client, GeminiResponse};
use url::Url;

mod client;

use crossterm::event::{self, Event};
use ratatui::{
    prelude::{Buffer, Rect},
    style::Stylize,
    text::Line,
    widgets::{Block, Paragraph, Widget, Wrap},
    DefaultTerminal, Frame,
};

struct App {
    current_url: Url,
    client: Client,
    content: String,
}

fn main() -> Result<()> {
    let app = App::default();
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}

impl Default for App {
    fn default() -> Self {
        Self {
            current_url: Url::parse("gemini://gemini.circumlunar.space/capcom/")
                .expect("We know that this is a valid url"),
            client: Client::new(true),
            content: String::new(),
        }
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let title = Line::from(self.current_url.as_str()).bold();
        let main_block = Block::bordered().title_top(title);
        Paragraph::new(self.content.as_str())
            .wrap(Wrap { trim: true })
            .block(main_block)
            .render(area, buf);
    }
}

impl App {
    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut first = true;
        loop {
            terminal.draw(|frame: &mut Frame| self.draw(frame))?;
            if first {
                first = false;
                self.reload_site()?;
            }
            if event::poll(Duration::from_millis(300))? && matches!(event::read()?, Event::Key(_)) {
                break Ok(());
            }
        }
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn reload_site(&mut self) -> Result<()> {
        let response = self.client.request(self.current_url.clone())?;
        match response {
            GeminiResponse::Success { mime, body } => {
                let content = if mime.starts_with("text") {
                    String::from_utf8(body)?
                } else {
                    format!("Not type that we know how to process. {mime}")
                };
                self.content = content;
            }
            _ => unimplemented!(),
        }
        Ok(())
    }
}
