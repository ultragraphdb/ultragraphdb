/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a role client refuses before its peer could: a password SASLprep refuses, and a name whose escaped client-first
//! message no frame holds; and what it refuses of its peer: a reply on a stream other than its request's, which closes the
//! connection, while a frame on a stream it has ended is late.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_common_role::{
    Answer, Push, ROLE_ITERATIONS, RoleClient, Server, Session, listen, serve_connection,
};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame, TransportErrorEnum};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, ErrorCode, ErrorReply, FLAG_LAST, FrameView, Header, Held,
    MAX_FRAME_SIZE, Members,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::serve_tests::{CLUSTER, PASSWORD, bounded, credentials, free_tcp, serving};

/// A password SASLprep refuses, holding a control character, fails the dial as an authentication, before anything is sent.
#[test]
fn test_a_password_saslprep_refuses_fails_the_authentication() -> TestResult {
    bounded(async {
        let address = serving(&free_tcp()?, Endpoint::Compute).await?;
        let refused = RoleClient::dial(&address, CLUSTER, "compute", "s3\u{7}cret")
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(refused.0, TransportErrorEnum::Authentication { .. }),
            "{refused}"
        );
        Ok(())
    })
}

/// A name the Hello holds whose client-first message, each `,` escaped as three bytes, passes the largest frame is refused
/// as an Authenticate that cannot be built.
#[test]
fn test_a_name_too_long_to_authenticate_is_refused_as_a_frame_not_built() -> TestResult {
    bounded(async {
        let address = serving(&free_tcp()?, Endpoint::Compute).await?;
        let name = ",".repeat(MAX_FRAME_SIZE / 2);
        let refused = RoleClient::dial(&address, CLUSTER, &name, PASSWORD)
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(
                refused.0,
                TransportErrorEnum::FrameNotBuilt { command, .. }
                    if command == Command::Authenticate.number()
            ),
            "{refused}"
        );
        Ok(())
    })
}

/// Where [`Misdirecting`] answers each request after its first, which it answers on the request's stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Misdirect {
    /// The reply on a stream above the request's, which the client has not opened.
    Ahead,
    /// The reply on stream 0.
    Zero,
    /// A Held on the stream before the request's, which has ended, then the reply on the request's.
    Late,
    /// As [`Misdirect::Late`], the Held's body failing its checksum.
    LateCorrupted,
}

/// Answers each request with a Members, its first on the request's stream and each later as `how` says.
#[derive(Debug)]
struct Misdirecting {
    how: Misdirect,
    taken: u32,
    then: Option<(u64, Vec<u8>)>,
}

/// A Members frame on `stream`, flagged last.
fn members_on(stream: u64) -> Result<Vec<u8>, ProtocolError> {
    Members::new().frame(
        Header::new(CLUSTER, Command::Members)
            .with_stream(stream)
            .with_flags(FLAG_LAST),
    )
}

/// An Error frame on `stream`, its last byte changed: its header holds, its body's checksum fails. Not flagged last, so the
/// stream it leaves on stays open.
fn corrupted_on(stream: u64) -> Result<Vec<u8>, ProtocolError> {
    let mut frame = ErrorReply::new(ErrorCode::Refused, "a late refusal")
        .frame(Header::new(CLUSTER, Command::Error).with_stream(stream))?;
    if let Some(last) = frame.last_mut() {
        *last ^= 0xFF;
    }
    Ok(frame)
}

impl Misdirecting {
    fn answered(&mut self, stream: u64) -> Result<Vec<u8>, ProtocolError> {
        self.taken += 1;
        if self.taken == 1 {
            return members_on(stream);
        }
        let ended = stream.saturating_sub(1);
        match self.how {
            Misdirect::Ahead => members_on(stream + 100),
            Misdirect::Zero => members_on(0),
            Misdirect::Late => {
                self.then = Some((stream, members_on(stream)?));
                Held.frame(Header::new(CLUSTER, Command::Held).with_stream(ended))
            }
            Misdirect::LateCorrupted => {
                self.then = Some((stream, members_on(stream)?));
                corrupted_on(ended)
            }
        }
    }
}

