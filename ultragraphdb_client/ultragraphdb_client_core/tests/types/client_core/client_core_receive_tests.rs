/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the core makes of each frame the server sends it, apart from a request's ordinary answer: frames on stream 0,
//! Error frames whose body breaks its layout, frames whose chunks do not verify, and replies naming another request.

use ultragraphdb_client_core::{ClientError, ClientErrorEnum};
use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{
    Authenticate, BodyBuilder, Command, DropCommand, ErrorCode, ErrorReply, FLAG_LAST, Header,
    TermTable, Write,
};

use crate::utils::harness::{CLUSTER, Harness, begun, dropped, registered, server};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

/// A frame of `command` on `stream` whose body is `body`, as laid out by no command: its chunks verify, its layout
/// does not.
fn raw(command: Command, stream: u64, body: &[u8]) -> TestResult<Vec<u8>> {
    Ok(Header::new(CLUSTER, command)
        .with_stream(stream)
        .with_flags(FLAG_LAST)
        .encode_frame(body)?)
}

/// `frame` with its last byte flipped, so its last chunk no longer verifies.
fn flipped(mut frame: Vec<u8>) -> Vec<u8> {
    let last = frame.len() - 1;
    frame[last] ^= 4;
    frame
}

/// Whether `error` is a frame whose chunk did not verify.
fn chunk_refused(error: &ClientError) -> bool {
    matches!(&error.0, ClientErrorEnum::Protocol { error } if matches!(error.0, ProtocolErrorEnum::Chunk { index: 0, .. }))
}

/// Whether `error` is a frame breaking its command's layout: a validation rule other than the header's or the chunks'.
fn layout_refused(error: &ClientError) -> bool {
    matches!(&error.0, ClientErrorEnum::Protocol { error } if !matches!(
        error.0,
        ProtocolErrorEnum::Chunk { .. } | ProtocolErrorEnum::SequenceOrder { .. }
    ))
}

/// Stream 0 of the side that dialled carries Welcome, Authenticate and Error alone (`PROTOCOL.md` §2.5): any other
/// command, a role command's or a client request's, closes the connection, naming the command (§2.6).
#[test]
fn test_a_command_other_than_the_connections_own_on_stream_zero_closes_the_connection() -> TestResult
{
    let others = || {
        Command::ALL
            .into_iter()
            .filter(|command| !matches!(command, Command::Authenticate | Command::Error))
    };
    for command in others() {
        let mut h = Harness::ready(server())?;
        let frame = raw(command, 0, &[])?;
        let frame = h.numbered(&frame)?;
        assert_eq!(
            h.deliver(&frame)?,
            Err(ClientError::protocol(ProtocolError::command_not_served(
                command.number()
            ))),
            "{command:?}"
        );
        assert!(!h.core.is_connected(), "{command:?}");
    }
    // The writes in one frame and every role command are among those tried.
    assert!(others().any(|c| c == Command::DirectWrite));
    assert!(others().any(|c| c == Command::ConditionalWrite));
    assert_eq!(others().filter(|c| c.is_role()).count(), 20);
    Ok(())
}

/// A Welcome comes once, as the answer to the Hello: a later one closes the connection, naming the command.
#[test]
fn test_a_later_welcome_on_stream_zero_closes_the_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    let frame = raw(Command::Welcome, 0, &[])?;
    let frame = h.numbered(&frame)?;
    let refused = h.deliver(&frame)?.err().context("a refusal")?;
    let expected =
        ClientError::protocol(ProtocolError::command_not_served(Command::Welcome.number()));
    assert_eq!(refused.to_string(), expected.to_string());
    assert!(!h.core.is_ready());
    Ok(())
}

