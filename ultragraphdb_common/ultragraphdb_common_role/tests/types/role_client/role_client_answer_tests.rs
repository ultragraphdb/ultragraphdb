/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! [`RoleClient::next_answer`], the one rule by which the side that dialled reads what answers a request (`PROTOCOL.md`
//! §2.5, step 3; §2.6), each branch over a server that sends what the test scripts: the request's stream, a late frame,
//! the peer's Error on either stream, stream 0, a stream not opened, a command the request is not answered by, and every
//! frame the client refuses closing the connection. [`RoleClient::request`] reads by the same rule.

use std::collections::VecDeque;
use std::time::Duration;

use compio::runtime::spawn;
use futures::channel::oneshot;
use ultragraphdb_common_role::{
    Answer, Answered, Push, ROLE_ITERATIONS, RoleClient, Server, Session, listen, serve_connection,
};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame, TransportErrorEnum};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorCode, ErrorReply, ErrorReplyView, FLAG_LAST, FrameView,
    Header, Held, MAX_FRAME_SIZE, Members, MembersView,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::serve_tests::{CLUSTER, PASSWORD, bounded, credentials, free_tcp};

/// The stream of the client's first request, which [`Scripted`] answers and so ends: a frame on it afterwards is late.
const ENDED: u64 = 1;

/// The stream of the client's second request, which [`Scripted`] answers with its script.
const OWN: u64 = 2;

/// The stream the client opens next, past [`OWN`]: the lowest stream it has not opened.
const NEXT: u64 = 3;

/// A stream far past any the client has opened.
const AHEAD: u64 = 102;

/// How long a test waits for the server to see the client's close.
const CLOSE_WAIT: Duration = Duration::from_secs(5);

/// Answers the client's second request with `script`, each push in turn on that request's stream, whatever stream a
/// pushed frame's own header names; every other request with an empty Members flagged last on its stream.
#[derive(Debug)]
struct Scripted {
    script: VecDeque<Push>,
    taken: u64,
    open: Option<u64>,
}

impl Session for Scripted {
    async fn answer(&mut self, stream: u64, _frame: ReceivedFrame) -> Answer {
        self.taken += 1;
        if self.taken == OWN {
            self.open = Some(stream);
            return Answer::Open;
        }
        members_on(stream, 0, FLAG_LAST).map_or_else(
            |rule| Answer::Refuse(ErrorReply::protocol(&rule)),
            Answer::Reply,
        )
    }

    async fn next_push(&mut self) -> (u64, Push) {
        let Some(stream) = self.open else {
            return std::future::pending().await;
        };
        match self.script.pop_front() {
            Some(push) => (stream, push),
            None => std::future::pending().await,
        }
    }
}

/// A Members frame of `count` compute members on `stream`, flagged `flags`.
fn members_on(stream: u64, count: usize, flags: u8) -> Result<Vec<u8>, ProtocolError> {
    let mut members = Members::new();
    (0..count).for_each(|member| members.push_compute(format!("tcp://compute-{member}:7082")));
    members.frame(
        Header::new(CLUSTER, Command::Members)
            .with_stream(stream)
            .with_flags(flags),
    )
}

/// An Error frame of `code` saying `message` on `stream`, not flagged last, so the stream it leaves on stays open.
fn error_on(stream: u64, code: ErrorCode, message: &str) -> Result<Vec<u8>, ProtocolError> {
    ErrorReply::new(code, message).frame(Header::new(CLUSTER, Command::Error).with_stream(stream))
}

/// An Error frame on `stream` whose body is empty: its header and checksums hold, its body breaks the Error's rules.
fn unreadable_error_on(stream: u64) -> Result<Vec<u8>, ProtocolError> {
    Header::new(CLUSTER, Command::Error)
        .with_stream(stream)
        .encode_frame(&[])
}

/// A Held frame on `stream`, not flagged last.
fn held_on(stream: u64) -> Result<Vec<u8>, ProtocolError> {
    Held.frame(Header::new(CLUSTER, Command::Held).with_stream(stream))
}

