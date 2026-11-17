/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::{Duration, Instant};

use compio::runtime::spawn;
use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_wire::{
    Action, Bind, BindKind, BodyBuilder, BodyView, CheckKind, Command, ConditionalWriteView,
    ConflictKind, DirectWriteView, DropCommandView, DropKind, ErrorCode, ErrorReply, FrameView,
    Header, WriteChange, xxh3_64,
};

use super::client_session_bind::cross_partition;
use super::client_session_writes::{changes_destination, operation_destination};
use super::{ClientSession, DropRelay, OneFrame, now};
use crate::traits::authorizer::Authorizer;
use crate::types::counted_frame::CountedFrame;
use crate::types::destination::Destination;
use crate::types::link_event::LinkEvent;
use crate::types::route_key::RouteKey;
use crate::types::session_event::SessionEvent;
use crate::utils::term_rules::{check_change, check_operation, check_table};

/// Where a one-frame write's `principal` lies in its command header (`PROTOCOL.md` §6.6).
const PRINCIPAL_AT: usize = 56;

/// The bytes of a one-frame write's `session` and `request`, which place a write naming only new blank nodes.
const SESSION_AND_REQUEST: usize = 24;

/// What a one-frame write says of itself, read alike from either command.
struct OneFrameFacts {
    session: [u8; 16],
    request: u64,
    tenant: u64,
    /// When the session took its frame.
    began: Instant,
}

impl<A: Authorizer + Clone> ClientSession<A> {
    /// A DirectWrite or a ConditionalWrite: the gate, the tenant, its session and its terms, and then relayed to its
    /// partition with `principal` filled and every other byte unchanged.
    pub(super) async fn one_frame(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
        bytes: &[u8],
    ) -> Result<Answer, ErrorReply> {
        if view.command() == Command::DirectWrite {
            return self.direct_write(stream, view, bytes).await;
        }
        let write = ConditionalWriteView::from_client_frame(view)
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        let facts = OneFrameFacts {
            session: write.session(),
            request: write.request(),
            tenant: write.tenant(),
            began: now(),
        };
        self.check_one_frame(&facts).await?;
        let checked = (|| -> Result<Destination, ErrorReply> {
            check_table(write.terms())?;
            let changes: Vec<WriteChange> = (0..write.change_count())
                .filter_map(|index| write.change(index))
                .collect();
            for change in &changes {
                check_change(write.terms(), facts.tenant, change)?;
            }
            let operations: Vec<_> = (0..write.operation_count())
                .filter_map(|index| write.operation(index))
                .collect();
            for operation in &operations {
                check_operation(write.terms(), facts.tenant, operation)?;
            }
            let destination = changes_destination(write.terms(), facts.tenant, changes.into_iter());
            let placed: Vec<Destination> = operations
                .iter()
                .map(|operation| operation_destination(write.terms(), facts.tenant, operation))
                .collect();
            Ok(with_operations(destination, placed))
        })();
        self.routed(stream, &facts, checked, bytes)
    }

    async fn direct_write(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
        bytes: &[u8],
    ) -> Result<Answer, ErrorReply> {
        let write =
            DirectWriteView::from_client_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let facts = OneFrameFacts {
            session: write.session(),
            request: write.request(),
            tenant: write.tenant(),
            began: now(),
        };
        self.check_one_frame(&facts).await?;
        let checked = (|| -> Result<Destination, ErrorReply> {
            check_table(write.terms())?;
            let changes: Vec<WriteChange> = (0..write.len())
                .filter_map(|index| write.change(index))
                .collect();
            for change in &changes {
                check_change(write.terms(), facts.tenant, change)?;
            }
            Ok(changes_destination(
                write.terms(),
                facts.tenant,
                changes.into_iter(),
            ))
        })();
        self.routed(stream, &facts, checked, bytes)
    }

