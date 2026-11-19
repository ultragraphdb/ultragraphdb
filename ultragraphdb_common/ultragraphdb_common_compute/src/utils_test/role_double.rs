/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compio::runtime::{JoinHandle, spawn};
use futures::channel::mpsc::unbounded;
use ultragraphdb_common_role::{
    Answer, ParkedBytes, ROLE_ITERATIONS, RoleClient, RoleCluster, Server, serve_connection,
};
use ultragraphdb_core_ids::{SessionToken, Version};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BindView, BodyBuilder, BodyView, Closed, ClosedPartition, ClosedView, Command,
    Commit, CommitView, ConditionalWriteView, DirectWriteView, DropCommandView, Entries,
    EntriesEnd, ErrorCode, ErrorReply, ErrorReplyView, FLAG_LAST, FrameView, Header, Held, Join,
    MAX_FRAME_SIZE, Monotonic, ResultBatch, ScanView,
};

use crate::utils_test::double_frame::DoubleFrame;
use crate::utils_test::double_session::DoubleSession;

/// A role a test plays: it accepts what a role's endpoint takes, keeps every frame, and answers as `script` says. Reads against
/// compute and writes against storage run over these, so a scenario decides what compute hears, when. Its clones
/// share what it took and its tasks by `Rc`s of `RefCell`s on the test's one thread, a borrow lasting one call (Ultra
/// rule 9).
#[derive(Debug, Clone)]
pub struct RoleDouble {
    address: String,
    cluster: u64,
    seen: Rc<RefCell<Vec<DoubleFrame>>>,
    connections: Rc<RefCell<Vec<JoinHandle<()>>>>,
    /// Every connection it accepted, those it closed among them.
    accepted: Rc<Cell<usize>>,
    accepting: Rc<RefCell<Option<JoinHandle<()>>>>,
    reading: ParkedBytes,
}

impl RoleDouble {
    /// A double of `cluster` listening on a port of its own, answering by `script`.
    ///
    /// A future dropped at its bind leaves nothing listening.
    ///
    /// # Errors
    ///
    /// What listening or the role secret refuses.
    pub async fn start<F>(
        cluster: &RoleCluster,
        endpoint: Endpoint,
        script: F,
    ) -> Result<Self, RoleError>
    where
        F: Fn(&DoubleFrame) -> Answer + 'static,
    {
        // A closure takes no type parameter, so it takes each error as a trait object (Ultra rules 1 and 9).
        let refused = |error: &dyn std::fmt::Display| RoleError::unavailable(error.to_string());
        let any = Address::any_port("127.0.0.1").map_err(|error| refused(&error))?;
        let listener = Listener::bind(&any)
            .await
            .map_err(|error| refused(&error))?;
        let address = listener
            .local_address()
            .map_err(|error| refused(&error))?
            .to_string();
        let credentials = cluster.secret().credentials()?;
        let server =
            Server::new(cluster.id(), "double", endpoint)?.with_iterations(ROLE_ITERATIONS);
        let double = Self {
            address,
            cluster: cluster.id(),
            seen: Rc::default(),
            connections: Rc::default(),
            accepted: Rc::default(),
            accepting: Rc::default(),
            reading: ParkedBytes::new(1),
        };
        let held = double.clone();
        let script = Rc::new(script);
        let accepting = spawn(async move {
            // An event loop: the double's drop, which drops this task, or a failed accept ends it (Ultra rule 2).
            while let Ok(carrier) = listener.accept().await {
                let (server, credentials) = (server.clone(), credentials.clone());
                // What a test's script pushes, which the test bounds.
                let (pushes, pushed) = unbounded();
                let session = DoubleSession {
                    seen: Rc::clone(&held.seen),
                    script: Rc::clone(&script),
                    pushes,
                    pushed,
                    reading: held.reading.clone(),
                };
                let task = spawn(async move {
                    serve_connection(carrier, &server, credentials, session).await;
                });
                held.connections.borrow_mut().push(task);
                held.accepted.set(held.accepted.get().saturating_add(1));
            }
        });
        *double.accepting.borrow_mut() = Some(accepting);
        Ok(double)
    }

    /// Joins the system group at `system` as what `join` says, on a connection that stays open as long as the double.
    ///
    /// A dropped future drops its connection, and a Join the system group took stands.
    ///
    /// # Errors
    ///
    /// What dialling or the system group refuses.
    pub async fn join(
        &self,
        system: &Address,
        cluster: &RoleCluster,
        name: &str,
        join: &Join,
    ) -> Result<(), String> {
        let mut client = RoleClient::dial(system, cluster.id(), name, cluster.secret().password())
            .await
            .map_err(|error| error.to_string())?;
        let reply = client
            .request(join)
            .await
            .map_err(|error| error.to_string())?;
        drop(reply);
        Ok(())
    }

