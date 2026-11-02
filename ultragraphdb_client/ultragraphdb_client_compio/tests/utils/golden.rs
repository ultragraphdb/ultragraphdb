/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A server speaking the golden frames of `ultragraphdb_protocol_wire` on compio, through the transport's compio
//! connection: the client's Hello checked against the
//! golden Hello and answered with a golden Welcome, then each frame the client sends checked against the golden frame
//! its step names and answered with its step's frames, each moved to the request's stream and numbered as the
//! connection's next, a step's frames in one write; a step may send its reply twice or not at all, as a network repeats
//! or loses it, damage it, cut it and close. The headers' checksums are recomputed with xxhash-rust directly.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::future::poll_fn;
use std::rc::Rc;
use std::task::Poll;

use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_protocol_transport::{BufferPool, Framer, SimStream};
use ultragraphdb_protocol_transport_compio::{CompioSimStream, Connection};
use ultragraphdb_protocol_wire::MAX_FRAME_SIZE;

use crate::utils::serve::CLUSTER;
use ultragraphdb_tool_bdd::{Context as _, TestResult};

/// Golden frame `name`, read from the wire crate's `testdata/golden/bodies/`.
pub fn golden(name: &str) -> TestResult<Vec<u8>> {
    // The repository's root, three levels above this crate's `testdata/`, taken lexically: the directory need not exist.
    let testdata = ultragraphdb_tool_bdd::testdata_dir()?;
    let root = testdata
        .ancestors()
        .nth(3)
        .context("the repository's root")?;
    let path = root
        .join("ultragraphdb_protocol/ultragraphdb_protocol_wire/testdata/golden/bodies")
        .join(format!("{name}.frame"));
    fs::read(&path)
        .map_err(|e| format!("{}: {e}", path.display()))
        .context("reading the golden frame")
}

/// The header's checksum over bytes 8 to 256, recomputed with xxhash-rust directly (`PROTOCOL.md` §2.2).
fn checksummed(mut frame: Vec<u8>) -> Vec<u8> {
    let checksum = xxhash_rust::xxh3::xxh3_64(&frame[8..256]);
    frame[..8].copy_from_slice(&checksum.to_le_bytes());
    frame
}

/// `frame` moved to `stream`: the header's `stream`, bytes 56 to 64.
pub fn restream(frame: &[u8], stream: u64) -> Vec<u8> {
    let mut frame = frame.to_vec();
    frame[56..64].copy_from_slice(&stream.to_le_bytes());
    checksummed(frame)
}

/// `frame` numbered `sequence`: the header's `sequence`, bytes 16 to 32.
pub fn numbered(frame: &[u8], sequence: u128) -> Vec<u8> {
    let mut frame = frame.to_vec();
    frame[16..32].copy_from_slice(&sequence.to_le_bytes());
    checksummed(frame)
}

/// `frame` without its last flag: bit 0 of the header's `flags`, byte 65.
pub fn not_last(frame: &[u8]) -> Vec<u8> {
    let mut frame = frame.to_vec();
    frame[65] &= !1;
    checksummed(frame)
}

pub fn stream_of(frame: &[u8]) -> TestResult<u64> {
    Ok(u64::from_le_bytes(
        frame[56..64].try_into().context("a header")?,
    ))
}

pub fn sequence_of(frame: &[u8]) -> TestResult<u128> {
    Ok(u128::from_le_bytes(
        frame[16..32].try_into().context("a header")?,
    ))
}

/// The golden frame `name` as the client's `frame` should be: moved to its stream and numbered with its sequence.
fn expected(name: &str, frame: &[u8]) -> TestResult<Vec<u8>> {
    Ok(numbered(
        &restream(&golden(name)?, stream_of(frame)?),
        sequence_of(frame)?,
    ))
}

/// What the server does with the client's next frame.
#[derive(Debug, Clone)]
pub struct Step {
    expect: Option<&'static str>,
    replies: Vec<Vec<u8>>,
    copies: usize,
    damage: Option<usize>,
    cut: Option<usize>,
    close: bool,
}

impl Step {
    /// A frame of the client's, checked against golden frame `name`.
    pub fn expect(name: &'static str) -> Self {
        Self {
            expect: Some(name),
            ..Self::any()
        }
    }

    /// A frame of the client's, of no golden frame.
    pub fn any() -> Self {
        Self {
            expect: None,
            replies: Vec::new(),
            copies: 1,
            damage: None,
            cut: None,
            close: false,
        }
    }

    /// Answered with `frame`, after the frames before it.
    pub fn reply(mut self, frame: Vec<u8>) -> Self {
        self.replies.push(frame);
        self
    }

    /// Each reply sent `copies` times, the same bytes: 0 as a network loses it, 2 as it repeats it.
    pub fn copies(mut self, copies: usize) -> Self {
        self.copies = copies;
        self
    }