/// The server's closing Error on stream 0 whose body breaks its layout closes the connection with the rule it broke,
/// not with a code the body does not carry.
#[test]
fn test_a_malformed_closing_error_closes_the_connection_with_its_rule() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.register(h.now)?;
    while h.core.transmit().is_some() {}
    let frame = raw(Command::Error, 0, &[0xFF; 3])?;
    let frame = h.numbered(&frame)?;
    let refused = h.deliver(&frame)?.err().context("an error")?;
    assert!(layout_refused(&refused), "{refused:?}");
    assert!(!h.core.is_connected());
    assert_eq!(
        h.core.take_reply(p).context("an answer")?,
        Err(ClientError::connection_lost(Some(refused)))
    );
    Ok(())
}

/// A frame on stream 0 whose chunk does not verify closes the connection.
#[test]
fn test_a_damaged_frame_on_stream_zero_closes_the_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    let frame =
        ErrorReply::new(ErrorCode::Protocol, "bye").frame(Header::new(CLUSTER, Command::Error))?;
    let frame = h.numbered(&flipped(frame))?;
    let refused = h.deliver(&frame)?.err().context("an error")?;
    assert!(chunk_refused(&refused), "{refused:?}");
    assert!(!h.core.is_connected());
    Ok(())
}

/// An Authenticate with no exchange running is out of step: the connection is closed, naming the step it carries.
#[test]
fn test_an_authenticate_with_no_exchange_running_closes_the_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    let frame = Authenticate::new(3, "", b"m".to_vec())
        .frame(Header::new(CLUSTER, Command::Authenticate))?;
    let frame = h.numbered(&frame)?;
    assert_eq!(
        h.deliver(&frame)?,
        Err(ClientError::protocol(ProtocolError::field_value(
            Command::Authenticate.number(),
            "step",
            3
        )))
    );
    assert!(!h.core.is_connected());
    Ok(())
}

/// A refusal of a Write, whose body breaks the Error's layout or whose chunk does not verify, closes the connection
/// rather than ending the transaction with a refusal it does not carry.
#[test]
fn test_a_malformed_refusal_of_a_write_closes_the_connection() -> TestResult {
    for damaged in [false, true] {
        let mut h = Harness::ready(server())?;
        let p = h.core.begin(0, false, h.now)?;
        let tx = begun(h.settle(p)?)?;
        h.core
            .write(&tx, &Write::new(TermTable::new(), Vec::new()))?;
        while h.core.transmit().is_some() {}
        let frame = if damaged {
            flipped(
                ErrorReply::new(ErrorCode::Refused, "no")
                    .frame(Header::new(CLUSTER, Command::Error).with_stream(tx.stream()))?,
            )
        } else {
            raw(Command::Error, tx.stream(), &[0xFF; 3])?
        };
        let frame = h.numbered(&frame)?;
        let refused = h.deliver(&frame)?.err().context("an error")?;
        let rule = if damaged {
            chunk_refused(&refused)
        } else {
            layout_refused(&refused)
        };
        assert!(rule, "damaged {damaged}: {refused:?}");
        assert!(!h.core.is_connected());
        assert_eq!(
            h.core
                .write(&tx, &Write::new(TermTable::new(), Vec::new()))
                .err()
                .context("an error")?
                .0,
            ClientErrorEnum::StreamFinished {
                stream: tx.stream()
            }
        );
    }
    Ok(())
}

/// A request's refusal whose body breaks the Error's layout closes the connection, and the request is lost with it.
#[test]
fn test_a_malformed_refusal_of_a_request_closes_the_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.register(h.now)?;
    while h.core.transmit().is_some() {}
    let frame = raw(Command::Error, 1, &[0xFF; 3])?;
    let frame = h.numbered(&frame)?;
    let refused = h.deliver(&frame)?.err().context("an error")?;
    assert!(layout_refused(&refused), "{refused:?}");
    assert_eq!(
        h.core.take_reply(p).context("an answer")?,
        Err(ClientError::connection_lost(Some(refused)))
    );
    Ok(())
}