    /// A one-frame write whose terms were checked: routed, or its refusal, which frees its session.
    fn routed(
        &mut self,
        stream: u64,
        facts: &OneFrameFacts,
        checked: Result<Destination, ErrorReply>,
        bytes: &[u8],
    ) -> Result<Answer, ErrorReply> {
        match checked {
            Ok(destination) => self.route_one_frame(stream, facts, &destination, bytes),
            Err(refusal) => {
                let session = u128::from_be_bytes(facts.session);
                if session != 0 {
                    self.core.system().ended(session.to_be_bytes());
                }
                Err(refusal)
            }
        }
    }

    /// The gate, the tenant, and the session's owner check.
    async fn check_one_frame(&self, facts: &OneFrameFacts) -> Result<(), ErrorReply> {
        // A token ahead of the view goes with the write to its partition, which holds the commit until its close (`PROTOCOL.md`
        // §4.8).
        let _known = self.gate(Action::Write, facts.tenant).await?;
        let session = u128::from_be_bytes(facts.session);
        if session != 0 {
            self.core
                .system()
                .check(session.to_be_bytes(), self.principal, CheckKind::Marks)
                .await?;
        }
        Ok(())
    }

    /// Chooses the partition by `destination`: a spread refused with code 23 before any relay.
    fn route_one_frame(
        &mut self,
        stream: u64,
        facts: &OneFrameFacts,
        destination: &Destination,
        bytes: &[u8],
    ) -> Result<Answer, ErrorReply> {
        let session = u128::from_be_bytes(facts.session);
        let mut frame = bytes.to_vec();
        // A frame the framer took holds its header.
        drop(Header::stamp_command_bytes(
            &mut frame,
            PRINCIPAL_AT,
            &self.principal.to_le_bytes(),
        ));
        let limit = Duration::from_millis(u64::from(self.core.limits().transaction_ms()));
        let frame = self.counted(frame);
        let _none = self.one_frames.insert(
            stream,
            OneFrame {
                frame,
                tenant: facts.tenant,
                session,
                resent: false,
                forced: None,
                attempt: 0,
                route: None,
                limit: Some(now() + limit),
                began: facts.began,
                relayed: None,
            },
        );
        match self.place_one_frame(destination, facts) {
            Ok(Some(partition)) => self.send_one_frame(stream, partition),
            Ok(None) => {
                if let Destination::Claim(claim) = destination {
                    self.ask_claim(stream, claim);
                }
            }
            Err(refusal) => {
                self.one_frame_answered(stream);
                return Err(refusal);
            }
        }
        Ok(Answer::Open)
    }

    /// The partition of a one-frame write; `None` while a claim's is unknown.
    fn place_one_frame(
        &self,
        destination: &Destination,
        facts: &OneFrameFacts,
    ) -> Result<Option<u32>, ErrorReply> {
        let links = self.core.links();
        match destination {
            Destination::Named(_) => match self.owners(destination).as_slice() {
                [] => Err(self.unavailable()),
                [only] => Ok(Some(*only)),
                [lowest, other, ..] => Err(cross_partition(*lowest, *other)),
            },
            Destination::Claim(_) => Ok(None),
            Destination::InTurn if u128::from_be_bytes(facts.session) != 0 => {
                // A resend reaches the session table the first did.
                let owners = links.run_partitions(facts.tenant);
                let mut placed = [0_u8; SESSION_AND_REQUEST];
                let (session, request) = placed.split_at_mut(16);
                session.copy_from_slice(&facts.session);
                request.copy_from_slice(&facts.request.to_le_bytes());
                let at = xxh3_64(&placed) % owners.len().max(1) as u64; // lossless: a `usize` is at most 64 bits
                usize::try_from(at) // below `owners.len()`, a `usize`
                    .ok()
                    .and_then(|at| owners.get(at))
                    .copied()
                    .map(Some)
                    .ok_or_else(|| self.unavailable())
            }
            Destination::InTurn => links
                .next_in_turn(facts.tenant)
                .map(Some)
                .ok_or_else(|| self.unavailable()),
            Destination::Run => match links.run_partitions(facts.tenant).as_slice() {
                [] => Err(self.unavailable()),
                [only] => Ok(Some(*only)),
                [lowest, other, ..] => Err(cross_partition(*lowest, *other)),
            },
        }
    }

