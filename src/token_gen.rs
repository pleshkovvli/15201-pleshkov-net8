extern crate mio;

use mio::Token;

pub struct TokenGen {
    id: usize
}

impl TokenGen {
    pub fn new() -> TokenGen {
        TokenGen {
            id: 0
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.id += 1;
        Token(self.id)
    }
}