/// A Members frame on `stream` whose last body byte is changed: its header holds, its body's checksum fails.
fn corrupted_on(stream: u64) -> Result<Vec<u8>, ProtocolError> {
    let mut frame = members_on(stream, 1, 0)?;
    if let Some(last) = frame.last_mut() {
        *last ^= 0xFF;
    }
    Ok(frame)
}

/// The script of `frames`, each pushed whole.
fn frames(frames: impl IntoIterator<Item = Vec<u8>>) -> VecDeque<Push> {
    frames.into_iter().map(Push::Frame).collect()
}

/// A client of a server answering its second request with `script`, its first request answered, and what tells of the
/// server's connection ending.
async fn scripted(script: VecDeque<Push>) -> TestResult<(RoleClient, oneshot::Receiver<()>)> {
    let listener = listen(&free_tcp()?).await?;
    let address = listener.local_address()?;
    let server = Server::new(CLUSTER, "test", Endpoint::System)?.with_iterations(ROLE_ITERATIONS);
    let held = credentials()?;
    let (told, ended) = oneshot::channel();
    let session = Scripted {
        script,
        taken: 0,
        open: None,
    };
    spawn(async move {
        if let Ok(carrier) = listener.accept().await {
            serve_connection(carrier, &server, held, session).await;
        }
        // A test gone waits for nothing.
        let _told: Result<(), ()> = told.send(());
    })
    .detach();
    let mut client = RoleClient::dial(&address, CLUSTER, "storage", PASSWORD).await?;
    drop(client.request(&Members::new()).await?);
    assert_eq!(client.last_stream(), ENDED);
    Ok((client, ended))
}

/// A client as [`scripted`] gives it, its second request sent and unread: what `script` holds is what answers it.
async fn asked(script: VecDeque<Push>) -> TestResult<(RoleClient, oneshot::Receiver<()>)> {
    let (mut client, ended) = scripted(script).await?;
    client.send_only(&Members::new()).await?;
    assert_eq!(client.last_stream(), OWN);
    Ok((client, ended))
}

/// Waits until the server's connection has ended, failing past [`CLOSE_WAIT`].
async fn until_closed(ended: oneshot::Receiver<()>, what: &str) -> TestResult {
    compio::time::timeout(CLOSE_WAIT, ended)
        .await
        .map_err(|_| TestError::new(format!("{what}: the client kept its connection")))?
        .map_err(|_| TestError::new(format!("{what}: the server's task ended untold")))
}

/// The Members frame `answer` is: how many members it lists, its stream, and whether it is flagged last.
fn members_of(answer: Answered) -> TestResult<(usize, u64, bool)> {
    match answer {
        Answered::Frame(frame) => {
            let view = frame.view()?;
            let members = MembersView::from_frame(&view)?;
            let header = view.header();
            Ok((
                members.len(),
                header.stream(),
                header.flags() & FLAG_LAST != 0,
            ))
        }
        refused @ (Answered::Refused { .. } | Answered::ConnectionRefused { .. }) => {
            Err(TestError::new(format!("a frame was due: {refused:?}")))
        }
    }
}

/// The rule the client refuses the frame `script` answers its second request with, the connection closed by the time the
/// read returns its error: `what` names the case.
async fn refused_rule(script: VecDeque<Push>, what: &str) -> TestResult<ProtocolError> {
    let (mut client, ended) = asked(script).await?;
    let refused = client
        .next_answer(Command::Members)
        .await
        .err()
        .context(what)?;
    // The client is still held: the close seen is the read's own, and no drop's.
    until_closed(ended, what).await?;
    drop(client);
    let TransportErrorEnum::Protocol { error } = &refused.0 else {
        return Err(TestError::new(format!("{what}: no rule: {refused}")));
    };
    Ok(error.clone())
}

