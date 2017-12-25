extern crate mio;

use proxy::*;

use std::net::Shutdown;

use mio::net::TcpStream;
use mio::Token;
use mio::Ready;
use mio::Event;
use std::io::Result;

const BUFFER_SIZE: usize = 8 * 1024;


pub struct HttpConnection {
    client: TokenStream,
    server: Option<TokenStream>,
    proxy: Proxy<TcpStream, TcpStream>
}

impl HttpConnection {
    pub fn new(client: TokenStream) -> HttpConnection {
        HttpConnection {
            client,
            server: None,
            proxy: Proxy::new(BUFFER_SIZE)
        }
    }
}

pub enum HttpConnectionResult<'a> {
    Continue(TokenReady<'a>, TokenReady<'a>),
    Close,
}

pub struct TokenStream {
    pub token: Token,
    pub stream: TcpStream,
}

pub struct TokenReady<'a> {
    pub token: Token,
    pub stream: &'a TcpStream,
    pub ready: Ready,
}