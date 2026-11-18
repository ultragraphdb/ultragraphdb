/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The reader rule's law (`PROTOCOL.md` §2.5, step 3; §2.6): over any drawn interleaving of late frames among a reply's
//! frames, [`RoleClient::next_answer`] gives the caller exactly the reply's frames, in the order sent, up to the first
//! frame the rule refuses, and then that refusal.

use std::collections::VecDeque;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_role::{
    Answer, Answered, Push, ROLE_ITERATIONS, RoleClient, RoleSecret, Server, Session, listen,
    serve_connection,
};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame, TransportErrorEnum};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorCode, ErrorReply, FLAG_LAST, Header, Held, Members,
    MembersView,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

const CLUSTER: u64 = 7;
const PASSWORD: &str = "s3cret";

/// How long one drawn case may take: far past what any takes, so a peer that stops answering fails the case.
const CASE_TIME: Duration = Duration::from_secs(10);

/// A frame sent on a stream the client has ended, of any command (§2.5, step 3): what a drawn late frame is.
#[derive(Debug, Clone, Copy)]
enum Late {
    Held,
    Members,
    Error,
}

/// A frame the rule refuses, each as a command not served: what a drawn case may end its frames with.
#[derive(Debug, Clone, Copy)]
enum Stray {
    /// A Members on stream 0.
    Zero,
    /// A Members on the stream this many past the request's.
    Ahead(u64),
    /// A Held on the request's stream, which answers no Members.
    Held,
}

/// A drawn case: the streams the client ended before its request, the late frames sent before each frame of the reply,
/// and the frame the rule refuses that takes the place of the reply's frame at an index, if any.
#[derive(Debug)]
struct Case {
    ended: u64,
    late: Vec<Vec<(Late, u64)>>,
    stray: Option<(usize, Stray)>,
}

impl Case {
    /// The stream of the request the case reads the answer of: the one after those ended.
    fn own(&self) -> u64 {
        self.ended + 1
    }
}

/// A number from `min` to `max`.
fn within(tc: &TestCase, min: u64, max: u64) -> u64 {
    tc.draw(gs::integers::<u64>().min_value(min).max_value(max))
}