/// §2.5 step 3: a frame on the request's stream, of a command the request is answered by, is the answer.
#[test]
fn test_a_frame_on_the_requests_stream_is_the_answer() -> TestResult {
    bounded(async {
        let (mut client, _ended) = asked(frames([members_on(OWN, 3, FLAG_LAST)?])).await?;
        let answer = client.next_answer(Command::Members).await?;
        assert_eq!(members_of(answer)?, (3, OWN, true));
        Ok(())
    })
}

/// A reply in several frames is read a frame a call, in the order sent, the last alone flagged so.
#[test]
fn test_a_reply_in_several_frames_is_read_a_frame_a_call() -> TestResult {
    bounded(async {
        let script = frames([
            members_on(OWN, 1, 0)?,
            members_on(OWN, 2, 0)?,
            members_on(OWN, 3, FLAG_LAST)?,
        ]);
        let (mut client, _ended) = asked(script).await?;
        for (count, last) in [(1, false), (2, false), (3, true)] {
            let answer = client.next_answer(Command::Members).await?;
            assert_eq!(members_of(answer)?, (count, OWN, last));
        }
        Ok(())
    })
}

/// §2.5 step 3, §11.2: a frame on a stream the client has ended is late whatever its command, a reply's own command and
/// an Error among them: dropped before the reply's first frame and between its frames, so the caller reads the reply's
/// frames alone.
#[test]
fn test_late_frames_before_and_between_a_replys_frames_are_dropped() -> TestResult {
    bounded(async {
        let script = frames([
            held_on(ENDED)?,
            members_on(OWN, 1, 0)?,
            error_on(ENDED, ErrorCode::Refused, "a late refusal")?,
            members_on(ENDED, 7, 0)?,
            members_on(OWN, 2, FLAG_LAST)?,
        ]);
        let (mut client, _ended) = asked(script).await?;
        let first = client.next_answer(Command::Members).await?;
        assert_eq!(members_of(first)?, (1, OWN, false));
        let second = client.next_answer(Command::Members).await?;
        assert_eq!(members_of(second)?, (2, OWN, true));
        Ok(())
    })
}

/// §2.5 step 3, §2.6: a late frame takes steps 4 to 15 before it is dropped, so one whose body fails its checksum is
/// refused by that rule and closes the connection.
#[test]
fn test_a_late_frame_failing_its_checksum_closes_the_connection() -> TestResult {
    bounded(async {
        let corrupted = corrupted_on(ENDED)?;
        let rule = FrameView::validate(&corrupted, CLUSTER, MAX_FRAME_SIZE)
            .err()
            .context("the corrupted frame's rule")?;
        let script = frames([corrupted, members_on(OWN, 1, FLAG_LAST)?]);
        let refused = refused_rule(script, "a late frame failing its checksum").await?;
        assert_eq!(refused, rule);
        Ok(())
    })
}

/// §2.5 step 3, §2.6: the peer's Error on the request's stream is its refusal of the request, with the Error's code and
/// message; the connection stays open, and the next request on it is answered.
#[test]
fn test_the_peers_error_on_the_requests_stream_is_its_refusal() -> TestResult {
    bounded(async {
        let script = frames([error_on(OWN, ErrorCode::Refused, "no such version")?]);
        let (mut client, _ended) = asked(script).await?;
        let answer = client.next_answer(Command::Members).await?;
        assert!(
            matches!(
                &answer,
                Answered::Refused { code: ErrorCode::Refused, message } if message == "no such version"
            ),
            "{answer:?}"
        );
        let next = client.request(&Members::new()).await?;
        assert_eq!(next.view()?.header().stream(), NEXT);
        Ok(())
    })
}

