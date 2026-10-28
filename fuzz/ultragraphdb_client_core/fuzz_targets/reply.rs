/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as what the server sent: framed as a shell frames them, then received by a client that waits on stream
//! 0's Error, a Register, a Begin, a Prepare, a Describe and a Commit, and reads a result stream, on streams 1 to 20,
//! which cover the golden frames' streams. Everything the client decodes from a server goes through here: replies,
//! errors, result batches, describe. It never panics; a frame breaking a rule leaves the client disconnected.

#![no_main]

use std::time::Instant;

use libfuzzer_sys::fuzz_target;
use ultragraphdb_client_core::{BatchPoll, ClientConfig, ClientCore, Reply, ResultStream};
use ultragraphdb_protocol_transport::{BufferPool, FrameStep, Framer};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Header, PatternSlot, Prepare, QuadPattern, Read, TermTable, Welcome,
};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/reply/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// A ready client with requests waiting on streams 1 to 20, and the result streams among them.
fn client(pool: &BufferPool, now: Instant) -> Option<(ClientCore, Vec<ResultStream>)> {
    let mut core = ClientCore::new(ClientConfig::new(CLUSTER, "fuzz"));
    core.connect(now).ok()?;
    let welcome = Welcome::new(1, 0, 1 << 20, 64, 2, "s")
        .frame(Header::new(CLUSTER, Command::Welcome))
        .ok()?;
    core.receive(frame(pool, &welcome)?, now).ok()?;
    let session = core.resume_session(9, 7);
    let mut streams = Vec::new();
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
    // Every Begin answered, so a transaction holds its stream: commits on some, reads on the others.
    for (stream, pending) in begun.iter().filter(|(s, _)| (s - 1) % 5 >= 3) {
        let begin = ultragraphdb_protocol_wire::Begin::new(0, 5, (stream - 1) % 5 == 4)
            .frame(Header::new(CLUSTER, Command::Begin).with_stream(*stream))
            .ok()?;
        core.receive(frame(pool, &begin)?, now).ok()?;
        if let Some(Ok(Reply::Begun(tx))) = core.take_reply(*pending) {
            if tx.read_only() {
                let read = Read::new(
                    0,
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
    while core.transmit().is_some() {}
    Some((core, streams))
}

/// `bytes` framed as a shell frames them, `None` when the framer refuses them.
fn frame(
    pool: &BufferPool,
    bytes: &[u8],
) -> Option<ultragraphdb_protocol_transport::ReceivedFrame> {
    let mut framer = Framer::new(CLUSTER);
    let mut buffer = pool.acquire().ok()?;
    loop {
        let mut data = buffer.take();
        data.clear();
        data.extend_from_slice(bytes.get(..framer.want())?);
        buffer.restore(data);
        match framer.advance(&buffer).ok()? {
            FrameStep::Read(_) => {}
            FrameStep::Frame => return Some(framer.finish(buffer)),
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let now = Instant::now();
    let pool = BufferPool::new(8, 1 << 20);
    let Some((mut core, streams)) = client(&pool, now) else {
        return;
    };
    let Some(received) = frame(&pool, data) else {
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
