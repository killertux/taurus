use std::{
    fs::{read_to_string, File},
    time::Duration,
};

use anyhow::Result;
use client::{Certificates, Client, GeminiResponse};
use serde::Deserialize;
use tracing::Level;
use url::Url;

mod client;

use crossterm::{
    event::{self, Event, KeyCode},
    terminal,
};
use ratatui::{
    layout::{Constraint, Layout},
    prelude::{Buffer, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph, Widget, Wrap},
    DefaultTerminal, Frame,
};

struct App {
    gemspaces_nav: GemspaceNav,
    client: Client,
    content: Option<Content>,
    scroll: (u16, u16),
    status: AppStatus,
}

enum AppStatus {
    Browsing,
    Typing(String),
    Loading,
    Input(String),
}

impl AppStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppStatus::Browsing => "Browsing",
            AppStatus::Typing(_) => "Typing",
            AppStatus::Loading => "Loading",
            AppStatus::Input(_) => "Input",
        }
    }
}

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

struct GemspaceNav {
    gemspaces: Vec<Url>,
    position: usize,
}

impl GemspaceNav {
    pub fn new(url: Url) -> Self {
        Self {
            gemspaces: Vec::from([url]),
            position: 1,
        }
    }

    pub fn push(&mut self, url: Url) {
        (self.position..self.gemspaces.len()).for_each(|pos| {
            self.gemspaces.remove(pos);
        });
        self.gemspaces.push(url);
        self.position += 1;
    }

    pub fn current(&self) -> Url {
        self.gemspaces
            .get(self.position - 1)
            .expect("We should always have a current URL")
            .clone()
    }

    pub fn back(&mut self) {
        if self.position > 1 {
            self.position -= 1;
        }
    }

    pub fn advance(&mut self) {
        self.position = self.gemspaces.len().min(self.position + 1);
    }
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

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]);
        let [browser, command] = layout.areas(area);
        let url = self.gemspaces_nav.current();
        let title = Line::from(url.as_str()).bold();
        let main_block = Block::bordered().title_top(title);
        match &self.content {
            None => {
                Paragraph::new("No content")
                    .wrap(Wrap { trim: true })
                    .block(main_block)
                    .render(browser, buf);
            }
            Some(content) => match &content.body {
                Body::Bytes(_) => {
                    Paragraph::new("Format not supported!")
                        .wrap(Wrap { trim: true })
                        .block(main_block)
                        .render(browser, buf);
                }
                Body::String(body) => {
                    if content.mime.starts_with("text/gemini") {
                        let parser = GemTextParser::new(&body, self.gemspaces_nav.current());
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
                            .scroll(self.scroll)
                            .render(browser, buf);
                    } else {
                        Paragraph::new(body.as_str())
                            .wrap(Wrap { trim: true })
                            .block(main_block)
                            .scroll(self.scroll)
                            .render(browser, buf);
                    }
                }
            },
        }
        let layout = Layout::horizontal([Constraint::Min(2), Constraint::Length(10)]);
        let [left, right] = layout.areas(command);
        let cmd_block = Block::new();
        let status_block = Block::new();
        let typed = match &self.status {
            AppStatus::Typing(text) | AppStatus::Input(text) => text.as_str(),
            _ => "",
        };
        Paragraph::new(format!("=> {typed}"))
            .block(cmd_block)
            .render(left, buf);
        Paragraph::new(self.status.as_str())
            .block(status_block)
            .render(right, buf);
    }
}

