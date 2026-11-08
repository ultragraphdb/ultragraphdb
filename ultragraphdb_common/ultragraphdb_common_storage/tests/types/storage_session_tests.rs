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
    BodyBuilder, BodyView, Closed, Command, ErrorCode, ErrorReply, ErrorReplyView, FLAG_LAST,
    Header, MAX_FRAME_FLOOR, MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

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
