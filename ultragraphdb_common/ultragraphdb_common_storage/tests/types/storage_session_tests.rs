/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use ultragraphdb_common_role::{
    Answer, Push, ROLE_ITERATIONS, RoleClient, RoleSecret, Server, Session, listen,
    serve_connection,
};
use ultragraphdb_common_storage::{CoreMessage, ParkedFrame, RoleConnectionPhases, StorageSession};
use ultragraphdb_protocol_transport::{BufferPool, Endpoint, Framer, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    Abort, BodyBuilder, BodyView, Cancel, Closed, Command, Commit, DirectWrite, ErrorCode,
    ErrorReply, ErrorReplyView, FLAG_LAST, Header, MAX_FRAME_FLOOR, MAX_FRAME_SIZE, ScanAt,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::types::partition_core::core_harness::{self as harness, edges_write};
use crate::types::partition_core::core_rig::{self as rig, Rig, RigSetup, with_core};
use crate::types::partition_core::core_writes::{alone, committed, edges, plain, scanned};
use crate::types::partition_core::inbox_tap::Tap;
use crate::types::partition_core::yielding_session::Yielding;

const CLUSTER: u64 = 7;
const PASSWORD: &str = "s3cret";
const DUE: Duration = Duration::from_secs(5);

/// Plays a core: each request's stream is ended by a pushed Error of `Refused`, and the request itself, which crossed that Error,
/// is then refused with `Protocol`, as a real core refuses a request on a stream whose transaction has ended, both pushed in order.
async fn core_ending_each_stream(mut inbox: UnboundedReceiver<CoreMessage>) {
    while let Some(message) = inbox.next().await {
        if let CoreMessage::Request { stream, pushes, .. } = message {
            for refusal in [
                ErrorReply::new(ErrorCode::Refused, "why the transaction ended"),
                ErrorReply::new(ErrorCode::Protocol, "a stream no Bind opened"),
            ] {
                let header = Header::new(CLUSTER, Command::Error)
                    .with_stream(stream)
                    .with_flags(FLAG_LAST);
                if let Ok(frame) = refusal.frame(header) {
                    drop(pushes.unbounded_send((stream, Push::Frame(frame))));
                }
            }
        }
    }
}

/// A request crossing the pushed Error that ended its stream takes no refusal of its own: the peer reads why the stream ended.
#[test]
fn test_a_refusal_crossing_its_streams_end_gives_way_to_it() -> TestResult {
    Runtime::new()?.block_on(async {
        // A port of this host that was free a moment ago.
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
        let address = listener.local_address()?;
        let file = std::env::temp_dir().join(format!("session-secret-{}", std::process::id()));
        std::fs::write(&file, PASSWORD)?;
        let held = RoleSecret::read(&file.display().to_string())?.credentials()?;
        let server =
            Server::new(CLUSTER, "test", Endpoint::Partition)?.with_iterations(ROLE_ITERATIONS);
        let (core, inbox) = unbounded();
        spawn(core_ending_each_stream(inbox)).detach();
        spawn(async move {
            if let Ok(carrier) = listener.accept().await {
                let session = Yielding(StorageSession::new(1, core, 1));
                serve_connection(carrier, &server, held, session).await;
            }
        })
        .detach();
        let mut client = RoleClient::dial(&address, CLUSTER, "compute", PASSWORD).await?;
        for _ in 0..2 {
            client.send_only(&Closed::request(&[1])).await?;
            let frame = compio::time::timeout(DUE, client.next_frame())
                .await
                .context("the Error ending the stream")??;
            let view = frame.view()?;
            assert_eq!(view.header().stream(), client.last_stream());
            assert_eq!(
                ErrorReplyView::from_frame(&view)?.code(),
                ErrorCode::Refused
            );
        }
        Ok(())
    })
}

/// The connection every session below serves.
const CONNECTION: u64 = 4;

/// `bytes` as the connection's machine hands a request to its session, its first frame.
fn received(bytes: Vec<u8>) -> TestResult<ReceivedFrame> {
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    let mut buffer = pool.acquire()?;
    buffer.restore(bytes);
    Ok(Framer::from_client(CLUSTER, MAX_FRAME_SIZE).finish(buffer))
}

/// A Closed request on `stream`, the connection's first frame.
fn closed_request(stream: u64) -> TestResult<ReceivedFrame> {
    received(
        Closed::request(&[1]).frame(
            Header::new(CLUSTER, Command::Closed)
                .with_stream(stream)
                .with_sequence(1),
        )?,
    )
}

/// What `session` answers to a request on `stream`, and what it handed the core: the request's frame and the largest frame its
/// peer takes.
async fn answered(
    session: &mut StorageSession,
    inbox: &mut UnboundedReceiver<CoreMessage>,
    stream: u64,
) -> TestResult<(Answer, ParkedFrame, u32)> {
    let answer = session.answer(stream, closed_request(stream)?).await;
    let Ok(CoreMessage::Request {
        connection,
        stream: on,
        frame,
        max_frame,
        ..
    }) = inbox.try_recv()
    else {
        return Err(TestError::new("a request handed on"));
    };
    assert_eq!((connection, on), (CONNECTION, stream));
    Ok((answer, frame, max_frame))
}

/// A session answers a request as open at once, and counts what it hands its core in what the connection holds until the core
/// drops it; that count is what [`Session::parked`] reads.
#[test]
fn test_a_session_counts_what_it_hands_its_core() -> TestResult {
    Runtime::new()?.block_on(async {
        let (core, mut inbox) = unbounded();
        let mut session = StorageSession::new(CONNECTION, core, 100);
        let held = session.parked().context("what it holds")?;
        let (answer, frame, _) = answered(&mut session, &mut inbox, 3).await?;
        assert_eq!(answer, Answer::Open, "its answer is pushed");
        assert_eq!(
            held.count(),
            frame.bytes().len(),
            "counted while the core holds it"
        );
        drop(frame);
        assert_eq!(held.count(), 0, "released as the core drops it");
        Ok(())
    })
}

/// A request out of order on its connection is refused by the session, which hands the core nothing.
#[test]
fn test_a_request_out_of_order_is_refused_at_once() -> TestResult {
    Runtime::new()?.block_on(async {
        let (core, mut inbox) = unbounded();
        let mut session = StorageSession::new(CONNECTION, core, 100);
        let late = received(
            Closed::request(&[1]).frame(
                Header::new(CLUSTER, Command::Closed)
                    .with_stream(3)
                    .with_sequence(5),
            )?,
        )?;
        let Answer::Refuse(refused) = session.answer(3, late).await else {
            return Err(TestError::new("a refusal"));
        };
        assert_eq!(refused.code(), ErrorCode::Protocol, "{}", refused.message());
        assert!(inbox.try_recv().is_err(), "nothing handed on");
        Ok(())
    })
}

/// Each event of the connection reaches the core naming the connection and the stream, and the session's end closes it.
#[test]
fn test_a_sessions_events_reach_its_core() -> TestResult {
    let (core, mut inbox) = unbounded();
    let mut session = StorageSession::new(CONNECTION, core, 100);
    session.ready(5);
    session.left(6);
    session.cancelled(7);
    drop(session);
    let mut told = Vec::new();
    while let Ok(message) = inbox.try_recv() {
        told.push(match message {
            CoreMessage::Ready { connection, stream } => ("ready", connection, stream),
            CoreMessage::Left { connection, stream } => ("left", connection, stream),
            CoreMessage::Cancelled { connection, stream } => ("cancelled", connection, stream),
            CoreMessage::Closed { connection } => ("closed", connection, 0),
            other @ (CoreMessage::Request { .. } | CoreMessage::Catalog(_)) => {
                return Err(TestError::new(format!("no {other:?}")));
            }
        });
    }
    assert_eq!(
        told,
        [
            ("ready", CONNECTION, 5),
            ("left", CONNECTION, 6),
            ("cancelled", CONNECTION, 7),
            ("closed", CONNECTION, 0)
        ]
    );
    Ok(())
}

/// A request carries the largest frame its peer takes: the floor until the Hello is answered, then what the peer stated.
#[test]
fn test_a_request_carries_the_largest_frame_its_peer_takes() -> TestResult {
    Runtime::new()?.block_on(async {
        let (core, mut inbox) = unbounded();
        let mut session = StorageSession::new(CONNECTION, core, 100);
        let (_, _, floor) = answered(&mut session, &mut inbox, 3).await?;
        assert_eq!(usize::try_from(floor)?, MAX_FRAME_FLOOR);
        let stated = u32::try_from(MAX_FRAME_FLOOR)? + 4_096;
        session.welcomed(stated);
        let (_, _, after) = answered(&mut session, &mut inbox, 5).await?;
        assert_eq!(after, stated);
        Ok(())
    })
}

/// What the core pushes leaves through the session on its stream, whole and in the order pushed, a stream's end with no frame
/// among them.
#[test]
fn test_what_the_core_pushes_leaves_in_its_order() -> TestResult {
    Runtime::new()?.block_on(async {
        let (core, mut inbox) = unbounded();
        let mut session = StorageSession::new(CONNECTION, core, 100);
        let _answered = session.answer(7, closed_request(7)?).await;
        let Ok(CoreMessage::Request { pushes, .. }) = inbox.try_recv() else {
            return Err(TestError::new("a request handed on"));
        };
        let pushed = [
            (7, Push::Frame(error_frame(7, false)?)),
            (9, Push::End),
            (7, Push::Frame(error_frame(7, true)?)),
        ];
        for push in pushed.clone() {
            pushes.unbounded_send(push)?;
        }
        for expected in pushed {
            let next = compio::time::timeout(DUE, session.next_push()).await?;
            assert_eq!(next, expected);
        }
        Ok(())
    })
}

/// A session timing its requests counts each request once, at its stream's first answer: a later frame of that stream, a stream
/// cancelled and a stream ended count nothing more.
#[test]
fn test_a_session_times_each_request_to_its_first_answer() -> TestResult {
    Runtime::new()?.block_on(async {
        let (core, mut inbox) = unbounded();
        let origin = compio::time::interval(Duration::from_secs(1)).tick().await;
        let phases = RoleConnectionPhases::new(origin);
        let mut session =
            StorageSession::new(CONNECTION, core, 1 << 20).with_phases(phases.clone());
        for stream in [3, 5, 7] {
            let _answered = session.answer(stream, closed_request(stream)?).await;
        }
        let Ok(CoreMessage::Request { pushes, .. }) = inbox.try_recv() else {
            return Err(TestError::new("a request handed on"));
        };
        session.cancelled(5);
        for push in [
            (3, Push::Frame(error_frame(3, false)?)),
            (3, Push::Frame(error_frame(3, true)?)),
            (5, Push::Frame(error_frame(5, true)?)),
            (7, Push::End),
            (7, Push::Frame(error_frame(7, true)?)),
        ] {
            pushes.unbounded_send(push)?;
            let _taken = compio::time::timeout(DUE, session.next_push()).await?;
        }
        // Stream 3's first answer and stream 7's end; stream 5 was cancelled before its answer.
        assert_eq!(phases.count(), 2);
        Ok(())
    })
}

/// A request reaching no core, its inbox gone, is refused as unavailable: the partition stopped.
#[test]
fn test_a_request_to_a_core_gone_is_refused_as_stopped() -> TestResult {
    Runtime::new()?.block_on(async {
        let (core, inbox) = unbounded();
        let mut session = StorageSession::new(CONNECTION, core, 100);
        drop(inbox);
        let Answer::Refuse(refused) = session.answer(1, closed_request(1)?).await else {
            return Err(TestError::new("a refusal"));
        };
        assert_eq!(
            (refused.code(), refused.message()),
            (ErrorCode::Unavailable, "the partition stopped")
        );
        Ok(())
    })
}

/// An Error frame on `stream`, ending it when `last`.
fn error_frame(stream: u64, last: bool) -> TestResult<Vec<u8>> {
    let flags = if last { FLAG_LAST } else { 0 };
    let header = Header::new(CLUSTER, Command::Error)
        .with_stream(stream)
        .with_flags(flags);
    Ok(ErrorReply::new(ErrorCode::Refused, "pushed").frame(header)?)
}

/// How many one-frame writes a scenario of a connection read on while its core takes nothing sends.
const WRITES: usize = 10;
/// The scenario's `storage.held_bytes`: three frames of about [`FRAME_BYTES`] reach it, two do not.
const HELD_BYTES: usize = 4_096;
/// About the size of each frame the bound's scenario sends.
const FRAME_BYTES: usize = 1_500;

/// `count` DirectWrites, each a commit of a subject of its own.
fn direct_writes(count: usize) -> Vec<DirectWrite> {
    (0..count)
        .map(|n| {
            let subject = format!("s{n}");
            edges(rig::TENANT, &[(subject.as_str(), "q", "o")])
        })
        .collect()
}

/// A DirectWrite of the subject `n` whose frame is at least `bytes` long, its subject's IRI padded to reach it, and its frame's
/// size: a frame's size is not every number.
fn direct_write_of(n: usize, bytes: usize) -> TestResult<(DirectWrite, usize)> {
    let size = |padding: usize| -> TestResult<(DirectWrite, usize)> {
        let subject = format!("s{n}-{}", "a".repeat(padding));
        let write = edges(rig::TENANT, &[(subject.as_str(), "q", "o")]);
        let frame = write.frame(Header::new(rig::CLUSTER, Command::DirectWrite))?;
        Ok((write, frame.len()))
    };
    let (_, unpadded) = size(0)?;
    let padding = bytes
        .checked_sub(unpadded)
        .context("a frame smaller than the size asked")?;
    size(padding)
}

/// The requests the tap has counted, plus `more`.
fn requests_after(tap: &Tap, more: usize) -> TestResult<u64> {
    Ok(tap.requests() + u64::try_from(more)?)
}

/// A commit on a connection of its own whose round's sync the test holds: the core's turn waits on that sync, as it waits on any,
/// and takes nothing meanwhile. The connection, to read that commit's answer once the sync is let go.
async fn syncing(core: &Rig) -> TestResult<RoleClient> {
    let mut holder = core.dial().await?;
    core.sync_gate.hold();
    holder
        .send_only(&edges(rig::TENANT, &[("held", "q", "o")]))
        .await?;
    core.sync_gate.until_waiting().await?;
    Ok(holder)
}

/// The commit `syncing` sent, answered once its sync was let go.
async fn held_commit(holder: &mut RoleClient) -> TestResult {
    let answer = committed(holder).await?;
    let _committed =
        answer.map_err(|refusal| TestError::new(format!("the held commit, not {refusal:?}")))?;
    Ok(())
}

/// Reads `count` commits on `client`, failing on any refusal.
async fn all_committed(client: &mut RoleClient, count: usize) -> TestResult<Vec<u64>> {
    let mut versions = Vec::with_capacity(count);
    for _ in 0..count {
        let answer = committed(client).await?;
        let commit =
            answer.map_err(|refusal| TestError::new(format!("a commit, not {refusal:?}")))?;
        versions.push(commit.version);
    }
    Ok(versions)
}

/// While the core's turn waits on a log sync, a role connection is read on: every request reaches the core's inbox, and each is
/// answered once the core has run it.
#[test]
fn test_a_connection_is_read_on_while_its_core_syncs() -> TestResult {
    with_core("session-read-on", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let writes = direct_writes(WRITES);
        let mut holder = syncing(&core).await?;
        let expected = requests_after(&core.tap, writes.len())?;
        for write in &writes {
            client.send_only(write).await?;
        }
        core.tap.until_requests(expected).await?;
        core.sync_gate.release();
        let _versions = all_committed(&mut client, writes.len()).await?;
        held_commit(&mut holder).await
    })
}

