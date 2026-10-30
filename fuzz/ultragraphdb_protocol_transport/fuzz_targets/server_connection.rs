/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as what a client sends on one connection to a server: the first byte chooses a server authenticating by
//! SCRAM-SHA-256 (RFC 7677's user and server nonce, so the golden exchange completes) or one without authentication;
//! the rest is cut into frames by their headers' `size`, each numbered as the client's next with any damage to its
//! header checksum kept, read through a server's framer and taken by the `ServerConnection`. The session the target
//! plays refuses every request standing alone and answers every read with one last Result frame.
//!
//! Nothing panics. A request is handed over only on an open stream; the principal is known exactly once the
//! connection serves; every frame the connection sends validates, is numbered one above the one before, and once it
//! closes nothing follows its closing Error; a connection past its deadlines closes.

#![no_main]

use std::time::{Duration, Instant};

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_scram::{Credentials, MIN_ITERATIONS, ScramAuthenticator, ScramVerifier};
use ultragraphdb_protocol_transport::{
    Authenticator, BufferPool, FrameStep, Framer, Inbound, ReceivedFrame, ServerConnection,
};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, ErrorCode, ErrorReply, FLAG_LAST, FrameView, Header, HeaderView,
    MAX_FRAME_SIZE, ResultBatch, Welcome, xxh3_64,
};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/server_connection/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;
/// RFC 7677's server nonce, so the golden client-final message carries the nonce this server expects.
const SERVER_NONCE: &str = "%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";
/// RFC 7677's salt and keys for the user `user` and the password `pencil` at 4,096 iterations.
const SALT: [u8; 16] = [
    0x5b, 0x6d, 0x99, 0x68, 0x9d, 0x12, 0x35, 0x8e, 0xec, 0xa0, 0x4b, 0x14, 0x12, 0x36, 0xfa, 0x81,
];
const STORED_KEY: [u8; 32] = [
    0x58, 0x6e, 0x5d, 0xf2, 0x83, 0xe6, 0xdc, 0xeb, 0x5c, 0x3e, 0x79, 0x1d, 0x8b, 0x85, 0x28, 0xec,
    0x19, 0x1e, 0x66, 0x40, 0x45, 0xce, 0x97, 0x17, 0x92, 0xe2, 0xe6, 0xb5, 0xbb, 0x13, 0xe2, 0xa6,
];
const SERVER_KEY: [u8; 32] = [
    0xc1, 0xf3, 0xcb, 0xc1, 0xc1, 0x3a, 0x9d, 0x35, 0xa1, 0x4c, 0x09, 0x90, 0xee, 0xd9, 0x76, 0x29,
    0xea, 0x22, 0x58, 0x63, 0xe5, 0x66, 0xa4, 0x31, 0x4a, 0xb9, 0x9f, 0x3f, 0x00, 0xe5, 0xd9, 0xd5,
];

/// The server's one principal, RFC 7677's user.
#[derive(Debug, Clone, Copy)]
struct Rfc7677;

impl Credentials for Rfc7677 {
    fn lookup(&self, name: &str) -> Option<(u64, ScramVerifier)> {
        (name == "user").then(|| {
            (
                7,
                ScramVerifier::new(SALT.to_vec(), 4_096, STORED_KEY, SERVER_KEY),
            )
        })
    }
}

/// `frame`, a frame of the client's, numbered `sequence`; a header checksum that did not hold before does not hold
/// after. `None` for fewer bytes than a header.
fn numbered(frame: &[u8], sequence: u128) -> Option<Vec<u8>> {
    let mut frame = frame.to_vec();
    let damage = u64::from_le_bytes(frame.get(..8)?.try_into().ok()?) ^ xxh3_64(frame.get(8..256)?);
    Header::stamp_sequence(&mut frame, sequence).ok()?;
    let resealed = u64::from_le_bytes(frame.get(..8)?.try_into().ok()?);
    frame
        .get_mut(..8)?
        .copy_from_slice(&(resealed ^ damage).to_le_bytes());
    Some(frame)
}

/// `bytes`, one frame, read through `framer`; the framer's refusal as `Err`.
fn read(framer: &mut Framer, pool: &BufferPool, bytes: &[u8]) -> Result<ReceivedFrame, ()> {
    let mut buffer = pool.acquire().map_err(drop)?;
    framer.reset();
    loop {
        let mut data = buffer.take();
        data.clear();
        data.extend_from_slice(bytes.get(..framer.want()).ok_or(())?);
        buffer.restore(data);
        match framer.advance(&buffer).map_err(drop)? {
            FrameStep::Read(_) => {}
            FrameStep::Frame => return Ok(framer.finish(buffer)),
        }
    }
}

