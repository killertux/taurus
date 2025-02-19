use std::time::Duration;

use anyhow::{bail, Result};
use client::{Client, GeminiResponse};
use url::Url;

mod client;

use crossterm::event::{self, Event};
use ratatui::{
    prelude::{Buffer, Rect},
    style::{self, Color, Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph, Widget, Wrap},
    DefaultTerminal, Frame,
};

struct App {
    current_url: Url,
    client: Client,
    content: Option<Content>,
}

fn main() -> Result<()> {
    let app = App::default();
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}

struct Content {
    mime: String,
    body: Body,
}

enum Body {
    String(String),
    Bytes(Vec<u8>),
}

impl Content {
    fn from_mime_and_bytes(mime: String, bytes: Vec<u8>) -> Result<Self> {
        if mime.starts_with("text/") {
            let body = String::from_utf8(bytes)?;
            return Ok(Self {
                mime,
                body: Body::String(body),
            });
        }
        Ok(Self {
            mime,
            body: Body::Bytes(bytes),
        })
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            current_url: Url::parse("gemini://gemini.circumlunar.space/capcom/")
                .expect("We know that this is a valid url"),
            client: Client::new(true),
            content: None,
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
        match &self.content {
            None => Paragraph::new("No content")
                .wrap(Wrap { trim: true })
                .block(main_block)
                .render(area, buf),
            Some(content) => match &content.body {
                Body::Bytes(_) => {
                    Paragraph::new("Format not supported!")
                        .wrap(Wrap { trim: true })
                        .block(main_block)
                        .render(area, buf);
                }
                Body::String(body) => {
                    if content.mime.starts_with("text/gemini") {
                        let parser = GemTextParser::new(&body, self.current_url.clone());
                        let mut n_links = 0;
                        let mut lines = Vec::new();
                        for line in parser {
                            let Ok(line) = line else {
                                dbg!(line.expect_err("Should be an error"));
                                continue;
                            };
                            match line {
                                GemTextLine::Text(text) => {
                                    lines.push(Line::raw(text).left_aligned());
                                }
                                GemTextLine::PreFormatted(text) => {
                                    lines.push(
                                        Line::raw(text)
                                            .left_aligned()
                                            .style(Style::new().bg(Color::Gray)),
                                    );
                                }
                                GemTextLine::Link { url, text } => {
                                    let color = if url.scheme() == "gemini" {
                                        Color::Blue
                                    } else {
                                        Color::Red
                                    };
                                    lines.push(Line::styled(
                                        format!("[{n_links}] {text}"),
                                        Style::new().fg(color),
                                    ));
                                    n_links += 1;
                                }
                            }
                        }

                        Paragraph::new(lines)
                            .wrap(Wrap { trim: true })
                            .block(main_block)
                            .render(area, buf);
                    }
                }
            },
        }
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
                self.content = Some(Content::from_mime_and_bytes(mime, body)?);
            }
            _ => unimplemented!(),
        }
        Ok(())
    }
}

struct GemTextParser<'a> {
    raw_text: &'a str,
    url: Url,
}

#[derive(Debug)]
enum GemTextLine<'a> {
    Text(&'a str),
    Link { url: Url, text: &'a str },
    PreFormatted(&'a str),
}

impl<'a> GemTextParser<'a> {
    pub fn new(raw_text: &'a str, url: Url) -> Self {
        Self { raw_text, url }
    }

    fn parse_next(&mut self) -> Result<GemTextLine<'a>> {
        let line = if let Some((line, rest)) = self.raw_text.split_once("\n") {
            self.raw_text = rest;
            line
        } else {
            let line = self.raw_text;
            self.raw_text = "";
            line
        };
        if let Some(link_line) = line.strip_prefix("=> ") {
            let Some((link, text)) = link_line.split_once(|x: char| x.is_whitespace()) else {
                bail!("Invalid link line {line}");
            };
            if !link.contains("://") {
                return Ok(GemTextLine::Link {
                    url: self.url.join(link)?,
                    text,
                });
            }
            return Ok(GemTextLine::Link {
                url: Url::parse(link)?,
                text,
            });
        }
        if let Some(pre_formatted_line) = line.strip_prefix("```") {
            return Ok(GemTextLine::PreFormatted(pre_formatted_line));
        }
        Ok(GemTextLine::Text(line))
    }
}

impl<'a> Iterator for GemTextParser<'a> {
    type Item = Result<GemTextLine<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.raw_text.is_empty() {
            return None;
        }
        Some(self.parse_next())
    }
}
