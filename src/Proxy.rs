use std::io::{Read, Write, Result};
use std::marker::PhantomData;
use std::cmp;
use std::ptr;


struct Proxy<R: Read, W: Write> {
    buffer: Box<[u8]>,
    offset: usize,
    pub state: ProxyState,
    process_state: ProcessState,
    method: HttpMethod,
    ph_read: PhantomData<R>,
    ph_write: PhantomData<W>,
}

enum HttpMethod {
    Get = "GET",
    Head = "HEAD",
    Post = "POST",
    Invalid,
}

impl HttpMethod {
    fn valid(piece: &str) -> Option<Headers> {
        if piece.len() < 4 {
            let len = piece.len();
            if piece[..len] != HttpMethod::Get[..len]
                && piece[..len] != HttpMethod::Post[..len]
                && piece[..len] != HttpMethod::Head[..len] {
                Some(HttpMethod::Invalid)
            } else {
                None
            }
        } else if piece.len() >= 4 && piece[0..3] == HttpMethod::Get && piece[4] == ' ' {
            Some(HttpMethod::Get)
        } else if piece.len() >= 5 && (piece[0..4] == HttpMethod::Post && piece[5] == ' ') {
            Some(HttpMethod::Post)
        } else if piece.len() >= 5 && (piece[0..4] == HttpMethod::Head) && piece[5] == ' ' {
            Some(HttpMethod::Head)
        } else {
            Some(HttpMethod::Invalid)
        }
    }
}

enum ProxyState {
    Request,
    Response,
}

enum ProcessState {
    Method,
    Protocol(usize),
    Host(usize),
    Path(usize),
    Version(usize),
    Headers(usize),
    Body(usize),
}

fn valid_protocol(piece: &str) -> Option<bool> {
    let len = piece.len();
    let http = "http://"[..len];
    match piece[..len] {
        http => if len >= 7 {
            Some(true)
        } else {
            None
        }
        _ => Some(false)
    }
}

fn valid_version(piece: &str) -> Option<bool> {
    let len = piece.len();
    let http1_0 = "HTTP/1.0\r\n"[..len];
    let http1_1 = "HTTP/1.1\r\n"[..len];
    match piece[..len] {
        http1_0 => if len >= 10 {
            Some(true)
        } else {
            None
        }
        _ => Some(false)
    }
}

impl<R: Read, W: Write> Proxy<R, W> {
    fn new(buf_size: usize) -> Proxy<R, W> {
        Proxy {
            buffer: Box::from(vec![0; buf_size]),
            offset: 0,
            state: ProxyState::Request,
            process_state: ProcessState::Method,
            method: HttpMethod::Invalid,
            ph_read: PhantomData,
            ph_write: PhantomData,
        }
    }

    pub fn read(&mut self, src: &mut R) -> Result<ProxyResult> {
        let read = src.read(&mut self.buffer[self.offset..])?;
        if read == 0 {
            return Ok(ProxyResult::Close);
        }
        self.offset += read;

        if self.state == ProxyState::Response {
            return if self.offset >= 4 {
                match buffer[self.offset - 4..self.offset] {
                    "\r\n\r\r" => Ok(ProxyResult::ChangeChannelState),
                    _ => Ok(ProxyResult::Continue)
                }
            } else {
                Ok(ProxyResult::Continue)
            };
        }

        match self.process_state {
            ProcessState::Method =>
                self.method = match HttpMethod::valid(self.buffer[..self.offset]) {
                    Some(value) => {
                        if value == HttpMethod::Invalid {
                            return ProxyResult::Close;
                        }


                        self.state = ProcessState::Protocol(value.to_string().len() + 1);

                        value
                    }
                    None => { HttpMethod::Invalid }
                },
            ProcessState::Protocol(start) =>
                match valid_protocol(self.buffer[start..]) {
                    Some(value) => if value {
                        self.process_state = ProcessState::Host(start + 7)
                    } else {
                        return ProxyResult::Close;
                    },
                    None => {}
                }
            ProcessState::Host(start) => {
                match self.buffer[start..].find('/') {
                    Some(index) => {
                        unsafe {
                            ptr::copy(
                                &self.buffer[index],
                                &self.buffer[start],
                                self.offset - index,
                            );
                        }
                        self.offset -= start - index;
                        self.process_state = ProcessState::Path(start)
                    }
                    None => {}
                }
            }
            ProcessState::Path(start) => {
                match self.buffer[start..].find(' ') {
                    Some(index) => self.process_state = ProcessState::Version(index + 1),
                    None => {}
                }
            }
            ProcessState::Version => {
                match valid_version(self.buffer[start..]) {
                    Some(value) => if value {
                        self.process_state = ProcessState::Headers(start + 10)
                    } else {
                        return ProxyResult::Close;
                    },
                    None => {}
                }
            }
            ProcessState::Headers(start) => {
                match self.buffer[start..].find("\r\n") {
                    Some(index) => {
                        if index == start {
                            return Ok(ProxyResult::ChangeChannelState)
                        }
                        match self.buffer[start..index].find("Connection:") {
                            Some(index_con) => {
                                let con_close = "Connection: close\r\n";
                                let len = con_close.len();
//                                self.buffer[start..len].copy_from_slice(con_close);
//                                self.buffer[len..].copy_from_slice(self.buffer[len..]);
//                                self.process_state = ProcessState::Headers(start + len)
                            }
                        }
                    }
                    None => {}
                }
            }
            ProcessState::Body(start) => {
                match self.buffer[start..].find("\r\n\r\n") {
                    Some(index) => {
                        self.offset = index + 4;
                        return Ok(ProxyResult::ChangeChannelState)
                    }
                    None => {}
                }
            }
        }
    }
}

pub enum ProxyResult {
    Continue,
    ChangeChannelState,
    Close,
}