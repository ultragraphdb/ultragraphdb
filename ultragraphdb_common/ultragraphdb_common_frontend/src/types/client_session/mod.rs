/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_session_bind;
mod client_session_commit;
mod client_session_one_frame;
mod client_session_prepared;
mod client_session_reads;
mod client_session_requests;
mod client_session_session;
mod client_session_timers;
mod client_session_writes;

use std::collections::{BTreeMap, VecDeque};
use std::net::IpAddr;
use std::time::Instant;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::OpenFrame;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, ErrorReply, FLAG_LAST, Header, HeaderView, MAX_FRAME_FLOOR,
    StatementCache,
};

use crate::traits::authorizer::Authorizer;
use crate::types::asked_credentials::AskedCredentials;
use crate::types::claim::Claim;
use crate::types::client_read::ClientRead;
use crate::types::client_transaction::ClientTransaction;
use crate::types::endpoint_core::EndpointCore;
use crate::types::owner_handle::DropStep;
use crate::types::role_link::RoleLink;
use crate::types::route_key::RouteKey;
use crate::types::route_sink::RouteSink;
use crate::types::session_event::SessionEvent;

/// A DirectWrite or a ConditionalWrite on its way to its partition.
#[derive(Debug)]
struct OneFrame {
    frame: Vec<u8>,
    tenant: u64,
    session: u128,
    /// Whether it has had its one resend, after a refusal naming another owner.
    resent: bool,
    /// The owner that refusal named.
    forced: Option<u32>,
    attempt: u32,
    route: Option<(RoleLink, u64)>,
}

/// A Drop on its way to its partition.
#[derive(Debug)]
struct DropRelay {
    frame: Vec<u8>,
    session: u128,
    tenant: u64,
    /// The catalog version its reply calls for.
    step: Option<DropStep>,
    /// Its reply, waiting for that version.
    reply: Option<Vec<u8>>,
    link: RoleLink,
    stream: u64,
    /// Whether the Drop itself has left on its Bind stream, after the Bind's answer.
    sent: bool,
}

/// A retried Commit asked of every partition, and what they answered so far (`PROTOCOL.md` §11.5).
#[derive(Debug, Default)]
struct Recalling {
    session: u128,
    request: u64,
    unanswered: usize,
    stored: Option<Vec<u8>>,
    busy: bool,
    unavailable: bool,
    out_of_order: Option<u64>,
}

/// One client connection: the rules of `PROTOCOL.md` §§6 to 8 and 11 past those its connection's machine keeps. It holds nothing
/// another connection reads: its requests arrive by [`ultragraphdb_common_role::Session::answer`], and everything else, each
/// role's frames and each owner's answer that does not wait, by message, so one request's wait holds up no other. Whatever it
/// sends goes out in the order its transaction's rules need, through [`ultragraphdb_common_role::Session::next_push`].
#[derive(Debug)]
pub struct ClientSession<A> {
    core: EndpointCore<A>,
    credentials: AskedCredentials,
    /// The name the connection's latest exchange authenticates as.
    asked_name: Option<String>,
    peer: Option<IpAddr>,
    principal: u64,
    max_frame: u32,
    statements: StatementCache,
    transactions: BTreeMap<u64, ClientTransaction>,
    reads: BTreeMap<u64, ClientRead>,
    one_frames: BTreeMap<u64, OneFrame>,
    drops: BTreeMap<u64, DropRelay>,
    recalls: BTreeMap<u64, Recalling>,
    claims: BTreeMap<u64, Claim>,
    held: u64,
    events_in: UnboundedSender<SessionEvent>,
    events: UnboundedReceiver<SessionEvent>,
    out: VecDeque<(u64, Vec<u8>)>,
    /// What it holds of the endpoint's open requests: one for each request relayed and not yet answered, and each frame queued.
    counted: Vec<OpenFrame>,
}

impl<A: Authorizer + Clone> ClientSession<A> {
    /// A session on `core`, filling `credentials` for the connection's machine, the connection's peer `peer`.
    pub fn new(core: EndpointCore<A>, credentials: AskedCredentials, peer: Option<IpAddr>) -> Self {
        let statements = StatementCache::new(core.limits().statements());
        let (events_in, events) = unbounded();
        Self {
            core,
            credentials,
            asked_name: None,
            peer,
            principal: 0,
            max_frame: MAX_FRAME_FLOOR as u32,
            statements,
            transactions: BTreeMap::new(),
            reads: BTreeMap::new(),
            one_frames: BTreeMap::new(),
            drops: BTreeMap::new(),
            recalls: BTreeMap::new(),
            claims: BTreeMap::new(),
            held: 0,
            events_in,
            events,
            out: VecDeque::new(),
            counted: Vec::new(),
        }
    }

    /// Counts in the endpoint's open requests, which a stop waits on, what this connection took and has not yet answered on the
    /// wire: each read, one-frame write, Drop and retried Commit, each transaction whose Commit it took, and each frame queued.
    /// A frame [`ultragraphdb_common_role::Session::next_push`] hands out stays counted until the next call, after its flush.
    fn recount(&mut self) {
        let open = self.reads.len()
            + self.one_frames.len()
            + self.drops.len()
            + self.recalls.len()
            + self
                .transactions
                .values()
                .filter(|txn| txn.commit_taken())
                .count()
            + self.out.len();
        self.counted.truncate(open);
        let more = open.saturating_sub(self.counted.len());
        let core = &self.core;
        self.counted
            .extend(std::iter::repeat_with(|| core.open().enter()).take(more));
    }

    fn cluster(&self) -> u64 {
        self.core.links().cluster()
    }

    fn sink(&self, key: RouteKey) -> RouteSink {
        RouteSink::new(self.events_in.clone(), key)
    }

    /// Queues `frame`, whole, on the client's `stream`.
    fn push(&mut self, stream: u64, frame: Vec<u8>) {
        self.out.push_back((stream, frame));
    }

    /// Queues `body` on the client's `stream`, as its last frame there.
    fn push_last<B: BodyBuilder>(&mut self, stream: u64, body: &B) {
        let header = Header::new(self.cluster(), B::COMMAND)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        match body.frame(header) {
            Ok(frame) => self.push(stream, frame),
            Err(error) => {
                tracing::error!(%error, stream, "a reply could not be framed");
            }
        }
    }

    /// Whether a frame ending `stream`, flagged `LAST`, waits to leave.
    fn ending(&self, stream: u64) -> bool {
        self.out.iter().any(|(on, frame)| {
            *on == stream
                && HeaderView::new(frame).is_ok_and(|header| header.flags() & FLAG_LAST != 0)
        })
    }

    /// Queues `error` on the client's `stream`, which it ends.
    fn push_error(&mut self, stream: u64, error: &ErrorReply) {
        self.push_last(stream, error);
    }

    /// The client's `stream`'s header of `command`, for a reply that does not end it.
    fn header(&self, stream: u64, command: Command) -> Header {
        Header::new(self.cluster(), command).with_stream(stream)
    }
}

/// `now`, in the shell: a transaction's limit and a read's deadline run on the frontend's own clock.
#[expect(
    clippy::disallowed_methods,
    reason = "a deadline in the shell, which no simulation replays"
)]
pub(crate) fn now() -> Instant {
    Instant::now()
}