/// The requests a connection hands its core and the core has not taken count against `storage.held_bytes`: the connection reads
/// until they reach it, then stops, refusing nothing, and reads on once the core takes them.
#[test]
fn test_requests_waiting_for_the_core_count_against_the_bound() -> TestResult {
    let setup = RigSetup {
        held_bytes: HELD_BYTES,
        ..RigSetup::default()
    };
    with_core("session-bound", setup, async |core| {
        let mut client = core.dial().await?;
        let (_, frame) = direct_write_of(0, FRAME_BYTES)?;
        // The frames that reach the bound, the last read whole: three of about 1,500 bytes under 4,096.
        let fitting = HELD_BYTES.div_ceil(frame);
        // Two more than fit: one stopped at the bound, one behind it.
        let writes = (0..fitting + 2)
            .map(|n| direct_write_of(n, FRAME_BYTES).map(|(write, _)| write))
            .collect::<TestResult<Vec<_>>>()?;
        let mut holder = syncing(&core).await?;
        let read = requests_after(&core.tap, fitting)?;
        let all = requests_after(&core.tap, writes.len())?;
        for write in &writes {
            client.send_only(write).await?;
        }
        core.tap.until_requests(read).await?;
        // Long enough for a connection that did not stop to read the next frame on loopback.
        compio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            core.tap.requests(),
            read,
            "the connection stops at its bound"
        );
        core.sync_gate.release();
        let _versions = all_committed(&mut client, writes.len()).await?;
        assert_eq!(core.tap.requests(), all);
        held_commit(&mut holder).await
    })
}