    /// Relays the one-frame write of `stream` on a new stream to `partition`.
    pub(super) fn send_one_frame(&mut self, stream: u64, partition: u32) {
        let Some(link) = self.core.links().partition(partition) else {
            let refusal = self.unavailable();
            self.one_frame_answered(stream);
            self.push_error(stream, &refusal);
            return;
        };
        let Some(one) = self.one_frames.get_mut(&stream) else {
            return;
        };
        one.attempt += 1;
        let attempt = one.attempt;
        let mut frame = one.frame.bytes().to_vec();
        drop(Header::stamp_stream(&mut frame, link.next_stream()));
        let sink = self.sink(RouteKey::OneFrame { stream, attempt });
        // Counted as the frame it keeps for a resend.
        let opened = link.open(CountedFrame::uncounted(frame), sink);
        if let Some(sent) = self.one_frames.get_mut(&stream) {
            sent.route = Some((link, opened));
            // The first relay's time stays: a resend after a refusal naming another owner is the partition's phase.
            let _first: &mut Instant = sent.relayed.get_or_insert_with(now);
        }
    }

    /// What a one-frame write's stream said: its reply, or a refusal naming another owner, resent there once.
    pub(super) fn one_frame_event(&mut self, stream: u64, attempt: u32, event: LinkEvent) {
        if self
            .one_frames
            .get(&stream)
            .is_none_or(|one| one.attempt != attempt)
        {
            return;
        }
        let mut frame = match event {
            // The Commit answering it, relayed as it came.
            LinkEvent::Frame(frame) => frame,
            LinkEvent::Refused(refusal) => {
                self.one_frame_refused(stream, &refusal);
                return;
            }
            // A one-frame write is answered by a Commit or an Error; the link closes on any other answer, which stands as its
            // close.
            LinkEvent::Closed
            | LinkEvent::Bound(_)
            | LinkEvent::Held
            | LinkEvent::Registered(_) => {
                let refusal = self.unavailable();
                self.one_frame_answered(stream);
                self.push_error(stream, &refusal);
                return;
            }
        };
        if let Some(one) = self.one_frames.get(&stream)
            && let Some(relayed) = one.relayed
        {
            self.core.phases().record(one.began, relayed, now());
        }
        self.one_frame_answered(stream);
        // A frame the framer took holds its header.
        drop(Header::stamp_stream(&mut frame, stream));
        self.push(stream, frame);
    }

    /// The partition refused the one-frame write of `stream` with `refusal`: naming another owner, resent there once; otherwise
    /// the client's.
    fn one_frame_refused(&mut self, stream: u64, refusal: &ErrorReply) {
        if refusal.code() == ErrorCode::WrongPartition
            && refusal.detail() >> 32 != 0
            && let Some(one) = self.one_frames.get_mut(&stream)
            && !one.resent
        {
            one.resent = true;
            one.forced = Some((refusal.detail() >> 32) as u32); // a `u64`'s high half, lossless
            one.route = None;
            self.refresh_map(stream);
            return;
        }
        self.one_frame_answered(stream);
        self.push_error(stream, refusal);
    }

    /// The map a refusal asked for is here: the write goes to the owner it named.
    pub(super) fn resend_one_frame(&mut self, stream: u64) {
        if let Some(partition) = self.one_frames.get(&stream).and_then(|one| one.forced) {
            self.send_one_frame(stream, partition);
        }
    }

    /// The one-frame write of `stream` passed `transaction_ms`: one none of which has left is taken back and answered code 4, having
    /// reached no partition. One that has left stays with its partition, which answers it, and the frame it kept for a resend goes,
    /// so what stands on a role's answer stops holding the connection's reads (`PROTOCOL.md` §11.12, *Frames on their way*).
    pub(super) fn one_frame_past_limit(&mut self, stream: u64) {
        let Some(one) = self.one_frames.get_mut(&stream) else {
            return;
        };
        one.limit = None;
        let unsent = one
            .route
            .as_ref()
            .is_none_or(|(link, relayed)| link.take_back(*relayed));
        if !unsent {
            one.frame = CountedFrame::uncounted(Vec::new());
            one.resent = true;
            return;
        }
        let refusal = self.past_limit(stream);
        self.one_frame_answered(stream);
        self.push_error(stream, &refusal);
    }

