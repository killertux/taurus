use anyhow::Result;
use url::Url;

pub struct GemTextParser<'a> {
    raw_text: &'a str,
    url: Url,
}

#[derive(Debug)]
pub enum GemTextLine<'a> {
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
        if let Some(link_line) = line.strip_prefix("=>") {
            let (link, text) = link_line
                .trim()
                .split_once(|x: char| x.is_whitespace())
                .unwrap_or((link_line, ""));
            if !link.contains("://") {
                return Ok(GemTextLine::Link {
                    url: self.url.join(link.trim())?,
                    text,
                });
            }
            return Ok(GemTextLine::Link {
                url: Url::parse(link.trim())?,
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