/// An Abort the core takes after a sync ends its stream with no frame, and a request behind it on the connection is answered.
#[test]
fn test_an_aborts_stream_ends_with_no_frame() -> TestResult {
    with_core("session-abort", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let (bound, _) = harness::bind(&mut client, 0x5a).await?;
        let mut holder = syncing(&core).await?;
        let expected = requests_after(&core.tap, 2)?;
        client.send_on(bound, &Abort).await?;
        client.send_only(&direct_writes(1).remove(0)).await?;
        let behind = client.last_stream();
        core.tap.until_requests(expected).await?;
        core.sync_gate.release();
        assert_eq!(
            next_on(&mut client).await?,
            (behind, Command::Commit.number())
        );
        held_commit(&mut holder).await?;
        assert!(
            !any_frame(&mut client).await,
            "no frame on the Abort's stream"
        );
        Ok(())
    })
}

/// Commits a frontend sends over one role connection while a log sync is in flight share the next round: one log append and one
/// sync for them all, each acknowledged once that round is applied (commit-rounds, "Commits are grouped into rounds"). A commit
/// first warms the log, whose first round also syncs its segment.
#[test]
fn test_commits_a_frontend_sends_during_a_sync_share_the_next_round() -> TestResult {
    with_core("session-one-round", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        client.send_only(&direct_writes(1).remove(0)).await?;
        let _warmed = committed(&mut client).await?;
        let writes = (1..=WRITES)
            .map(|n| {
                let subject = format!("s{n}");
                edges(rig::TENANT, &[(subject.as_str(), "q", "o")])
            })
            .collect::<Vec<_>>();
        let before = core.log_io.totals();
        let mut holder = syncing(&core).await?;
        let expected = requests_after(&core.tap, writes.len())?;
        for write in &writes {
            client.send_only(write).await?;
        }
        core.tap.until_requests(expected).await?;
        core.sync_gate.release();
        let mut versions = all_committed(&mut client, writes.len()).await?;
        held_commit(&mut holder).await?;
        let spent = core.log_io.totals().since(&before);
        // The held commit's round, then one round for every commit sent during its sync.
        let rounds = 2;
        assert_eq!(
            (spent.writes(), spent.syncs()),
            (rounds, rounds),
            "one append and one sync for each round"
        );
        versions.sort_unstable();
        versions.dedup();
        assert_eq!(
            versions.len(),
            writes.len(),
            "each commit has a version of its own"
        );
        Ok(())
    })
}