    /// The one-frame write of `stream` is over: its session may commit again.
    pub(super) fn one_frame_answered(&mut self, stream: u64) {
        if let Some(one) = self.one_frames.remove(&stream)
            && one.session != 0
        {
            self.core.system().ended(one.session.to_be_bytes());
        }
    }

    /// A Drop: the gate, the tenant, its catalog version, its session; refused with code 23 when several partitions hold the
    /// tenant's run, and otherwise bound to the one that does (`PROTOCOL.md` §11.12, *Drops*). A Drop of a tenant's data or
    /// of everything in it naming another version than the one in effect at the core's open epoch is refused before any
    /// partition sees it (§6.5): an early refusal, refusing nothing the partition would take at that epoch, whose check at
    /// its round's epoch decides.
    pub(super) async fn drop_request(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
        bytes: &[u8],
    ) -> Result<Answer, ErrorReply> {
        let asked =
            DropCommandView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let tenant = asked.tenant();
        let _facts = self.gate(Action::Drop, tenant).await?;
        let session = u128::from_be_bytes(asked.session());
        let partitions = self.core.links().run_partitions(tenant);
        let partition = match partitions.as_slice() {
            [] => return Err(self.unavailable()),
            [only] => *only,
            [lowest, other, ..] => return Err(cross_partition(*lowest, *other)),
        };
        let names_version = matches!(asked.kind(), DropKind::TenantData | DropKind::Tenant);
        let in_effect = self.core.system().view().in_effect(tenant);
        if names_version
            && let Some(in_effect) = in_effect
            && asked.catalog() != in_effect
        {
            return Err(stale_drop(asked.catalog(), in_effect));
        }
        let Some(link) = self.core.links().partition(partition) else {
            return Err(self.unavailable());
        };
        if session != 0 {
            self.core
                .system()
                .check(session.to_be_bytes(), self.principal, CheckKind::Marks)
                .await?;
        }
        // A Drop holds no transaction, and Bind's rule only asks its identity be not all zero.
        let identity = [1; 16];
        let body = Bind::new(
            partition,
            BindKind::Drop,
            tenant,
            0,
            self.principal,
            identity,
        );
        let header = link.header(Command::Bind, link.next_stream());
        let frame = body
            .frame(header)
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        let sink = self.sink(RouteKey::Drop { stream });
        let opened = link.open(self.counted(frame), sink);
        // A Drop of a graph or of a tenant's data calls for no catalog version (`PROTOCOL.md` §13.27).
        let step = match asked.kind() {
            DropKind::Predicate => asked
                .target()
                .map(|predicate| (DropKind::Predicate, Some(predicate.to_string()))),
            DropKind::Tenant => Some((DropKind::Tenant, None)),
            DropKind::Graph | DropKind::TenantData => None,
        };
        let _none = self.drops.insert(
            stream,
            DropRelay {
                frame: bytes.to_vec(),
                session,
                tenant,
                step,
                reply: None,
                link,
                stream: opened,
                sent: false,
            },
        );
        Ok(Answer::Open)
    }