/// A Drop's reply naming another session than the one it was sent through is no answer of it.
#[test]
fn test_a_drop_reply_of_another_session_is_a_mismatch() -> TestResult {
    let mut h = Harness::ready(server())?;
    let registering = h.core.register(h.now)?;
    let s = registered(h.settle(registering)?)?;
    let p = h
        .core
        .drop_command(DropCommand::whole_tenant(0), s, h.now)?;
    while h.core.transmit().is_some() {}
    let version = h.server.version();
    let other = s.number() + 1;
    // The registration took stream 1; the drop takes stream 2.
    let reply = DropCommand::whole_tenant(0)
        .with_session(other.to_be_bytes(), 1)
        .with_commit_version(version)
        .with_token(ultragraphdb_client_core::TestServer::token(version))
        .frame(
            Header::new(CLUSTER, Command::Drop)
                .with_stream(2)
                .with_flags(FLAG_LAST),
        )?;
    let reply = h.numbered(&reply)?;
    h.deliver(&reply)??;
    assert_eq!(
        h.core.take_reply(p).context("an answer")?,
        Err(ClientError::reply_mismatch("session", s.number(), other))
    );
    assert_eq!(h.core.next_request(s), Some(1), "no drop was answered");
    Ok(())
}

/// A drop answered with its commit version and request advances the session, as a commit does.
#[test]
fn test_a_drop_answers_its_version_and_request() -> TestResult {
    let mut h = Harness::ready(server())?;
    let registering = h.core.register(h.now)?;
    let s = registered(h.settle(registering)?)?;
    let p = h
        .core
        .drop_command(DropCommand::whole_tenant(0), s, h.now)?;
    let d = dropped(h.settle(p)?)?;
    assert_eq!(
        (d.version(), d.request(), d.results().len()),
        (h.server.version(), 1, 0)
    );
    assert_eq!(h.core.next_request(s), Some(2));
    assert_eq!(h.server.applied(), 1);
    Ok(())
}

/// Connecting again while connected gives the old connection up: its requests are lost, with nothing more to say.
#[test]
fn test_connecting_again_loses_the_requests_of_the_old_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.register(h.now)?;
    while h.core.transmit().is_some() {}
    h.core.connect(h.now, h.unix())?;
    assert!(h.core.is_connected() && !h.core.is_ready());
    assert_eq!(
        h.core.take_reply(p).context("an answer")?,
        Err(ClientError::connection_lost(None))
    );
    assert_eq!(h.core.take_reply(p), None, "answered once");
    Ok(())
}

/// A reply of the command its request asks for whose body breaks the command's layout closes the connection, and the
/// request is lost with it.
#[test]
fn test_a_malformed_reply_of_the_right_command_closes_the_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.register(h.now)?;
    while h.core.transmit().is_some() {}
    let frame = raw(Command::Register, 1, &[0xFF; 3])?;
    let frame = h.numbered(&frame)?;
    let refused = h.deliver(&frame)?.err().context("an error")?;
    assert!(layout_refused(&refused), "{refused:?}");
    assert!(!h.core.is_connected());
    assert_eq!(
        h.core.take_reply(p).context("an answer")?,
        Err(ClientError::connection_lost(Some(refused)))
    );
    Ok(())
}

/// Before the handshake's end, the side that dialled takes a Welcome or an Error on stream 0, and nothing on another
/// stream: anything else is refused by its command at step 3 (`PROTOCOL.md` §2.5), which no later check precedes.
#[test]
fn test_the_handshake_refuses_what_it_does_not_serve_by_its_command() -> TestResult {
    use ultragraphdb_protocol_wire::{BodyBuilder as _, Welcome};
    let welcome = Welcome::new(1, 0, "s")
        .with_max_frame(1 << 20)
        .with_max_streams(8)
        .with_initial_credit(1);
    let on_stream_one = welcome.frame(Header::new(CLUSTER, Command::Welcome).with_stream(1))?;
    let register = raw(Command::Register, 0, &[])?;
    let error_on_stream_one = raw(Command::Error, 1, &[])?;
    for (frame, command) in [
        (on_stream_one, Command::Welcome),
        (register, Command::Register),
        (error_on_stream_one, Command::Error),
    ] {
        let mut h = Harness::new(server());
        h.connect()?;
        let frame = crate::types::client_core::client_core_conformance_tests::numbered(&frame, 1);
        assert_eq!(
            h.deliver(&frame)?,
            Err(ClientError::protocol(ProtocolError::command_not_served(
                command.number()
            ))),
            "{command:?}"
        );
        assert!(!h.core.is_connected(), "{command:?}");
    }
    Ok(())
}