/// A request that reaches the core's inbox while its turn waits on a log sync, the core then stopping, is refused as the
/// partition stopped: the stop answers what waits for the core.
#[test]
fn test_a_stop_refuses_the_requests_waiting_for_the_core() -> TestResult {
    with_core("session-stop", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let mut holder = syncing(&core).await?;
        let expected = requests_after(&core.tap, 1)?;
        client.send_only(&direct_writes(1).remove(0)).await?;
        core.tap.until_requests(expected).await?;
        core.stopping.set();
        core.sync_gate.release();
        held_commit(&mut holder).await?;
        let refused = committed(&mut client).await?;
        let Err((code, message)) = refused else {
            return Err(TestError::new("a refusal, not a commit"));
        };
        assert_eq!(
            (code, message.as_str()),
            (ErrorCode::Unavailable, "the partition stopped")
        );
        Ok(())
    })
}

/// The next frame on `client` within [`rig::DUE`]: its stream and command number.
async fn next_on(client: &mut RoleClient) -> TestResult<(u64, u16)> {
    let frame = compio::time::timeout(rig::DUE, client.next_frame())
        .await
        .context("a frame within its time")??;
    Ok((frame.stream(), frame.command()))
}

/// Whether any frame arrives on `client` within a moment: long enough for one already sent to arrive on loopback.
async fn any_frame(client: &mut RoleClient) -> bool {
    compio::time::timeout(Duration::from_millis(200), client.next_frame())
        .await
        .is_ok()
}

