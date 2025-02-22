use url::Url;

pub struct GemspaceNav {
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
        let len = self.gemspaces.len();
        (self.position..len).for_each(|pos| {
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