/// A frame on a stream at or above the next the client will open is refused whatever its command, before its chunks.
#[test]
fn test_a_stream_not_yet_opened_is_refused_before_its_chunks() -> TestResult {
    let mut h = Harness::ready(server())?;
    let frame = flipped(raw(Command::Error, 1, b"chunk")?);
    let frame = h.numbered(&frame)?;
    assert_eq!(
        h.deliver(&frame)?,
        Err(ClientError::protocol(ProtocolError::command_not_served(
            Command::Error.number()
        )))
    );
    Ok(())
}

/// An open transaction's stream, with no request waiting, takes an Error alone.
#[test]
fn test_an_idle_transactions_stream_takes_only_an_error() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.begin(0, false, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let frame = raw(Command::Result, tx.stream(), &[])?;
    let frame = h.numbered(&frame)?;
    assert_eq!(
        h.deliver(&frame)?,
        Err(ClientError::protocol(ProtocolError::command_not_served(
            Command::Result.number()
        )))
    );
    assert!(!h.core.is_connected());
    Ok(())
}

/// A late frame takes steps 4 to 15: whatever its command, it is dropped, unless it breaks a rule, which closes the
/// connection.
#[test]
fn test_a_late_frame_is_checked_and_then_dropped_whatever_its_command() -> TestResult {
    for command in [
        Command::Result,
        Command::Commit,
        Command::Entries,
        Command::Welcome,
    ] {
        let mut h = Harness::ready(server())?;
        let p = h.core.register(h.now)?;
        let _session = registered(h.settle(p)?)?;
        // The Register took stream 1 and ended it.
        let frame = raw(command, 1, &[])?;
        let frame = h.numbered(&frame)?;
        assert_eq!(h.deliver(&frame)?, Ok(()), "{command:?}");
        assert!(h.core.is_ready(), "{command:?}");
    }
    let mut h = Harness::ready(server())?;
    let p = h.core.register(h.now)?;
    let _session = registered(h.settle(p)?)?;
    let frame = flipped(raw(Command::Result, 1, b"chunk")?);
    let frame = h.numbered(&frame)?;
    let refused = h.deliver(&frame)?.err().context("refused")?;
    assert!(chunk_refused(&refused), "{refused:?}");
    assert!(!h.core.is_connected());
    Ok(())
}

/// A frame behind a read's `LAST`, or behind a transaction's refusal, falls on a stream that has ended, so it is late.
#[test]
fn test_a_frame_behind_a_streams_end_is_late() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.begin(0, true, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let read = crate::types::client_core::client_core_results_tests::read(&tx);
    let stream = h.core.read(&tx, &read, h.now)?;
    while h.step()? {}
    let frame = raw(Command::Begin, stream.stream(), &[])?;
    let frame = h.numbered(&frame)?;
    assert_eq!(
        h.deliver(&frame)?,
        Ok(()),
        "the read's LAST is in its inbox"
    );
    assert!(h.core.is_ready());
    let mut other = Harness::ready(server())?;
    let beginning = other.core.begin(0, false, other.now)?;
    let writer = begun(other.settle(beginning)?)?;
    let refusal = ErrorReply::new(ErrorCode::Refused, "no").frame(
        Header::new(CLUSTER, Command::Error)
            .with_stream(writer.stream())
            .with_flags(FLAG_LAST),
    )?;
    let first = other.numbered(&refusal)?;
    other.deliver(&first)??;
    let second = other.numbered(&refusal)?;
    assert_eq!(
        other.deliver(&second)?,
        Ok(()),
        "the transaction's stream has ended"
    );
    Ok(())
}