/// A transaction's Writes and its Commit, all sent while a log sync is in flight, are answered in the order they were sent.
#[test]
fn test_a_streams_requests_waiting_for_the_core_are_answered_in_order() -> TestResult {
    with_core("session-stream-order", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let (stream, _) = harness::bind(&mut client, 0x61).await?;
        let replies = [Command::Held, Command::Held, Command::Commit];
        let mut holder = syncing(&core).await?;
        let expected = requests_after(&core.tap, replies.len())?;
        client.send_on(stream, &edges_write(&[("a", "b")])).await?;
        client.send_on(stream, &edges_write(&[("c", "d")])).await?;
        client.send_on(stream, &Commit::new([0; 16], 0, 0)).await?;
        core.tap.until_requests(expected).await?;
        core.sync_gate.release();
        let mut answered = Vec::with_capacity(replies.len());
        for _ in &replies {
            answered.push(next_on(&mut client).await?);
        }
        assert_eq!(answered, replies.map(|reply| (stream, reply.number())));
        held_commit(&mut holder).await
    })
}

/// A Cancel crossing a Write that waits for the core ends its stream: no Held follows on it, and a request behind it on the
/// connection is answered.
#[test]
fn test_a_cancel_crossing_a_request_waiting_for_the_core_ends_its_stream() -> TestResult {
    with_core("session-cancel", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let (stream, _) = harness::bind(&mut client, 0x62).await?;
        let mut holder = syncing(&core).await?;
        let expected = requests_after(&core.tap, 2)?;
        client.send_on(stream, &edges_write(&[("a", "b")])).await?;
        client.send_on(stream, &Cancel).await?;
        client.send_only(&direct_writes(1).remove(0)).await?;
        let behind = client.last_stream();
        core.tap.until_requests(expected).await?;
        core.sync_gate.release();
        assert_eq!(
            next_on(&mut client).await?,
            (behind, Command::Commit.number())
        );
        held_commit(&mut holder).await?;
        assert!(
            !any_frame(&mut client).await,
            "no Held on the cancelled stream"
        );
        Ok(())
    })
}

