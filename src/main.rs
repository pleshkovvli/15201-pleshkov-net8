extern crate mio;

mod token_gen;

use token_gen::TokenGen;

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

    let mut connections: HashMap<Token, Rc<RefCell<Connection>>> = HashMap::new();

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
                handle_server_event(&r_host, &listener, &poll, &mut token_gen, &mut connections);
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