impl App {
    pub fn new(config: Option<Config>) -> Self {
        Self {
            gemspaces_nav: GemspaceNav::new(
                Url::parse("gemini://gemini.circumlunar.space/capcom/")
                    .expect("We know that this is a valid url"),
            ),
            client: Client::new(
                true,
                config.map(|cfg| Certificates {
                    cert_file: cfg.cert_file,
                    key_file: cfg.key_file,
                }),
            ),
            content: None,
            scroll: (0, 0),
            status: AppStatus::Loading,
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame: &mut Frame| self.draw(frame))?;
            if matches!(self.status, AppStatus::Loading) {
                self.load_site()?;
            }
            if event::poll(Duration::from_millis(300))? {
                match event::read()? {
                    Event::Key(key_event) => match self.status {
                        AppStatus::Loading => {}
                        AppStatus::Browsing => match key_event.code {
                            KeyCode::Esc => {
                                break Ok(());
                            }
                            KeyCode::PageUp => {
                                let step = terminal::size()?.1 - 3;
                                self.scroll.0 = self.scroll.0.saturating_sub(step);
                            }
                            KeyCode::PageDown => {
                                let step = terminal::size()?.1 - 3;
                                self.scroll.0 = self.scroll.0.saturating_add(step);
                            }
                            KeyCode::Up => {
                                self.scroll.0 = self.scroll.0.saturating_sub(1);
                            }
                            KeyCode::Down => {
                                self.scroll.0 = self.scroll.0.saturating_add(1);
                            }
                            KeyCode::Char('i') => {
                                self.status = AppStatus::Typing(String::new());
                            }
                            KeyCode::Char('<') => {
                                self.gemspaces_nav.back();
                                self.set_status_to_loading();
                            }
                            KeyCode::Char('>') => {
                                self.gemspaces_nav.advance();
                                self.set_status_to_loading();
                            }
                            _ => {}
                        },
                        AppStatus::Typing(ref mut text) => match key_event.code {
                            KeyCode::Esc => {
                                self.status = AppStatus::Browsing;
                            }
                            KeyCode::Char(c) => {
                                text.push(c);
                            }
                            KeyCode::Enter => {
                                if let Ok(n) = text.parse::<usize>() {
                                    let Some(Content { body, .. }) = &self.content else {
                                        continue;
                                    };
                                    let Body::String(body) = body else {
                                        continue;
                                    };
                                    let parser =
                                        GemTextParser::new(&body, self.gemspaces_nav.current());
                                    let Some(link) = parser
                                        .flatten()
                                        .filter_map(|line| match line {
                                            GemTextLine::Link { url, .. } => Some(url),
                                            _ => None,
                                        })
                                        .enumerate()
                                        .filter_map(
                                            |(n_link, link)| {
                                                if n_link == n {
                                                    Some(link)
                                                } else {
                                                    None
                                                }
                                            },
                                        )
                                        .next()
                                    else {
                                        continue;
                                    };
                                    self.push_url(link);
                                    continue;
                                }
                                if text.starts_with("gemini://") {
                                    let url = Url::parse(text)?;
                                    self.push_url(url);
                                    continue;
                                }
                                let url = self.gemspaces_nav.current().join(&text)?;
                                self.push_url(url);
                            }
                            _ => {}
                        },
                        AppStatus::Input(ref mut text) => match key_event.code {
                            KeyCode::Esc => {
                                *text = String::new();
                            }
                            KeyCode::Char(c) => {
                                text.push(c);
                            }
                            KeyCode::Enter => {
                                let mut url = self.gemspaces_nav.current();
                                url.set_query(Some(text));
                                self.gemspaces_nav.back();
                                self.push_url(url);
                            }
                            _ => {}
                        },
                    },

                    _ => {}
                }
            }
        }
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn load_site(&mut self) -> Result<()> {
        let response = self.client.request(self.gemspaces_nav.current());
        let Ok(response) = response else {
            let err = response.unwrap_err();
            tracing::error!("Error requesting gemini url: {}", err);
            return Err(err);
        };
        match response {
            GeminiResponse::Success { mime, body } => {
                self.content = Some(Content::from_mime_and_bytes(mime, body)?);
            }
            GeminiResponse::Input { status: _, prompt } => {
                self.content = Some(Content {
                    mime: "text/plain".into(),
                    body: Body::String(prompt),
                });
                self.status = AppStatus::Input(String::new());
                return Ok(());
            }
            response => unimplemented!("For {response:?}"),
        }
        self.status = AppStatus::Browsing;
        Ok(())
    }

    fn push_url(&mut self, url: Url) {
        self.gemspaces_nav.push(url);
        self.set_status_to_loading();
    }

    fn set_status_to_loading(&mut self) {
        self.scroll = (0, 0);
        self.status = AppStatus::Loading;
        self.content = None;
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
            let (link, text) = link_line
                .split_once(|x: char| x.is_whitespace())
                .unwrap_or((link_line, ""));
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