/// A connection closing with a Write that waits for the core aborts its transaction: nothing of the Write commits.
#[test]
fn test_a_connection_closing_with_a_write_waiting_for_the_core_aborts_its_transaction() -> TestResult
{
    with_core("session-close", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let (stream, _) = harness::bind(&mut client, 0x63).await?;
        let mut holder = syncing(&core).await?;
        let expected = requests_after(&core.tap, 1)?;
        let closes = core.tap.closed() + 1;
        client
            .send_on(stream, &edges_write(&[("gone", "o")]))
            .await?;
        core.tap.until_requests(expected).await?;
        drop(client);
        core.tap.until_closed(closes).await?;
        core.sync_gate.release();
        held_commit(&mut holder).await?;
        let mut reader = core.dial().await?;
        // A subject the dictionary does not name has no forward list either.
        let Ok(key) = harness::forward_key(&mut reader, "gone").await else {
            return Ok(());
        };
        let (run, refused) =
            scanned(&mut reader, &plain(ScanAt::Latest, 0, 0, &[alone(key)])).await?;
        assert!(refused.is_none(), "{refused:?}");
        assert!(
            run.iter().all(|batch| batch.entries.is_empty()),
            "the aborted Write left no edge"
        );
        Ok(())
    })
}

/// How many one-frame writes the burst test answers in one round.
const BURST: usize = 1_000;

/// A burst of answers on one role connection (design D10): `BURST` DirectWrites sent while a log sync is held, answered in one
/// round once it is let go, and read back one frame at a time as the frontend's role link reads them. The figures, from the sync's
/// release to the first answer read and from the first to the last, are printed for the change's notes; no bound is asserted.
#[test]
fn test_a_burst_of_answers_leaves_one_role_connection() -> TestResult {
    with_core("session-burst", RigSetup::default(), async |core| {
        let origin = compio::time::interval(Duration::from_secs(1)).tick().await;
        let clock = RoleConnectionPhases::new(origin);
        let mut client = core.dial().await?;
        client.send_only(&direct_writes(1).remove(0)).await?;
        let _warmed = committed(&mut client).await?;
        let writes = (1..=BURST)
            .map(|n| {
                let subject = format!("burst-{n}");
                edges(rig::TENANT, &[(subject.as_str(), "q", "o")])
            })
            .collect::<Vec<_>>();
        let mut holder = syncing(&core).await?;
        let expected = requests_after(&core.tap, writes.len())?;
        for write in &writes {
            client.send_only(write).await?;
        }
        core.tap.until_requests(expected).await?;
        let before = core.log_io.totals();
        let released = clock.now_us();
        core.sync_gate.release();
        let answer = committed(&mut client).await?;
        let first = clock.now_us();
        let _first =
            answer.map_err(|refusal| TestError::new(format!("a commit, not {refusal:?}")))?;
        let _rest = all_committed(&mut client, writes.len() - 1).await?;
        let last = clock.now_us();
        held_commit(&mut holder).await?;
        let spent = core.log_io.totals().since(&before);
        let drain_us = last.saturating_sub(first).max(1);
        println!(
            "burst: {} answers, {} log syncs; release to first answer {} us, first to last {} us, {} answers a second read",
            writes.len(),
            spent.syncs(),
            first.saturating_sub(released),
            drain_us,
            u64::try_from(writes.len())? * 1_000_000 / drain_us
        );
        Ok(())
    })
}