/// §2.5 step 3, §2.6: the peer's Error on stream 0 is about the connection, whatever its code: a connection error's code
/// 1, and a code a stopping peer gives.
#[test]
fn test_the_peers_error_on_stream_0_is_the_connections() -> TestResult {
    bounded(async {
        for (code, message) in [
            (ErrorCode::Protocol, "the frame's header checksum"),
            (ErrorCode::Unavailable, "this process is stopping"),
        ] {
            let (mut client, _ended) = asked(frames([error_on(0, code, message)?])).await?;
            let answer = client.next_answer(Command::Members).await?;
            assert!(
                matches!(
                    &answer,
                    Answered::ConnectionRefused { code: found, message: said }
                        if *found == code && said == message
                ),
                "{code:?}: {answer:?}"
            );
        }
        Ok(())
    })
}

/// §2.6: a peer refusing a frame by a frame-level rule answers Error 1 on stream 0 and closes: the client reads the
/// connection's refusal, and the read after it fails as the connection's end.
#[test]
fn test_a_connection_error_is_read_before_the_connections_end() -> TestResult {
    bounded(async {
        let rule = ProtocolError::chunk(1, 3_840, 4_000);
        let script = VecDeque::from([Push::Close(rule.clone())]);
        let (mut client, _ended) = asked(script).await?;
        let answer = client.next_answer(Command::Members).await?;
        assert!(
            matches!(
                &answer,
                Answered::ConnectionRefused { code: ErrorCode::Protocol, message }
                    if *message == rule.to_string()
            ),
            "{answer:?}"
        );
        drop(answer);
        let ended = client
            .next_answer(Command::Members)
            .await
            .err()
            .context("the connection's end")?;
        assert_eq!(ended.0, TransportErrorEnum::Closed);
        Ok(())
    })
}

/// §2.5 step 3, §2.6: a frame on stream 0 that is no Error answers no request, a reply's own command included: refused
/// as a command not served, and the connection closed.
#[test]
fn test_a_frame_on_stream_0_that_is_no_error_closes_the_connection() -> TestResult {
    bounded(async {
        let script = frames([members_on(0, 0, FLAG_LAST)?]);
        let refused = refused_rule(script, "a Members on stream 0").await?;
        assert_eq!(
            refused,
            ProtocolError::command_not_served(Command::Members.number())
        );
        Ok(())
    })
}

/// §2.5 step 3, §2.6: a stream at or above the next the client will open answers no request: the very next stream, and
/// one far past it, a reply's own command and an Error alike, each refused as a command not served and the connection
/// closed.
#[test]
fn test_a_frame_on_a_stream_not_opened_closes_the_connection() -> TestResult {
    bounded(async {
        for (stray, command) in [
            (members_on(NEXT, 0, FLAG_LAST)?, Command::Members),
            (members_on(AHEAD, 0, FLAG_LAST)?, Command::Members),
            (
                error_on(NEXT, ErrorCode::Refused, "astray")?,
                Command::Error,
            ),
        ] {
            let what = format!("a {command:?} on a stream not opened");
            let refused = refused_rule(frames([stray]), &what).await?;
            assert_eq!(
                refused,
                ProtocolError::command_not_served(command.number()),
                "{what}"
            );
        }
        Ok(())
    })
}

/// §2.5 step 3, §2.6: a frame on the request's stream of a command the request is not answered by is refused as a command
/// not served, and the connection closed.
#[test]
fn test_a_command_the_request_is_not_answered_by_closes_the_connection() -> TestResult {
    bounded(async {
        let refused = refused_rule(frames([held_on(OWN)?]), "a Held answering a Members").await?;
        assert_eq!(
            refused,
            ProtocolError::command_not_served(Command::Held.number())
        );
        Ok(())
    })
}

/// The request named decides what answers it: the Members a Members request takes is refused for a request it does not
/// answer, a Catalog's.
#[test]
fn test_the_request_named_decides_what_answers_it() -> TestResult {
    bounded(async {
        let (mut client, ended) = asked(frames([members_on(OWN, 0, FLAG_LAST)?])).await?;
        let refused = client
            .next_answer(Command::Catalog)
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(
            refused.0,
            TransportErrorEnum::Protocol {
                error: ProtocolError::command_not_served(Command::Members.number())
            }
        );
        until_closed(ended, "a Members answering a Catalog").await
    })
}