    /// What a Drop's Bind stream said: the Bind's answer sends the Drop, and its reply awaits the catalog step.
    pub(super) fn drop_event(&mut self, stream: u64, event: LinkEvent) {
        match event {
            LinkEvent::Bound(_) => {
                if let Some(dropping) = self.drops.get_mut(&stream) {
                    let mut relayed = dropping.frame.clone();
                    drop(Header::stamp_stream(&mut relayed, dropping.stream));
                    dropping
                        .link
                        .send(dropping.stream, CountedFrame::uncounted(relayed));
                    dropping.sent = true;
                }
            }
            // The Drop's reply, relayed as it came.
            LinkEvent::Frame(frame) => self.drop_replied(stream, frame),
            LinkEvent::Refused(refusal) => {
                self.drop_answered(stream);
                self.push_error(stream, &refusal);
            }
            // A Drop's stream is answered by a Bind, the Drop's reply or an Error; the link closes on any other answer, which
            // stands as its close.
            LinkEvent::Closed | LinkEvent::Held | LinkEvent::Registered(_) => {
                let refusal = self.unavailable();
                self.drop_answered(stream);
                self.push_error(stream, &refusal);
            }
        }
    }

    /// A Drop's reply, stored or new: the catalog version it calls for commits as pending before it is relayed, and a version that
    /// would change nothing is not built (`PROTOCOL.md` §11.13).
    fn drop_replied(&mut self, stream: u64, frame: Vec<u8>) {
        let Some(dropping) = self.drops.get_mut(&stream) else {
            return;
        };
        dropping.reply = Some(frame);
        let Some(step) = dropping.step.clone() else {
            self.drop_catalog(stream, Ok(()));
            return;
        };
        let (tenant, principal) = (dropping.tenant, self.principal);
        let system = self.core.system().clone();
        let events = self.events_in.clone();
        spawn(async move {
            let (kind, target) = step;
            let outcome = system
                .drop_catalog(tenant, principal, kind, target)
                .await
                .map(drop);
            // A session gone has no one to tell; the resend builds the version.
            drop(events.unbounded_send(SessionEvent::DropCatalog { stream, outcome }));
        })
        .detach();
    }

    /// The catalog step is durable, or was refused: the reply goes to the client.
    pub(super) fn drop_catalog(&mut self, stream: u64, outcome: Result<(), ErrorReply>) {
        let reply = self
            .drops
            .get_mut(&stream)
            .and_then(|dropping| dropping.reply.take());
        self.drop_answered(stream);
        match (outcome, reply) {
            (Ok(()), Some(mut frame)) => {
                drop(Header::stamp_stream(&mut frame, stream));
                self.push(stream, frame);
            }
            (Err(refusal), _) => self.push_error(stream, &refusal),
            (Ok(()), None) => {}
        }
    }

    /// The Drop of `stream` is over: its session may commit again.
    fn drop_answered(&mut self, stream: u64) {
        if let Some(dropping) = self.drops.remove(&stream)
            && dropping.session != 0
        {
            self.core.system().ended(dropping.session.to_be_bytes());
        }
    }
}

/// A Drop naming catalog version `named` where `in_effect` is in effect at the core's open epoch: code 2, kind 5, naming both
/// (`PROTOCOL.md` §6.5).
fn stale_drop(named: u64, in_effect: u64) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Conflict,
        format!("the Drop names catalog version {named}, and {in_effect} is in effect"),
    )
    .with_detail(ConflictKind::Catalog.number())
}

/// Where a ConditionalWrite goes, from its changes' `destination` and each operation's `placed`: every subject named by a change or
/// an operation, so a spread over partitions is refused before any relay (`PROTOCOL.md` §6.6); with none named, the first claim's
/// partition; else the changes', or the first operation's turn.
fn with_operations(destination: Destination, placed: Vec<Destination>) -> Destination {
    let mut buckets = match &destination {
        Destination::Named(buckets) => buckets.clone(),
        Destination::Claim(_) | Destination::InTurn | Destination::Run => Vec::new(),
    };
    let mut claim = None;
    let mut in_turn = false;
    for operation in placed {
        match operation {
            Destination::Named(named) => buckets.extend(named),
            Destination::Claim(first) => claim = claim.or(Some(first)),
            Destination::InTurn => in_turn = true,
            Destination::Run => {}
        }
    }
    if !buckets.is_empty() {
        return Destination::Named(buckets);
    }
    match (claim, destination) {
        (Some(first), _) => Destination::Claim(first),
        (None, Destination::Run) if in_turn => Destination::InTurn,
        (None, other) => other,
    }
}
