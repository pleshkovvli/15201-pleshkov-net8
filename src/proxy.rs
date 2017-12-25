use std::io::{Read, Write, Result};
use std::marker::PhantomData;
use std::cmp;
use std::ptr;

pub struct Proxy<R: Read, W: Write> {
    buffer: Box<[u8]>,
    offset: usize,
    length: usize,
    state: ProxyState,
    process_state: ProcessState,
    current_method: HttpMethod,
    ph_read: PhantomData<R>,
    ph_write: PhantomData<W>,
}

enum HttpMethod {
    Get = "GET",
    Head = "HEAD",
    Post = "POST",
    Invalid
}

impl HttpMethod {
    const MIN_METHOD_LENGTH: usize = 4;

    fn eq_method_with_whitespace(&self, piece: &str) -> bool {
        let method_len = self.to_string().len();
        let space_position = method_len + 1;

        piece.len() >= (space_position)
            && (piece[0..method_len] == self.to_string())
            && (piece[space_position] == ' ')
    }

    fn eq_method(&self, piece: &str, len: usize) -> bool {
        piece[..len] == self.to_string()[..len]
    }

    fn valid(piece: &str) -> Option<HttpMethod> {
        if piece.len() < MIN_METHOD_LENGTH {
            let len = piece.len();
            let not_fitting = !HttpMethod::Get.eq_method(piece, len)
                && !HttpMethod::Head.eq_method(piece, len)
                && !HttpMethod::Post.eq_method(piece, len);
            if not_fitting {
                Some(HttpMethod::Invalid)
            } else {
                None
            }
        } else if HttpMethod::Get.eq_method_with_whitespace(piece) {
            Some(HttpMethod::Get)
        } else if HttpMethod::Head.eq_method_with_whitespace(piece) {
            Some(HttpMethod::Head)
        } else if HttpMethod::Post.eq_method_with_whitespace(piece) {
            Some(HttpMethod::Post)
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

const CRLF: &str = "\r\n\r\r";

impl<R: Read, W: Write> Proxy<R, W> {
    pub fn new(buf_size: usize) -> Proxy<R, W> {
        Proxy {
            buffer: Box::from(vec![0; buf_size]),
            offset: 0,
            length: 0,
            state: ProxyState::Request,
            process_state: ProcessState::Method,
            current_method: HttpMethod::Invalid,
            ph_read: PhantomData,
            ph_write: PhantomData,
        }
    }

    pub fn write(&mut self, dest: &mut W) -> Result<ProxyResult> {
        let write = dest.write(self.buffer[self.length])?;
        self.length += write;

        //if self.length
    }

    pub fn read(&mut self, src: &mut R) -> Result<ProxyResult> {
        let read = src.read(&mut self.buffer[self.length..])?;
        if read == 0 {
            return Ok(ProxyResult::Close);
        }
        self.length += read;

        if self.state == ProxyState::Response {
            return if self.length >= 4 {
                match self.buffer[(self.length - 4)..self.length] {
                    CRLF => Ok(ProxyResult::ChangeChannelState),
                    _ => Ok(ProxyResult::Continue)
                }
            } else {
                Ok(ProxyResult::Continue)
            };
        }

        match self.process_state {
            ProcessState::Method => match self.process_method() {
                Some(v) => return v,
                None => {}
            },
            ProcessState::Protocol(start) => match self.process_protocol(start) {
                Some(v) => return v,
                None => {}
            },
            ProcessState::Host(start) => match self.process_host(start) {
                Some(v) => return v,
                None => {}
            },
            ProcessState::Path(start) => match self.process_path(start) {
                Some(v) => return v,
                None => {}
            },
            ProcessState::Version(start) => match self.process_version(start) {
                Some(v) => return v,
                None => {}
            },
            ProcessState::Headers(start) => match self.process_headers(start) {
                Some(v) => return v,
                None => {}
            },
            ProcessState::Body(start) => match self.process_headers(start) {
                Some(v) => return v,
                None => {}
            },
        }
    }

    fn process_method(&mut self) -> Option<ProxyResult> {
        self.current_method = match HttpMethod::valid(self.buffer[..self.length]) {
            Some(value) => {
                if value == HttpMethod::Invalid {
                    return Some(ProxyResult::Close);
                }

                self.state = ProcessState::Protocol(value.to_string().len() + 1);

                value
            }
            None => { HttpMethod::Invalid }
        };

        None
    }

    fn process_protocol(&mut self, start: usize) -> Option<ProxyResult> {
        match valid_protocol(self.buffer[start..]) {
            Some(value) => if value {
                self.process_state = ProcessState::Host(start + 7)
            } else {
                return ProxyResult::Close;
            },
            None => {}
        }

        None
    }

    fn process_host(&mut self, start: usize) {
        match self.buffer[start..].find('/') {
            Some(index) => {
                unsafe {
                    ptr::copy(
                        &self.buffer[index],
                        &self.buffer[start],
                        self.length - index,
                    );
                }
                self.length -= start - index;
                self.process_state = ProcessState::Path(start)
            }
            None => {}
        }
    }

    fn process_path(&mut self, start: usize) {
        match self.buffer[start..].find(' ') {
            Some(index) => self.process_state = ProcessState::Version(index + 1),
            None => {}
        }
    }

    fn process_version(&mut self, start: usize) -> Option<ProxyResult> {
        match valid_version(self.buffer[start..]) {
            Some(value) => if value {
                self.process_state = ProcessState::Headers(start + 10)
            } else {
                return ProxyResult::Close;
            },
            None => {}
        }

        None
    }

    fn process_headers(&mut self, start: usize) -> Option<ProxyResult> {
        match self.buffer[start..].find("\r\n") {
            Some(index) => {
                if index == start {
                    return Some(ProxyResult::ChangeChannelState)
                }
                match self.buffer[start..index].find("Connection:") {
                    Some(index_con) => {
                        let con_close = "Connection: close\r\n";
                        let len = con_close.len();
                        self.buffer[start..len].copy_from_slice(con_close);
                        self.buffer[len..].copy_from_slice(self.buffer[len..]);
                        self.process_state = ProcessState::Headers(start + len)
                    }
                }
            }
            None => {}
        }
    }

    fn process_body(&mut self, start: usize) {
        match self.buffer[start..].find("\r\n\r\n") {
            Some(index) => {
                self.length = index + 4;
                return Ok(ProxyResult::ChangeChannelState)
            }
            None => {}
        }
    }

}

pub enum ProxyResult {
    Continue,
    ChangeChannelState,
    Close,
}