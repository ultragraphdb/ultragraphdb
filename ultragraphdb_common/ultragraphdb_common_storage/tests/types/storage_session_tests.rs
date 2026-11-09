/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleClient, RoleSecret, Server, Session, listen, serve_connection,
};
use ultragraphdb_common_storage::{CoreMessage, StorageSession};
use ultragraphdb_protocol_transport::{BufferPool, Endpoint, Framer, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    Abort, Bind, BindKind, BindView, BodyBuilder, BodyView, Cancel, Closed, Command, Commit,
    DirectWrite, ErrorCode, ErrorReply, ErrorReplyView, FLAG_LAST, Header, MAX_FRAME_FLOOR,
    MAX_FRAME_SIZE, ScanAt,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::types::partition_core::core_harness::{self as harness, edges_write};
use crate::types::partition_core::core_rig::{self as rig, RigSetup, with_core};
use crate::types::partition_core::core_writes::{alone, committed, edges, plain, scanned};
use crate::types::partition_core::inbox_tap::Tap;
use crate::types::partition_core::yielding_session::Yielding;

const CLUSTER: u64 = 7;
const PASSWORD: &str = "s3cret";
const DUE: Duration = Duration::from_secs(5);

/// Plays a core: each request's stream is ended by a pushed Error of `Refused`, and the request itself, which crossed that Error,
/// is refused with `Protocol`, as a real core refuses a request on a stream whose transaction has ended.
async fn core_ending_each_stream(mut inbox: UnboundedReceiver<CoreMessage>) {
    while let Some(message) = inbox.next().await {
        if let CoreMessage::Request {
            stream,
            done,
            pushes,
            ..
        } = message
        {
            let ended = ErrorReply::new(ErrorCode::Refused, "why the transaction ended");
            let header = Header::new(CLUSTER, Command::Error)
                .with_stream(stream)
                .with_flags(FLAG_LAST);
            if let Ok(frame) = ended.frame(header) {
                drop(pushes.unbounded_send((stream, frame)));
            }
            let crossed = ErrorReply::new(ErrorCode::Protocol, "a stream no Bind opened");
            drop(done.send(Answer::Refuse(crossed)));
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

/// `bytes` as the connection's machine hands a request to its session.
fn received(bytes: Vec<u8>) -> TestResult<ReceivedFrame> {
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    let mut buffer = pool.acquire()?;
    buffer.restore(bytes);
    Ok(Framer::from_client(CLUSTER, MAX_FRAME_SIZE).finish(buffer))
}

/// An Error frame on `stream`, ending it when `last`.
fn error_frame(stream: u64, last: bool) -> TestResult<Vec<u8>> {
    let flags = if last { FLAG_LAST } else { 0 };
    let header = Header::new(CLUSTER, Command::Error)
        .with_stream(stream)
        .with_flags(flags);
    Ok(ErrorReply::new(ErrorCode::Refused, "pushed").frame(header)?)
}

/// What `session` answers to a request on `stream`, the core pushing each of `pushed` and then answering `answer`: the session's
/// answer and the largest frame the request carried to the core.
async fn answered(
    session: &mut StorageSession,
    inbox: &mut UnboundedReceiver<CoreMessage>,
    stream: u64,
    pushed: Vec<(u64, Vec<u8>)>,
    answer: Answer,
) -> TestResult<(Answer, u32)> {
    let request = received(Closed::request(&[1]).frame(Header::new(CLUSTER, Command::Closed))?)?;
    let core = async {
        let Some(CoreMessage::Request {
            connection,
            stream: on,
            done,
            pushes,
            max_frame,
            ..
        }) = inbox.next().await
        else {
            return Err(TestError::new("a request"));
        };
        assert_eq!((connection, on), (CONNECTION, stream));
        for push in pushed {
            pushes.unbounded_send(push)?;
        }
        done.send(answer)
            .map_err(|_| TestError::new("the session waits"))?;
        Ok(max_frame)
    };
    let (answered, max_frame) = futures::future::join(session.answer(stream, request), core).await;
    Ok((answered, max_frame?))
}

/// A session announces its connection with what it counts parked, which is what [`Session::parked`] reads.
#[test]
fn test_a_session_announces_what_it_counts_parked() -> TestResult {
    let (core, mut inbox) = unbounded();
    let session = StorageSession::new(CONNECTION, core, 100);
    let Ok(CoreMessage::Opened { connection, parked }) = inbox.try_recv() else {
        return Err(TestError::new("the connection's opening"));
    };
    assert_eq!(connection, CONNECTION);
    parked.hold(10);
    assert_eq!(session.parked().context("what it parks")?.count(), 10);
    Ok(())
}

/// Each event of the connection reaches the core naming the connection and the stream, and the session's end closes it.
#[test]
fn test_a_sessions_events_reach_its_core() -> TestResult {
    let (core, mut inbox) = unbounded();
    let mut session = StorageSession::new(CONNECTION, core, 100);
    let _opened = inbox.try_recv();
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
            other @ (CoreMessage::Opened { .. }
            | CoreMessage::Request { .. }
            | CoreMessage::Catalog(_)) => return Err(TestError::new(format!("no {other:?}"))),
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
        let _opened = inbox.next().await;
        let (_, floor) = answered(&mut session, &mut inbox, 3, Vec::new(), Answer::Open).await?;
        assert_eq!(usize::try_from(floor)?, MAX_FRAME_FLOOR);
        let stated = u32::try_from(MAX_FRAME_FLOOR)? + 4_096;
        session.welcomed(stated);
        let (_, after) = answered(&mut session, &mut inbox, 5, Vec::new(), Answer::Open).await?;
        assert_eq!(after, stated);
        Ok(())
    })
}

/// A refusal stands unless a frame ending its own stream waits to leave: a frame on the stream that does not end it, or one ending
/// another stream, leaves it as it is. Every frame looked at still leaves, in the order pushed.
#[test]
fn test_a_refusal_gives_way_only_to_its_own_streams_end() -> TestResult {
    Runtime::new()?.block_on(async {
        let (core, mut inbox) = unbounded();
        let mut session = StorageSession::new(CONNECTION, core, 100);
        let _opened = inbox.next().await;
        let refused = ErrorReply::new(ErrorCode::Protocol, "a stream no Bind opened");
        let refuse = || Answer::Refuse(refused.clone());
        let (alone, _) = answered(&mut session, &mut inbox, 3, Vec::new(), refuse()).await?;
        assert_eq!(alone, refuse(), "nothing pushed");
        let open = (3, error_frame(3, false)?);
        let other = (9, error_frame(9, true)?);
        let pushed = vec![open.clone(), other.clone()];
        let (crossed, _) = answered(&mut session, &mut inbox, 3, pushed, refuse()).await?;
        assert_eq!(crossed, refuse(), "neither ends stream 3");
        let ended = (3, error_frame(3, true)?);
        let (gave_way, _) =
            answered(&mut session, &mut inbox, 3, vec![ended.clone()], refuse()).await?;
        assert_eq!(gave_way, Answer::Open, "stream 3's end waits to leave");
        for expected in [open, other, ended] {
            let next = compio::time::timeout(DUE, session.next_push()).await?;
            assert_eq!(next, expected);
        }
        Ok(())
    })
}

/// A frame the core pushes leaves through the session on its stream, whole.
#[test]
fn test_a_pushed_frame_leaves_on_its_stream() -> TestResult {
    Runtime::new()?.block_on(async {
        let (core, mut inbox) = unbounded();
        let mut session = StorageSession::new(CONNECTION, core, 100);
        let _opened = inbox.next().await;
        let (pushes_to, pushed) = (7, error_frame(7, true)?);
        let request =
            received(Closed::request(&[1]).frame(Header::new(CLUSTER, Command::Closed))?)?;
        let played = async {
            if let Some(CoreMessage::Request { pushes, done, .. }) = inbox.next().await {
                pushes.unbounded_send((pushes_to, pushed.clone()))?;
                done.send(Answer::Open)
                    .map_err(|_| TestError::new("the session waits"))?;
            }
            TestResult::Ok(())
        };
        let (_, served) = futures::future::join(session.answer(7, request), played).await;
        served?;
        let next = compio::time::timeout(DUE, session.next_push()).await?;
        assert_eq!(next, (pushes_to, pushed));
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
        let request =
            received(Closed::request(&[1]).frame(Header::new(CLUSTER, Command::Closed))?)?;
        let Answer::Refuse(refused) = session.answer(1, request).await else {
            return Err(TestError::new("a refusal"));
        };
        assert_eq!(
            (refused.code(), refused.message()),
            (ErrorCode::Unavailable, "the partition stopped")
        );
        Ok(())
    })
}

/// A request its core drops unanswered, as a core stopping drops what it holds, is refused as unavailable: the partition
/// stopped.
#[test]
fn test_a_request_its_core_drops_is_refused_as_stopped() -> TestResult {
    Runtime::new()?.block_on(async {
        let (core, mut inbox) = unbounded();
        let mut session = StorageSession::new(CONNECTION, core, 100);
        let request =
            received(Closed::request(&[1]).frame(Header::new(CLUSTER, Command::Closed))?)?;
        let dropping = async {
            while let Some(message) = inbox.next().await {
                if let CoreMessage::Request { done, .. } = message {
                    drop(done);
                    return;
                }
            }
        };
        let (answer, ()) = futures::future::join(session.answer(1, request), dropping).await;
        let Answer::Refuse(refused) = answer else {
            return Err(TestError::new("a refusal"));
        };
        assert_eq!(
            (refused.code(), refused.message()),
            (ErrorCode::Unavailable, "the partition stopped")
        );
        Ok(())
    })
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

/// While the core takes nothing, as while its turn waits on a log sync, a role connection is read on: every request reaches the
/// core's inbox, and each is answered once the core has run it.
#[test]
fn test_a_connection_is_read_on_while_its_core_syncs() -> TestResult {
    with_core("session-read-on", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let writes = direct_writes(WRITES);
        let expected = requests_after(&core.tap, writes.len())?;
        core.tap.hold();
        for write in &writes {
            client.send_only(write).await?;
        }
        core.tap.until_requests(expected).await?;
        core.tap.release()?;
        for _ in &writes {
            let answer = committed(&mut client).await?;
            let _committed =
                answer.map_err(|refusal| TestError::new(format!("a commit, not {refusal:?}")))?;
        }
        Ok(())
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
        let read = requests_after(&core.tap, fitting)?;
        let all = requests_after(&core.tap, writes.len())?;
        core.tap.hold();
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
        core.tap.release()?;
        for _ in &writes {
            let answer = committed(&mut client).await?;
            let _committed =
                answer.map_err(|refusal| TestError::new(format!("a commit, not {refusal:?}")))?;
        }
        assert_eq!(core.tap.requests(), all);
        Ok(())
    })
}

/// An Abort the core takes after a wait ends its stream with no frame, and a request behind it on the connection is answered.
#[test]
fn test_an_aborts_stream_ends_with_no_frame() -> TestResult {
    with_core("session-abort", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let bind = Bind::new(
            rig::PARTITION,
            BindKind::Transaction,
            rig::TENANT,
            0,
            rig::PRINCIPAL,
            [0x5a; 16],
        );
        let reply = compio::time::timeout(rig::DUE, client.request(&bind))
            .await
            .context("the Bind's reply")??;
        let _bound = BindView::from_frame(&reply.view()?)?;
        drop(reply);
        let bound = client.last_stream();
        let expected = requests_after(&core.tap, 2)?;
        core.tap.hold();
        client.send_on(bound, &Abort).await?;
        client.send_only(&direct_writes(1).remove(0)).await?;
        let behind = client.last_stream();
        core.tap.until_requests(expected).await?;
        core.tap.release()?;
        let answer = compio::time::timeout(rig::DUE, client.next_frame())
            .await
            .context("the write's answer")??;
        assert_eq!(
            (answer.stream(), answer.command()),
            (behind, Command::Commit.number())
        );
        drop(answer);
        // Long enough for a frame the Abort's stream would end with to arrive on loopback.
        let after = compio::time::timeout(Duration::from_millis(200), client.next_frame()).await;
        assert!(after.is_err(), "no frame follows on any stream");
        Ok(())
    })
}

/// Commits a frontend sends over one role connection while the core takes nothing, as while its turn waits on a log sync, share
/// the next round: one log append and one sync for them all, each acknowledged once that round is applied (commit-rounds,
/// "Commits are grouped into rounds").
#[test]
fn test_commits_a_frontend_sends_during_a_sync_share_the_next_round() -> TestResult {
    with_core("session-one-round", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let writes = direct_writes(WRITES);
        let expected = requests_after(&core.tap, writes.len())?;
        let before = core.log_io.totals();
        core.tap.hold();
        for write in &writes {
            client.send_only(write).await?;
        }
        core.tap.until_requests(expected).await?;
        core.tap.release()?;
        let mut versions = Vec::with_capacity(writes.len());
        for _ in &writes {
            let answer = committed(&mut client).await?;
            let commit =
                answer.map_err(|refusal| TestError::new(format!("a commit, not {refusal:?}")))?;
            versions.push(commit.version);
        }
        let spent = core.log_io.totals().since(&before);
        assert_eq!(
            (spent.writes(), spent.syncs()),
            (1, 1),
            "one append and one sync for the round"
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

/// A transaction's Writes and its Commit, all waiting for the core, are answered in the order they were sent.
#[test]
fn test_a_streams_requests_waiting_for_the_core_are_answered_in_order() -> TestResult {
    with_core("session-stream-order", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let (stream, _) = harness::bind(&mut client, 0x61).await?;
        let replies = [Command::Held, Command::Held, Command::Commit];
        let expected = requests_after(&core.tap, replies.len())?;
        core.tap.hold();
        client.send_on(stream, &edges_write(&[("a", "b")])).await?;
        client.send_on(stream, &edges_write(&[("c", "d")])).await?;
        client.send_on(stream, &Commit::new([0; 16], 0, 0)).await?;
        core.tap.until_requests(expected).await?;
        core.tap.release()?;
        let mut answered = Vec::with_capacity(replies.len());
        for _ in &replies {
            answered.push(next_on(&mut client).await?);
        }
        assert_eq!(answered, replies.map(|reply| (stream, reply.number())));
        Ok(())
    })
}

/// A Cancel crossing a Write that waits for the core ends its stream: no Held follows on it, and a request behind it on the
/// connection is answered.
#[test]
fn test_a_cancel_crossing_a_request_waiting_for_the_core_ends_its_stream() -> TestResult {
    with_core("session-cancel", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let (stream, _) = harness::bind(&mut client, 0x62).await?;
        let expected = requests_after(&core.tap, 2)?;
        core.tap.hold();
        client.send_on(stream, &edges_write(&[("a", "b")])).await?;
        client.send_on(stream, &Cancel).await?;
        client.send_only(&direct_writes(1).remove(0)).await?;
        let behind = client.last_stream();
        core.tap.until_requests(expected).await?;
        core.tap.release()?;
        assert_eq!(
            next_on(&mut client).await?,
            (behind, Command::Commit.number())
        );
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
        let expected = requests_after(&core.tap, 1)?;
        let closes = core.tap.closed() + 1;
        core.tap.hold();
        client
            .send_on(stream, &edges_write(&[("gone", "o")]))
            .await?;
        core.tap.until_requests(expected).await?;
        drop(client);
        core.tap.until_closed(closes).await?;
        core.tap.release()?;
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