    /// The last reply, numbered, with bit `at % 8` of its byte `at` flipped; the connection closed after it.
    pub fn damaged(mut self, at: usize) -> Self {
        self.damage = Some(at);
        self.close = true;
        self
    }

    /// The connection closed after the replies.
    pub fn close(mut self) -> Self {
        self.close = true;
        self
    }

    /// The last reply, numbered, cut to its first `len` bytes; the connection closed after it.
    pub fn cut(mut self, len: usize) -> Self {
        self.cut = Some(len);
        self.close = true;
        self
    }
}

/// The server's script and what it saw: the frame answering each Hello, the steps to come, the golden frames the
/// client's frames matched and those they did not.
#[derive(Debug)]
pub struct Golden {
    welcome: Vec<u8>,
    steps: VecDeque<Step>,
    matched: Vec<&'static str>,
    mismatches: Vec<String>,
    connections: u32,
}

impl Golden {
    /// A server answering each Hello with golden frame `welcome`.
    pub fn new(welcome: &str) -> TestResult<Rc<RefCell<Self>>> {
        Ok(Rc::new(RefCell::new(Self {
            welcome: golden(welcome)?,
            steps: VecDeque::new(),
            matched: Vec::new(),
            mismatches: Vec::new(),
            connections: 0,
        })))
    }

    pub fn push(&mut self, step: Step) {
        self.steps.push_back(step);
    }

    pub fn matched(&self) -> &[&'static str] {
        &self.matched
    }

    pub fn mismatches(&self) -> &[String] {
        &self.mismatches
    }

    pub fn connections(&self) -> u32 {
        self.connections
    }

    pub fn steps_left(&self) -> usize {
        self.steps.len()
    }

    /// `frame` of the client's against golden frame `name`, but for its stream and sequence; a golden frame that
    /// cannot be read is a mismatch naming why.
    fn check(&mut self, name: &'static str, frame: &[u8]) {
        match expected(name, frame) {
            Ok(golden_frame) if frame == golden_frame => self.matched.push(name),
            Ok(_) => self.mismatches.push(name.to_string()),
            Err(error) => self.mismatches.push(format!("{name}: {error}")),
        }
    }
}

/// One connection served from `golden`'s script, until the client closes it or a step closes it.
pub async fn serve_golden<S: AsyncRead + AsyncWrite>(golden: &RefCell<Golden>, stream: S) {
    golden.borrow_mut().connections += 1;
    let pool = BufferPool::new(4, MAX_FRAME_SIZE);
    let mut connection = Connection::new(stream, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
    let Ok(hello) = connection.read_frame(&pool).await else {
        return;
    };
    golden.borrow_mut().check("hello", hello.bytes());
    drop(hello);
    let welcome = golden.borrow().welcome.clone();
    if connection.write_frame(welcome).await.is_err() {
        return;
    }
    let mut sent = 1;
    while let Ok(received) = connection.read_frame(&pool).await {
        let frame = received.bytes().to_vec();
        drop(received);
        let Some(step) = golden.borrow_mut().steps.pop_front() else {
            golden
                .borrow_mut()
                .mismatches
                .push("a frame past the script".to_string());
            return;
        };
        if let Some(name) = step.expect {
            golden.borrow_mut().check(name, &frame);
        }
        // The step's frames in one write, so a read may meet the next frame's bytes behind a frame's, as over TCP.
        let Ok(request_stream) = stream_of(&frame) else {
            golden
                .borrow_mut()
                .mismatches
                .push("a frame without a header".to_string());
            return;
        };
        let mut out = Vec::new();
        let replies = step.replies.len();
        for (i, reply) in step.replies.iter().enumerate() {
            sent += 1;
            let mut reply = numbered(&restream(reply, request_stream), sent);

            if let Some(at) = step.damage.filter(|_| i + 1 == replies) {
                reply[at] ^= 1 << (at % 8);
            }
            if let Some(len) = step.cut.filter(|_| i + 1 == replies) {
                reply.truncate(len);
            }
            for _ in 0..step.copies {
                out.extend_from_slice(&reply);
            }
        }
        if !out.is_empty() && connection.write_frame(out).await.is_err() {
            return;
        }
        if step.close {
            connection.shutdown().await;
            return;
        }
    }
}

/// Once polled pending, then ready: the other tasks run in between.
async fn yield_now() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            return Poll::Ready(());
        }
        yielded = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    })
    .await;
}

/// Serves every connection the connector makes from `golden`'s script, one after another, for as long as the task
/// lives.
pub async fn serve_golden_sim(
    golden: Rc<RefCell<Golden>>,
    accepted: Rc<RefCell<VecDeque<SimStream>>>,
) {
    loop {
        let next = accepted.borrow_mut().pop_front();
        match next {
            Some(stream) => serve_golden(&golden, CompioSimStream::new(stream)).await,
            None => yield_now().await,
        }
    }
}