    /// Where it listens.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The frames of `command` it took, in order.
    #[must_use]
    pub fn frames(&self, command: Command) -> Vec<DoubleFrame> {
        self.seen
            .borrow()
            .iter()
            .filter(|frame| frame.command() == command)
            .cloned()
            .collect()
    }

    /// The commands of every frame it took, in order, those of `leaving_out` left out.
    #[must_use]
    pub fn commands(&self, leaving_out: &[Command]) -> Vec<Command> {
        self.seen
            .borrow()
            .iter()
            .map(DoubleFrame::command)
            .filter(|command| !leaving_out.contains(command))
            .collect()
    }

    /// How many frames of `command` it took.
    #[must_use]
    pub fn count(&self, command: Command) -> usize {
        self.frames(command).len()
    }

    /// How many connections it accepted, those it closed since among them.
    #[must_use]
    pub fn accepted(&self) -> usize {
        self.accepted.get()
    }

    /// How many of the connections it accepted have ended.
    #[must_use]
    pub fn connections_ended(&self) -> usize {
        self.connections
            .borrow()
            .iter()
            .filter(|connection| connection.is_finished())
            .count()
    }

    /// Closes every connection and stops listening, as a process gone does.
    pub fn close(&self) {
        *self.accepting.borrow_mut() = None;
        self.connections.borrow_mut().clear();
    }

    /// Closes every connection it accepted and goes on listening, as a process whose connections were cut does.
    pub fn drop_connections(&self) {
        self.connections.borrow_mut().clear();
    }

    /// Stops reading its one connection past the frame it is reading now, and goes on sending what it answers.
    pub fn stop_reading(&self) {
        self.reading.hold(1);
    }

    /// Reads its connection again.
    pub fn read_again(&self) {
        self.reading.release(1);
    }

    /// The cluster it serves.
    #[must_use]
    pub fn cluster(&self) -> u64 {
        self.cluster
    }
}

/// `body` as the frame ending `stream`.
///
/// # Errors
///
/// As the frame cannot be built.
pub fn last_frame<B: BodyBuilder>(
    cluster: u64,
    stream: u64,
    body: &B,
) -> Result<Vec<u8>, ultragraphdb_error_protocol::ProtocolError> {
    body.frame(
        Header::new(cluster, B::COMMAND)
            .with_stream(stream)
            .with_flags(FLAG_LAST),
    )
}

/// `frame` read back, for its command's view, its bytes a frame some connection took.
///
/// # Errors
///
/// What the frame's rules refuse.
pub fn view_of(
    frame: &[u8],
    cluster: u64,
) -> Result<FrameView<'_>, ultragraphdb_error_protocol::ProtocolError> {
    FrameView::validate(frame, cluster, MAX_FRAME_SIZE)
}

/// What a partition answers when nothing goes wrong: a Bind reading at `version`, a Write and an Operation held, a Commit, a one-frame
/// write and a Drop committed at `version`, a Recall refused as not committed, a Scan as by a partition holding nothing, and a
/// Closed naming `version`'s epoch closed.
#[must_use]
pub fn partition_answer(frame: &DoubleFrame, cluster: u64, version: u64) -> Answer {
    let Ok(view) = view_of(frame.bytes(), cluster) else {
        return Answer::Done;
    };
    let stream = frame.stream();
    let command = frame.command();
    let built = |body: Result<Vec<u8>, _>| body.map_or(Answer::Done, Answer::Reply);
    if command == Command::Bind {
        return bind_reply(&view, cluster, stream, version).map_or(Answer::Done, Answer::Reply);
    }
    if command == Command::Write || command == Command::Operation {
        let header = Header::new(cluster, Command::Held).with_stream(stream);
        return built(Held.frame(header));
    }
    if command == Command::Commit {
        let Ok(commit) = CommitView::from_frame(&view) else {
            return Answer::Done;
        };
        let body = committed(commit.session(), commit.request(), version);
        return built(last_frame(cluster, stream, &body));
    }
    if command == Command::DirectWrite {
        let Ok(write) = DirectWriteView::from_frame(&view) else {
            return Answer::Done;
        };
        let body = committed(write.session(), write.request(), version);
        return built(last_frame(cluster, stream, &body));
    }
    if command == Command::ConditionalWrite {
        let Ok(write) = ConditionalWriteView::from_frame(&view) else {
            return Answer::Done;
        };
        let body = committed(write.session(), write.request(), version);
        return built(last_frame(cluster, stream, &body));
    }
    if command == Command::Drop {
        let Ok(asked) = DropCommandView::from_frame(&view) else {
            return Answer::Done;
        };
        let Ok(body) = asked.to_body() else {
            return Answer::Done;
        };
        let reply = body.with_commit_version(version);
        // A reply through a session carries the session's token, of the reply's epoch (`PROTOCOL.md` §6.5).
        let reply = if asked.session() == [0; 16] {
            reply
        } else {
            reply.with_token(SessionToken::new(1, 1, Version::from_u64(version).epoch()))
        };
        return built(last_frame(cluster, stream, &reply));
    }
    if command == Command::Recall {
        return Answer::Refuse(ErrorReply::new(ErrorCode::NotCommitted, "not held").with_detail(0));
    }
    if command == Command::Scan {
        return scan_answer(frame, cluster, version);
    }
    if command == Command::Closed {
        let Ok(asked) = ClosedView::from_frame(&view) else {
            return Answer::Done;
        };
        let partition = asked.partition(0).map_or(1, |held| held.partition());
        let epoch = Version::from_u64(version).epoch().get();
        let reply = Closed::new(vec![ClosedPartition::new(partition, epoch)]);
        return built(last_frame(cluster, stream, &reply));
    }
    Answer::Done
}

