/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
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
    Bind, BindView, BodyBuilder, BodyView, Command, Commit, CommitView, ConditionalWriteView,
    DirectWriteView, DropCommandView, ErrorCode, ErrorReply, ErrorReplyView, FLAG_LAST, FrameView,
    Header, Held, Join, MAX_FRAME_SIZE, Monotonic, ResultBatch, TermTable,
};

use crate::utils_test::double_frame::DoubleFrame;
use crate::utils_test::double_session::DoubleSession;

/// A role a test plays: it accepts what a role's endpoint takes, keeps every frame, and answers as `script` says. Reads against
/// compute and writes against storage run over these, so a scenario decides what the frontend hears, when.
#[derive(Debug, Clone)]
pub struct RoleDouble {
    address: String,
    cluster: u64,
    seen: Rc<RefCell<Vec<DoubleFrame>>>,
    connections: Rc<RefCell<Vec<JoinHandle<()>>>>,
    accepting: Rc<RefCell<Option<JoinHandle<()>>>>,
    reading: ParkedBytes,
}

impl RoleDouble {
    /// A double of `cluster` listening on a port of its own, answering by `script`.
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
            accepting: Rc::default(),
            reading: ParkedBytes::new(1),
        };
        let held = double.clone();
        let script = Rc::new(script);
        let accepting = spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let (server, credentials) = (server.clone(), credentials.clone());
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
            }
        });
        *double.accepting.borrow_mut() = Some(accepting);
        Ok(double)
    }

    /// Joins the frontend at `frontend` as what `join` says, on a connection that stays open as long as the double.
    ///
    /// # Errors
    ///
    /// What dialling or the frontend refuses.
    pub async fn join(
        &self,
        frontend: &Address,
        cluster: &RoleCluster,
        name: &str,
        join: &Join,
    ) -> Result<(), String> {
        let mut client =
            RoleClient::dial(frontend, cluster.id(), name, cluster.secret().password())
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
/// write and a Drop committed at `version`, and a Recall refused as not committed.
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
        return built(last_frame(
            cluster,
            stream,
            &body.with_commit_version(version),
        ));
    }
    if command == Command::Recall {
        return Answer::Refuse(ErrorReply::new(ErrorCode::NotCommitted, "not held").with_detail(0));
    }
    Answer::Done
}

/// A Commit reply at `version`, through a session its token, of the version's epoch.
fn committed(session: [u8; 16], request: u64, version: u64) -> Commit {
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
    let reply: Bind = bind.to_body()?.with_read_version(version);
    reply.frame(Header::new(cluster, Command::Bind).with_stream(stream))
}

/// A batch of no row for a pattern of `columns` columns, read at `partition`.
#[must_use]
pub fn empty_batch(columns: u32, partition: u32) -> ResultBatch {
    let _terms = TermTable::new();
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