/// §2.6, §13: the peer's Error whose body does not read is a frame the client refuses, on the request's stream and on
/// stream 0: its body's rule, and the connection closed.
#[test]
fn test_an_error_whose_body_does_not_read_closes_the_connection() -> TestResult {
    bounded(async {
        for stream in [OWN, 0] {
            let unreadable = unreadable_error_on(stream)?;
            let view = FrameView::validate(&unreadable, CLUSTER, MAX_FRAME_SIZE)?;
            let rule = ErrorReplyView::from_frame(&view)
                .err()
                .context("the empty Error's rule")?;
            let what = format!("an unreadable Error on stream {stream}");
            let refused = refused_rule(frames([unreadable]), &what).await?;
            assert_eq!(refused, rule, "{what}");
        }
        Ok(())
    })
}

/// A Members frame on `stream` of another cluster: its header breaks §2.5's step 5, which the connection's framer checks
/// as it reads the header.
fn foreign_on(stream: u64) -> Result<Vec<u8>, ProtocolError> {
    Members::new().frame(
        Header::new(CLUSTER + 1, Command::Members)
            .with_stream(stream)
            .with_flags(FLAG_LAST),
    )
}

/// §2.5 step 5, §2.6: a frame whose header breaks a rule, found as the header is read, is a frame the client refuses as
/// any other: its rule, and the connection closed.
#[test]
fn test_a_frame_whose_header_breaks_a_rule_closes_the_connection() -> TestResult {
    bounded(async {
        let refused = refused_rule(frames([foreign_on(OWN)?]), "another cluster's frame").await?;
        assert_eq!(refused, ProtocolError::wrong_cluster(CLUSTER, CLUSTER + 1));
        Ok(())
    })
}

/// [`RoleClient::request`] reads by the same rule: a reply whose header breaks a rule is refused by it, and the connection
/// closed while the client is still held (§2.6).
#[test]
fn test_a_request_answered_by_a_header_breaking_a_rule_closes_the_connection() -> TestResult {
    bounded(async {
        let (mut client, ended) = scripted(frames([foreign_on(OWN)?])).await?;
        let refused = client
            .request(&Members::new())
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(
            refused.0,
            TransportErrorEnum::Protocol {
                error: ProtocolError::wrong_cluster(CLUSTER, CLUSTER + 1)
            }
        );
        until_closed(ended, "a request answered by another cluster's frame").await
    })
}

/// [`RoleClient::request`] takes the peer's Error as its failure wherever it came: on the request's stream and on stream
/// 0, each with the Error's code and message.
#[test]
fn test_a_request_fails_by_the_peers_error_on_either_stream() -> TestResult {
    bounded(async {
        for (stream, code, message) in [
            (OWN, ErrorCode::Refused, "no such version"),
            (0, ErrorCode::Unavailable, "this process is stopping"),
        ] {
            let (mut client, _ended) = scripted(frames([error_on(stream, code, message)?])).await?;
            let refused = client
                .request(&Members::new())
                .await
                .err()
                .context("a refusal")?;
            assert_eq!(
                refused.0,
                TransportErrorEnum::Refused {
                    code,
                    message: message.to_string()
                },
                "stream {stream}"
            );
        }
        Ok(())
    })
}

/// [`Answered::frame`] gives a frame read as the answer back whole.
#[test]
fn test_an_answers_frame_is_given_back_whole() -> TestResult {
    bounded(async {
        let (mut client, _ended) = asked(frames([members_on(OWN, 2, FLAG_LAST)?])).await?;
        let frame = client.next_answer(Command::Members).await?.frame()?;
        let view = frame.view()?;
        assert_eq!(MembersView::from_frame(&view)?.len(), 2);
        assert_eq!(view.header().stream(), OWN);
        Ok(())
    })
}