/// A Scan answered as by a partition holding nothing: each range ended with no entry, at `version` and its epoch, the last
/// range's frame flagged last.
#[must_use]
pub fn scan_answer(frame: &DoubleFrame, cluster: u64, version: u64) -> Answer {
    let Ok(view) = view_of(frame.bytes(), cluster) else {
        return Answer::Done;
    };
    let Ok(scan) = ScanView::from_frame(&view) else {
        return Answer::Done;
    };
    let epoch = Version::from_u64(version).epoch().get();
    let count = scan.ranges().count();
    let stream = frame.stream();
    let mut frames = (0..count).filter_map(|at| {
        let range = u32::try_from(at).ok()?;
        let entries = Entries::new(range, EntriesEnd::Ended, epoch).with_version(version);
        let header = Header::new(cluster, Command::Entries).with_stream(stream);
        let header = if at + 1 == count {
            header.with_flags(FLAG_LAST)
        } else {
            header
        };
        entries.frame(header).ok()
    });
    let Some(first) = frames.next() else {
        return Answer::Done;
    };
    frames.for_each(|more| frame.answer_with(more));
    Answer::Reply(first)
}

/// A Commit reply at `version`, through a session its token, of the version's epoch (`PROTOCOL.md` §6.3).
#[must_use]
pub fn committed(session: [u8; 16], request: u64, version: u64) -> Commit {
    let reply = Commit::new(session, request, version);
    if session == [0; 16] {
        return reply;
    }
    let epoch = Version::from_u64(version).epoch();
    reply.with_token(SessionToken::new(1, 1, epoch))
}

fn bind_reply(
    view: &FrameView<'_>,
    cluster: u64,
    stream: u64,
    version: u64,
) -> Result<Vec<u8>, ultragraphdb_error_protocol::ProtocolError> {
    let bind = BindView::from_frame(view)?;
    // A Drop's Bind is answered with `read_version` 0 (`PROTOCOL.md` §13.10).
    let read_version = if bind.kind() == BindKind::Transaction {
        version
    } else {
        0
    };
    let reply: Bind = bind.to_body()?.with_read_version(read_version);
    reply.frame(Header::new(cluster, Command::Bind).with_stream(stream))
}

/// A batch of no row for a pattern of `columns` columns, read at `partition`.
#[must_use]
pub fn empty_batch(columns: u32, partition: u32) -> ResultBatch {
    ResultBatch::new(columns)
        .with_read_at(0, partition)
        .with_monotonic(Monotonic::Snapshot)
}

/// The refusal an Error frame holds.
///
/// # Errors
///
/// What the frame's rules refuse.
pub fn refusal_of(
    frame: &[u8],
    cluster: u64,
) -> Result<ErrorReply, ultragraphdb_error_protocol::ProtocolError> {
    ErrorReplyView::from_frame(&view_of(frame, cluster)?)?.to_body()
}

/// A Result batch as a frame on `stream`, flagged last when `last`.
///
/// # Errors
///
/// As the frame cannot be built.
pub fn batch_frame(
    cluster: u64,
    stream: u64,
    batch: &ResultBatch,
    last: bool,
) -> Result<Vec<u8>, ultragraphdb_error_protocol::ProtocolError> {
    let flags = if last { FLAG_LAST } else { 0 };
    batch.frame(
        Header::new(cluster, Command::Result)
            .with_stream(stream)
            .with_flags(flags),
    )
}
