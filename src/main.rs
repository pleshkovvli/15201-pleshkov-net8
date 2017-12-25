extern crate mio;

mod token_gen;
mod http_connection;
mod proxy;

use token_gen::TokenGen;
use http_connection::HttpConnection;

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;

use std::clone::Clone;
use std::env;
use std::process::exit;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};

use mio::Token;
use mio::*;
use mio::Poll;
use mio::net::{TcpListener, TcpStream};

const EXIT_FAILURE: i32 = 1;

const MAX_CONNECTIONS_COUNT: usize = 1024;

const HTTP_PROXY_PORT: u16 = 80;

fn main() {
    let localaddr = sock_addr_ip_unspecified(HTTP_PROXY_PORT);

    let listener = match TcpListener::bind(&localaddr) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("Failed to bind server socket: {}", e);
            exit(EXIT_FAILURE);
        }
    };

    let poll = Poll::new()
        .expect("Fatal error: failed to init poll");

    let mut token_gen = TokenGen::new();
    let server_token = token_gen.next_token();

    poll.register(&listener, server_token, Ready::readable(), PollOpt::level())
        .expect("Fatal error: failed to register server socket");

    let mut events = Events::with_capacity(MAX_CONNECTIONS_COUNT);

    let mut connections: HashMap<Token, Rc<RefCell<HttpConnection>>> = HashMap::new();

    loop {
        match poll.poll(&mut events, None) {
            Ok(event_count) => event_count,
            Err(e) => {
                eprintln!("Poll error: {}", e);
                exit(EXIT_FAILURE);
            }
        };

        for event in events.iter() {
            let token = event.token();

            if token == server_token {
                handle_server_event(&listener, &poll, &mut token_gen, &mut connections);
            } else {
                match handle_client_event(event, token, &poll, &mut connections) {
                    Some(tokens) => {
                        connections.remove(&tokens.0);
                        connections.remove(&tokens.1);
                    }
                    None => {}
                }
            }
        }
    }
}


fn handle_server_event(
    listener: &TcpListener,
    poll: &Poll,
    token_gen: &mut TokenGen,
    connections: &mut HashMap<Token, Rc<RefCell<HttpConnection>>>,
) {
    let client: TcpStream = match listener.accept() {
        Ok(result) => result.0,
        Err(e) => {
            eprintln!("Accept client error: {}", e);
            return;
        }
    };

    if connections.len() >= MAX_CONNECTIONS_COUNT {
        return;
    }

    let client_token = token_gen.next_token();

    poll.register(&client, client_token, Ready::readable(), PollOpt::level())
        .expect("Failed to register tcp stream");

    let connection = HttpConnection::new(
        TokenStream {
            stream: client,
            token: client_token,
        }
    );

    let rc_connection = Rc::new(RefCell::new(connection));
    connections.insert(client_token, Rc::clone(&rc_connection));
}

fn sock_addr_ip_unspecified(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(<Ipv4Addr>::unspecified()), port)
}


fn handle_client_event(
    event: Event,
    token: Token,
    poll: &Poll,
    connections: &mut HashMap<Token, Rc<RefCell<HttpConnection>>>,
) -> Option<(Token, Token)> {
    let mut connection = match connections.get(&token) {
        Some(ref connection) => connection.borrow_mut(),
        None => return None,
    };

    let tokens = connection.tokens();

    let ready: Option<(TokenReady, TokenReady)> = match connection.handle_event(token, event) {
        Ok(result) => match result {
            ConnectionResult::Continue(token, other) => Some((token, other)),
            ConnectionResult::Close => {
                None
            }
        },
        Err(e) => {
            eprintln!("Connection broken: {}", e);
            None
        }
    };

    match ready {
        Some(values) => {
            poll.reregister(values.0.stream, values.0.token, values.0.ready, PollOpt::level())
                .expect("Failed to register");

            poll.reregister(values.1.stream, values.1.token, values.1.ready, PollOpt::level())
                .expect("Failed to register");

            None
        }
        None => Some(tokens)
    }
}
