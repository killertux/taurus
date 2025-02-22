use url::Url;

pub struct GemspaceNav {
    gemspaces: Vec<Url>,
    position: usize,
}

impl GemspaceNav {
    pub fn new(url: Url) -> Self {
        Self {
            gemspaces: Vec::from([url]),
            position: 0,
        }
    }

    pub fn push(&mut self, url: Url) {
        let len = self.gemspaces.len();
        ((self.position + 1)..len).for_each(|pos| {
            self.gemspaces.remove(pos);
        });
        self.gemspaces.push(url);
        self.position += 1;
    }

    pub fn current(&self) -> Url {
        self.gemspaces
            .get(self.position)
            .expect("We should always have a current URL")
            .clone()
    }

    pub fn back(&mut self) {
        if self.position > 0 {
            self.position -= 1;
        }
    }

    pub fn advance(&mut self) {
        self.position = (self.gemspaces.len() - 1).min(self.position + 1);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn current() {
        let url = Url::parse("gemini://test.com/").unwrap();
        let nav = GemspaceNav::new(url.clone());
        assert_eq!(url, nav.current());
    }

    #[test]
    fn push_back_advance() {
        let url_1 = Url::parse("gemini://test.com/1").unwrap();
        let url_2 = Url::parse("gemini://test.com/2").unwrap();
        let url_3 = Url::parse("gemini://test.com/3").unwrap();
        let mut nav = GemspaceNav::new(url_1.clone());
        assert_eq!(url_1, nav.current());
        nav.push(url_2.clone());
        assert_eq!(url_2, nav.current());
        nav.back();
        assert_eq!(url_1, nav.current());
        nav.push(url_3.clone());
        assert_eq!(url_3, nav.current());
        nav.back();
        assert_eq!(url_1, nav.current());
        nav.advance();
        assert_eq!(url_3, nav.current());
        nav.advance();
        assert_eq!(url_3, nav.current());
    }
}