impl Session for Misdirecting {
    async fn answer(&mut self, stream: u64, _frame: ReceivedFrame) -> Answer {
        self.answered(stream).map_or_else(
            |rule| Answer::Refuse(ErrorReply::protocol(&rule)),
            Answer::Reply,
        )
    }

    async fn next_push(&mut self) -> (u64, Push) {
        match self.then.take() {
            Some((stream, frame)) => (stream, Push::Frame(frame)),
            None => std::future::pending().await,
        }
    }
}

/// How long a test waits for the server to see the client's close.
const CLOSE_WAIT: Duration = Duration::from_secs(5);

/// A client of a server answering as `how` says, its first request answered, and whether the server's connection has ended.
async fn misdirected(how: Misdirect) -> TestResult<(RoleClient, Rc<Cell<bool>>)> {
    let listener = listen(&free_tcp()?).await?;
    let address = listener.local_address()?;
    let server = Server::new(CLUSTER, "test", Endpoint::System)?.with_iterations(ROLE_ITERATIONS);
    let held = credentials()?;
    let ended = Rc::new(Cell::new(false));
    let told = Rc::clone(&ended);
    let session = Misdirecting {
        how,
        taken: 0,
        then: None,
    };
    spawn(async move {
        if let Ok(carrier) = listener.accept().await {
            serve_connection(carrier, &server, held, session).await;
        }
        told.set(true);
    })
    .detach();
    let mut client = RoleClient::dial(&address, CLUSTER, "storage", PASSWORD).await?;
    drop(client.request(&Members::new()).await?);
    Ok((client, ended))
}

/// Waits until `ended` says the server's connection has ended, failing past [`CLOSE_WAIT`].
async fn until_closed(ended: &Cell<bool>, what: &str) -> TestResult {
    let closed = async {
        while !ended.get() {
            compio::time::sleep(Duration::from_millis(5)).await;
        }
    };
    compio::time::timeout(CLOSE_WAIT, closed)
        .await
        .map_err(|_| TestError::new(format!("{what}: the client kept its connection")))
}

/// §2.5 step 3, §2.6: a reply on a stream the client has not opened, or on stream 0, answers no request; the client refuses
/// it as a command not served and closes the connection.
#[test]
fn test_a_reply_on_another_stream_is_refused_and_closes_the_connection() -> TestResult {
    bounded(async {
        for how in [Misdirect::Ahead, Misdirect::Zero] {
            let (mut client, ended) = misdirected(how).await?;
            let refused = client
                .request(&Members::new())
                .await
                .err()
                .context("a refusal")?;
            assert_eq!(
                refused.0,
                TransportErrorEnum::Protocol {
                    error: ProtocolError::command_not_served(Command::Members.number())
                },
                "{how:?}"
            );
            until_closed(&ended, &format!("{how:?}")).await?;
        }
        Ok(())
    })
}

/// §2.5 step 3, §11.2: a frame on a stream the client has ended is late: checked whole and dropped, and the reply read after
/// it. One whose body fails its checksum is refused, which closes the connection (§2.6).
#[test]
fn test_a_late_frame_is_dropped_once_whole() -> TestResult {
    bounded(async {
        let (mut client, _ended) = misdirected(Misdirect::Late).await?;
        let reply = client.request(&Members::new()).await?;
        let view = reply.view()?;
        assert_eq!(
            (view.command(), view.header().stream()),
            (Command::Members, client.last_stream())
        );
        drop(reply);
        let (mut client, ended) = misdirected(Misdirect::LateCorrupted).await?;
        let corrupted = corrupted_on(client.last_stream())?;
        let rule = FrameView::validate(&corrupted, CLUSTER, MAX_FRAME_SIZE)
            .err()
            .context("the corrupted frame's rule")?;
        let refused = client
            .request(&Members::new())
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(refused.0, TransportErrorEnum::Protocol { error: rule });
        until_closed(&ended, "a late frame failing its checksum").await
    })
}
