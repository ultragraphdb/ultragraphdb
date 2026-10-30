/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as what the server sent next: framed by the connection's framer, numbered as the connection's next frame
//! with any damage to its header checksum kept, then received by a client that waits on a Register, a Prepare, a
//! Describe and Begins on streams 1 to 20, a Commit and a Drop through sessions, and reads on streams of their own
//! from 21. Everything the client decodes from a server goes through here: replies, errors, result batches, describe,
//! stream 0's frames. It never panics; a frame breaking a rule leaves the client disconnected.

#![no_main]

use std::time::Instant;

use libfuzzer_sys::fuzz_target;
use ultragraphdb_client_core::{BatchPoll, ClientConfig, ClientCore, Reply, ResultStream};
use ultragraphdb_protocol_transport::{Anonymous, BufferPool, FrameStep, Framer, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    Begin, BodyBuilder, Command, DropCommand, Header, MAX_FRAME_SIZE, PatternSlot, Prepare,
    QuadPattern, Read, TermTable, Welcome, xxh3_64,
};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/reply/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// The server's side of one connection as the fuzz target plays it: the client's framer and the sequence of the
/// server's next frame.
struct Server {
    framer: Framer,
    sequence: u128,
}

impl Server {
    /// `bytes`, a frame of the server's, numbered as the connection's next and read through the framer; `None` when
    /// the framer refuses it. A header checksum that did not hold before the numbering does not hold after it.
    fn frame(&mut self, pool: &BufferPool, bytes: &[u8]) -> Option<ReceivedFrame> {
        let mut bytes = bytes.to_vec();
        let damage = match (bytes.get(..8), bytes.get(8..256)) {
            (Some(stored), Some(tail)) => {
                u64::from_le_bytes(stored.try_into().ok()?) ^ xxh3_64(tail)
            }
            _ => 0,
        };
        Header::stamp_sequence(&mut bytes, self.sequence).ok()?;
        let resealed = u64::from_le_bytes(bytes.get(..8)?.try_into().ok()?);
        bytes
            .get_mut(..8)?
            .copy_from_slice(&(resealed ^ damage).to_le_bytes());
        self.sequence += 1;
        let mut buffer = pool.acquire().ok()?;
        self.framer.reset();
        loop {
            let mut data = buffer.take();
            data.clear();
            data.extend_from_slice(bytes.get(..self.framer.want())?);
            buffer.restore(data);
            match self.framer.advance(&buffer).ok()? {
                FrameStep::Read(_) => {}
                FrameStep::Frame => return Some(self.framer.finish(buffer)),
            }
        }
    }
}

/// A ready client with requests waiting on streams 1 to 20, a commit and a drop in flight, and its result streams.
fn client(
    pool: &BufferPool,
    server: &mut Server,
    now: Instant,
) -> Option<(ClientCore<Anonymous>, Vec<ResultStream>)> {
    let mut core = ClientCore::new(ClientConfig::new(CLUSTER, "fuzz"), Anonymous);
    core.connect(now, 0).ok()?;
    while core.transmit().is_some() {}
    let welcome = Welcome::new(1, 0, "s")
        .with_max_streams(64)
        .with_initial_credit(2)
        .frame(Header::new(CLUSTER, Command::Welcome))
        .ok()?;
    core.receive(server.frame(pool, &welcome)?, now).ok()?;
    let session = core.resume_session(9, 7);
    let dropping = core.resume_session(10, 3);
    let mut begun = Vec::new();
    for i in 0..20u64 {
        let pending = match i % 5 {
            0 => core.register(now).ok()?,
            1 => core
                .prepare(
                    &Prepare::new(
                        0,
                        0,
                        0,
                        TermTable::new(),
                        QuadPattern::new(
                            PatternSlot::DefaultGraph,
                            PatternSlot::Unbound,
                            PatternSlot::Unbound,
                            PatternSlot::Unbound,
                        ),
                        Vec::new(),
                        Vec::new(),
                    ),
                    now,
                )
                .ok()?,
            2 => core.describe(0, now).ok()?,
            _ => core.begin(0, i % 5 == 4, now).ok()?,
        };
        begun.push((i + 1, pending));
    }
    // Every Begin answered, so a transaction holds its stream: a commit on one, reads on the read-only ones.
    let mut streams = Vec::new();
    for (stream, pending) in begun.iter().filter(|(s, _)| (s - 1) % 5 >= 3) {
        let begin = Begin::new(0, 5, (stream - 1) % 5 == 4)
            .frame(Header::new(CLUSTER, Command::Begin).with_stream(*stream))
            .ok()?;
        core.receive(server.frame(pool, &begin)?, now).ok()?;
        if let Some(Ok(Reply::Begun(tx))) = core.take_reply(*pending) {
            if tx.read_only() {
                let read = Read::new(
                    tx.stream(),
                    0,
                    TermTable::new(),
                    QuadPattern::new(
                        PatternSlot::Unbound,
                        PatternSlot::Unbound,
                        PatternSlot::Unbound,
                        PatternSlot::Unbound,
                    ),
                    Vec::new(),
                );
                streams.push(core.read(&tx, &read, now).ok()?);
            } else if tx.stream() == 4 {
                core.commit(tx, session, now).ok()?;
            }
        }
    }
    core.drop_command(DropCommand::all(), dropping, now).ok()?;
    while core.transmit().is_some() {}
    Some((core, streams))
}

fuzz_target!(|data: &[u8]| {
    let now = Instant::now();
    let pool = BufferPool::new(8, MAX_FRAME_SIZE);
    let mut server = Server {
        framer: Framer::new(CLUSTER, MAX_FRAME_SIZE),
        sequence: 1,
    };
    let Some((mut core, streams)) = client(&pool, &mut server, now) else {
        return;
    };
    let Some(received) = server.frame(&pool, data) else {
        return;
    };
    let accepted = core.receive(received, now).is_ok();
    if !accepted {
        assert!(!core.is_ready(), "a refused frame gives the connection up");
    }
    for stream in &streams {
        if let Ok(BatchPoll::Batch(rows)) = core.next_batch(stream, now, |view| {
            (0..view.rows())
                .map(|row| {
                    view.cell(0, row)
                        .and_then(|cell| view.term(cell))
                        .map(|t| t.encoding().len())
                })
                .count()
        }) {
            assert!(rows <= 1 << 20);
        }
    }
});