/// The frames the connection sends now: each validates and is numbered one above `sent`, which advances; none
/// follows a closing Error on stream 0.
fn drain<A: Authenticator>(
    connection: &mut ServerConnection<A>,
    sent: &mut u128,
    closed: &mut bool,
) {
    while let Some((stream, frame)) = connection.next_frame() {
        assert!(!*closed, "a frame after the connection's closing Error");
        let view =
            FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE).expect("a sent frame validates");
        assert_eq!(
            view.header().sequence(),
            *sent + 1,
            "frames are numbered in order"
        );
        assert_eq!(view.header().stream(), stream);
        *sent += 1;
        let header = HeaderView::new(&frame).expect("a header");
        if stream == 0 && header.command() == Command::Error.number() && connection.is_closing() {
            *closed = true;
        }
    }
}

/// The connection driven through `frames`, at `now`, then past its deadlines.
fn run<A: Authenticator>(
    mut connection: ServerConnection<A>,
    authenticates: bool,
    frames: &[u8],
    now: Instant,
) {
    let pool = BufferPool::new(2, MAX_FRAME_SIZE);
    let mut framer = Framer::from_client(CLUSTER, MAX_FRAME_SIZE);
    let (mut sent, mut closed, mut sequence) = (0u128, false, 1u128);
    let mut rest = frames;
    while !rest.is_empty() && !connection.is_closing() {
        // The frame's extent by its header's size, or the rest when it names none that fits.
        let size = rest
            .get(40..44)
            .and_then(|s| s.try_into().ok())
            .map(u32::from_le_bytes)
            .map_or(rest.len(), |s| (s as usize).clamp(256, rest.len().max(256)));
        let (frame, after) = rest.split_at(size.min(rest.len()));
        rest = after;
        let Some(frame) = numbered(frame, sequence) else {
            break;
        };
        sequence += 1;
        let inbound = match read(&mut framer, &pool, &frame) {
            Ok(received) => connection.receive(&received, now),
            Err(()) => {
                let refusal = ultragraphdb_error_protocol::ProtocolError::ShortHeader(frame.len());
                connection.refuse_connection(&refusal)
            }
        };
        match inbound {
            Inbound::Welcomed { protocol } => {
                framer.set_protocol(protocol);
                assert_eq!(connection.principal().is_some(), !authenticates);
            }
            Inbound::Authenticated { principal } => {
                assert_eq!(principal, 7, "only RFC 7677's user authenticates");
                assert_eq!(connection.principal(), Some(7));
            }
            Inbound::Request { stream, command } => {
                assert!(
                    connection.principal().is_some(),
                    "a request before authentication"
                );
                assert!(
                    connection.outbound().is_open(stream) || command == Command::Abort,
                    "a request on a stream not open"
                );
                match command {
                    Command::Read | Command::Execute => {
                        if let Ok(frame) = ResultBatch::new(1).frame(
                            Header::new(CLUSTER, Command::Result)
                                .with_stream(stream)
                                .with_flags(FLAG_LAST),
                        ) {
                            let _ = connection.outbound().push(stream, frame);
                        }
                    }
                    Command::Register
                    | Command::Prepare
                    | Command::Describe
                    | Command::Drop
                    | Command::Commit => {
                        connection.refuse(stream, &ErrorReply::new(ErrorCode::Refused, "fuzz"));
                    }
                    _ => {}
                }
            }
            Inbound::Closing { .. } => assert!(connection.is_closing()),
            Inbound::Handled | Inbound::Cancelled { .. } | Inbound::Refused { .. } => {}
        }
        drain(&mut connection, &mut sent, &mut closed);
    }
    if !connection.is_closing() {
        assert!(
            connection.next_deadline().is_some(),
            "an open connection has a deadline"
        );
        connection.handle_timeout(now + Duration::from_secs(86_400));
        assert!(
            connection.is_closing(),
            "a connection past its deadlines closes"
        );
    }
    drain(&mut connection, &mut sent, &mut closed);
    assert!(connection.next_frame().is_none());
}

fuzz_target!(|data: &[u8]| {
    let Some((mode, frames)) = data.split_first() else {
        return;
    };
    let now = Instant::now();
    let welcome = Welcome::new(1, 0, "fuzz")
        .with_max_streams(8)
        .with_initial_credit(1);
    if mode % 2 == 1 {
        let authenticator = ScramAuthenticator::new(Rfc7677, || SERVER_NONCE.to_string(), [7; 32])
            .with_iterations(MIN_ITERATIONS);
        let server = welcome.with_mechanisms(["SCRAM-SHA-256"]);
        run(
            ServerConnection::new(CLUSTER, server, authenticator, now),
            true,
            frames,
            now,
        );
    } else {
        let authenticator = ScramAuthenticator::new(Rfc7677, || SERVER_NONCE.to_string(), [7; 32]);
        run(
            ServerConnection::new(CLUSTER, welcome, authenticator, now),
            false,
            frames,
            now,
        );
    }
});
