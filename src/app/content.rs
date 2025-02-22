use anyhow::Result;

pub struct Content {
    pub mime: String,
    pub body: Body,
}

pub enum Body {
    String(String),
    Bytes(Vec<u8>),
}

impl Content {
    pub fn from_mime_and_bytes(mime: String, bytes: Vec<u8>) -> Result<Self> {
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
