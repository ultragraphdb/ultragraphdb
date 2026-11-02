/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::traits::dial::Dial;

/// Opens TCP connections: the first of the host's addresses that accepts within the timeout, its reads and writes
/// each bounded by the same timeout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TcpDial;

impl Dial for TcpDial {
    type Stream = TcpStream;

    fn dial(&self, host: &str, port: u16, timeout: Duration) -> io::Result<TcpStream> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, format!("no address for {host}"));
        for addr in (host, port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(stream) => {
                    stream.set_read_timeout(Some(timeout))?;
                    stream.set_write_timeout(Some(timeout))?;
                    return Ok(stream);
                }
                Err(e) => last = io::Error::new(e.kind(), format!("{addr}: {e}")),
            }
        }
        Err(last)
    }
}