/// A case of 1 to 3 ended streams and a reply of 1 to 5 frames, each behind 0 to 3 late frames on a drawn ended stream;
/// in half the cases a frame the rule refuses stands in for one of the reply's frames.
fn case(tc: &TestCase) -> Case {
    let ended = within(tc, 1, 3);
    let late: Vec<Vec<(Late, u64)>> = (0..within(tc, 1, 5))
        .map(|_| {
            (0..within(tc, 0, 3))
                .map(|_| {
                    let kind = tc.draw(gs::sampled_from(vec![
                        Late::Held,
                        Late::Members,
                        Late::Error,
                    ]));
                    (kind, within(tc, 1, ended))
                })
                .collect()
        })
        .collect();
    let stray = tc.draw(gs::booleans()).then(|| {
        let at = tc.draw(
            gs::integers::<usize>()
                .min_value(0)
                .max_value(late.len() - 1),
        );
        let ahead = within(tc, 1, 4);
        let stray = tc.draw(gs::sampled_from(vec![
            Stray::Zero,
            Stray::Ahead(ahead),
            Stray::Held,
        ]));
        (at, stray)
    });
    Case { ended, late, stray }
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

/// A Held frame on `stream`, not flagged last.
fn held_on(stream: u64) -> Result<Vec<u8>, ProtocolError> {
    Held.frame(Header::new(CLUSTER, Command::Held).with_stream(stream))
}

/// What the server sends for `case`: before each frame of the reply its late frames, then the frame, which lists as many
/// members as its place in the reply, from 1, the last flagged so; the stray frame in its place ends the script.
fn script(case: &Case) -> Result<VecDeque<Push>, ProtocolError> {
    let own = case.own();
    let mut frames = VecDeque::new();
    for (index, late) in case.late.iter().enumerate() {
        for (kind, stream) in late {
            frames.push_back(Push::Frame(match kind {
                Late::Held => held_on(*stream)?,
                Late::Members => members_on(*stream, 9, 0)?,
                Late::Error => ErrorReply::new(ErrorCode::Refused, "late")
                    .frame(Header::new(CLUSTER, Command::Error).with_stream(*stream))?,
            }));
        }
        if let Some((_, stray)) = case.stray.filter(|(at, _)| *at == index) {
            frames.push_back(Push::Frame(match stray {
                Stray::Zero => members_on(0, 9, 0)?,
                Stray::Ahead(by) => members_on(own + by, 9, 0)?,
                Stray::Held => held_on(own)?,
            }));
            return Ok(frames);
        }
        let flags = if index + 1 == case.late.len() {
            FLAG_LAST
        } else {
            0
        };
        frames.push_back(Push::Frame(members_on(own, index + 1, flags)?));
    }
    Ok(frames)
}

/// What a case's reads are due by the rule, from the case alone: the members each frame read lists, counting from 1, up
/// to the stray frame's place or the reply's end, and the stray frame's refusal.
fn due(case: &Case) -> (Vec<usize>, Option<ProtocolError>) {
    let Some((at, stray)) = case.stray else {
        return ((1..=case.late.len()).collect(), None);
    };
    let command = match stray {
        Stray::Zero | Stray::Ahead(_) => Command::Members,
        Stray::Held => Command::Held,
    };
    (
        (1..=at).collect(),
        Some(ProtocolError::command_not_served(command.number())),
    )
}

/// Ends each of the client's first `ended` requests with an empty Members, and answers the one after them with `script`,
/// each push in turn on that request's stream, whatever stream a pushed frame's own header names.
#[derive(Debug)]
struct Scripted {
    ended: u64,
    script: VecDeque<Push>,
    open: Option<u64>,
}

impl Session for Scripted {
    async fn answer(&mut self, stream: u64, _frame: ReceivedFrame) -> Answer {
        if stream > self.ended {
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

/// What a client reads of `case` through [`RoleClient::next_answer`]: the members each frame it was given lists, in the
/// order given, until a frame flagged last, and the rule of the refusal that ended its reads, if one did.
async fn read(case: &Case) -> TestResult<(Vec<usize>, Option<ProtocolError>)> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
    let address = listener.local_address()?;
    let server = Server::new(CLUSTER, "test", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
    let file = std::env::temp_dir().join(format!("answer-law-secret-{}", std::process::id()));
    std::fs::write(&file, PASSWORD)?;
    let held = RoleSecret::read(&file.display().to_string())?.credentials()?;
    let session = Scripted {
        ended: case.ended,
        script: script(case)?,
        open: None,
    };
    spawn(async move {
        if let Ok(carrier) = listener.accept().await {
            serve_connection(carrier, &server, held, session).await;
        }
    })
    .detach();
    let mut client = RoleClient::dial(&address, CLUSTER, "storage", PASSWORD).await?;
    for _ in 0..case.ended {
        drop(client.request(&Members::new()).await?);
    }
    client.send_only(&Members::new()).await?;
    let mut given = Vec::new();
    // One read more than the reply has frames, so a frame given past the reply's would be seen.
    for _ in 0..=case.late.len() {
        match client.next_answer(Command::Members).await {
            Ok(Answered::Frame(frame)) => {
                let view = frame.view()?;
                let header = view.header();
                if header.stream() != case.own() {
                    return Err(TestError::new(format!(
                        "a frame of stream {} was given",
                        header.stream()
                    )));
                }
                given.push(MembersView::from_frame(&view)?.len());
                if header.flags() & FLAG_LAST != 0 {
                    return Ok((given, None));
                }
            }
            Ok(refused @ (Answered::Refused { .. } | Answered::ConnectionRefused { .. })) => {
                return Err(TestError::new(format!("a refusal was given: {refused:?}")));
            }
            Err(error) => {
                let TransportErrorEnum::Protocol { error: rule } = error.0 else {
                    return Err(TestError::new(format!("the read failed: {error}")));
                };
                return Ok((given, Some(rule)));
            }
        }
    }
    Err(TestError::new("more frames were given than the reply has"))
}

/// The reads of `case` on a runtime of its own, failing past [`CASE_TIME`].
fn run(case: &Case) -> TestResult<(Vec<usize>, Option<ProtocolError>)> {
    Runtime::new()?.block_on(async {
        compio::time::timeout(CASE_TIME, read(case))
            .await
            .map_err(|_| TestError::new(format!("the case within {CASE_TIME:?}")))?
    })
}

/// The reader rule's law: whatever late frames stand before and between a reply's frames, on whichever ended streams and
/// of whichever commands, the caller is given the reply's frames alone, each once and in the order sent; a frame the rule
/// refuses ends the reads with its refusal, the frames before it given and none after.
#[hegel::test(test_cases = 30)]
fn test_a_reader_is_given_the_replys_frames_alone_and_in_order(tc: TestCase) {
    let drawn = case(&tc);
    let given = run(&drawn).map_err(|error| error.to_string());
    assert_eq!(given, Ok(due(&drawn)), "{drawn:?}");
}
