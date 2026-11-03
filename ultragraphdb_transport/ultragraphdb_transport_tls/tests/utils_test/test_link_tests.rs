/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{self, ErrorKind, Read, Write};

use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transport_tls::{TestLink, TestPki, TlsError, TlsErrorEnum};

/// A carrier of the tests' own: what it does with every read and every write.
#[derive(Debug)]
struct Carrier {
    read: Option<ErrorKind>,
    write: Option<ErrorKind>,
}

impl Read for Carrier {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(self.read.unwrap_or(ErrorKind::WouldBlock).into())
    }
}

impl Write for Carrier {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.write {
            Some(kind) => Err(kind.into()),
            None => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A carrier that takes every write and has nothing to read.
fn void() -> Carrier {
    Carrier {
        read: None,
        write: None,
    }
}

/// The error a failed carrier operation of `kind` reads as.
fn refused(kind: ErrorKind) -> TlsError {
    TlsError(TlsErrorEnum::ConfigurationRefused(
        io::Error::from(kind).to_string(),
    ))
}

#[test]
fn test_a_link_holds_its_session() -> TestResult {
    let pki = TestPki::new()?;
    let link = TestLink::new(pki.connector()?.session("localhost")?, void());
    assert_eq!(link.session().host(), "localhost");
    assert!(link.session().is_handshaking());
    Ok(())
}

#[test]
fn test_a_pump_fails_on_a_carrier_that_fails_its_write_or_its_read() -> TestResult {
    let pki = TestPki::new()?;
    // A client's hello waits to be written: the write fails.
    let mut broken = TestLink::new(
        pki.connector()?.session("localhost")?,
        Carrier {
            read: None,
            write: Some(ErrorKind::BrokenPipe),
        },
    );
    assert_eq!(broken.pump(), Err(refused(ErrorKind::BrokenPipe)));
    // A server has nothing to write and reads: the read fails with more than `WouldBlock`.
    let mut reset = TestLink::new(
        pki.acceptor()?.session()?,
        Carrier {
            read: Some(ErrorKind::ConnectionReset),
            write: None,
        },
    );
    assert_eq!(reset.pump(), Err(refused(ErrorKind::ConnectionReset)));
    // `WouldBlock` is no failure: nothing has arrived yet.
    let mut waiting = TestLink::new(pki.acceptor()?.session()?, void());
    assert_eq!(waiting.pump(), Ok(()));
    Ok(())
}

#[test]
fn test_an_exchange_with_no_answer_fails_after_its_turns() -> TestResult {
    let pki = TestPki::new()?;
    // Neither carrier delivers anything: the line never reaches the server.
    let mut client = TestLink::new(pki.connector()?.session("localhost")?, void());
    let mut server = TestLink::new(pki.connector()?.session("localhost")?, void());
    let Err(error) = TestLink::exchange_line(&mut client, &mut server, b"ping\n") else {
        return Err(TestError::new(
            "a line was echoed over carriers that deliver nothing",
        ));
    };
    assert_eq!(
        error.0,
        TlsErrorEnum::ConfigurationRefused("no answer after 1000 turns".to_string())
    );
    Ok(())
}
