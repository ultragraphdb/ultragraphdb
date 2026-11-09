/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use futures::StreamExt;
use ultragraphdb_common_role::{Answer, ParkedBytes, Push, Session};
use ultragraphdb_protocol_scram::{MECHANISM, ScramServer, ScramVerifier};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};

use super::{ClientSession, now};
use crate::traits::authorizer::Authorizer;
use crate::types::bind_state::BindState;
use crate::types::commit_hold::CommitHold;
use crate::types::link_event::LinkEvent;
use crate::types::route_key::RouteKey;
use crate::types::session_event::SessionEvent;

impl<A: Authorizer + Clone> Session for ClientSession<A> {
    /// Asks the owner for the name's verifier by message, first waiting out the failures of its address and its name. A name
    /// refused holds no verifier here, so its exchange fails as any other does and says nothing of why.
    /// A future dropped at its delay or at the owner's answer leaves the credentials unanswered, which fails the
    /// exchange.
    async fn authenticating(&mut self, first: &[u8]) {
        let name = ScramServer::name_in(first).ok();
        let throttle = self.core.throttle();
        let delay = self
            .peer
            .map_or(Duration::ZERO, |peer| throttle.delay(peer))
            .max(
                name.as_deref()
                    .map_or(Duration::ZERO, |name| throttle.name_delay(name)),
            );
        if !delay.is_zero() {
            compio::time::sleep(delay).await;
        }
        // A malformed message asks for nothing, and the machine refuses it.
        if let Some(name) = name {
            let held = self.core.owner().verifier(name.clone()).await;
            let refused = self.core.throttle().name_refused(&name, now());
            // A refused name keeps its salt and iterations and fails its proof, as an unknown name keeps its own (SECURITY.md,
            // *Failures*): no key hashes to all zeros.
            let held = held.map(|(principal, verifier)| {
                if !refused {
                    return (principal, verifier);
                }
                let failing = ScramVerifier::new(
                    verifier.salt().to_vec(),
                    verifier.iterations(),
                    [0; 32],
                    [0; 32],
                );
                (principal, failing)
            });
            self.credentials.answered(name.clone(), held);
            self.asked_name = Some(name);
        }
    }

    fn authenticated(&mut self, principal: u64) {
        self.principal = principal;
        let throttle = self.core.throttle();
        if let Some(peer) = self.peer {
            throttle.succeeded(peer);
        }
        if let Some(name) = &self.asked_name {
            throttle.name_succeeded(name);
        }
        self.audit_authentication("accepted");
    }

    fn authentication_refused(&mut self) {
        let throttle = self.core.throttle();
        if let Some(peer) = self.peer {
            throttle.failed(peer);
        }
        if let Some(name) = &self.asked_name {
            throttle.name_failed(name, now());
        }
        self.audit_authentication("refused");
    }

    fn welcomed(&mut self, max_frame: u32) {
        self.max_frame = max_frame;
    }

    fn parked(&self) -> Option<ParkedBytes> {
        Some(self.on_way.clone())
    }

    /// A dropped future leaves what [`ClientSession`] says.
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let answer = match self.request(stream, frame).await {
            Ok(answer) => answer,
            Err(refusal) => Answer::Refuse(refusal),
        };
        self.recount();
        answer
    }

    /// Takes what reaches the session from outside, one event at a time, and its timers; a wait dropped for a request that arrived
    /// first loses nothing, since an event is taken whole.
    async fn next_push(&mut self) -> (u64, Push) {
        loop {
            self.recount();
            if let Some(pushed) = self.out.pop_front() {
                return (pushed.0, Push::Frame(pushed.1));
            }
            let wait = self
                .next_deadline()
                .map(|deadline| deadline.saturating_duration_since(now()));
            let event = match wait {
                Some(wait) => compio::time::timeout(wait, self.events.next())
                    .await
                    .ok()
                    .flatten(),
                None => self.events.next().await,
            };
            match event {
                Some(event) => self.event(event),
                None => self.timers(now()),
            }
        }
    }

    fn cancelled(&mut self, stream: u64) {
        if self.transactions.contains_key(&stream) {
            self.abort(stream);
        } else if self.reads.contains_key(&stream) {
            self.read_cancelled(stream);
        } else if let Some(one) = self.one_frames.remove(&stream) {
            // Whether it committed is its resend's to learn: a Cancel never undoes one, and its session stays in flight until
            // its partition answers it (`PROTOCOL.md` §11.4, §11.5).
            match &one.route {
                Some((link, relayed)) if one.session != 0 => {
                    link.hold(
                        *relayed,
                        CommitHold::new(self.core.owner_weak(), one.session),
                    );
                }
                Some((link, relayed)) => {
                    let _unopened = link.forget(*relayed);
                }
                None if one.session != 0 => self.core.owner().commit_ended(one.session),
                None => {}
            }
        }
    }

    fn credited(&mut self, stream: u64, frames: u32) {
        self.read_credited(stream, frames);
    }
}

impl<A: Authorizer + Clone> ClientSession<A> {
    pub(super) fn event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Role { key, event } => self.role_event(key, event),
            SessionEvent::Map { stream, map } => self.map_refreshed(stream, map),
            SessionEvent::Declared { stream, outcome } => self.declared(stream, outcome),
            SessionEvent::DropCatalog { stream, outcome } => self.drop_catalog(stream, outcome),
            SessionEvent::SessionHeld { stream, held } => self.recall_session(stream, held),
        }
    }

    fn role_event(&mut self, key: RouteKey, event: LinkEvent) {
        match key {
            RouteKey::Bind {
                transaction,
                attempt,
            } => self.bind_event(transaction, attempt, event),
            RouteKey::Read { read, attempt } => self.read_event(read, attempt, event),
            RouteKey::OneFrame { stream, attempt } => self.one_frame_event(stream, attempt, event),
            RouteKey::Drop { stream } => self.drop_event(stream, event),
            RouteKey::Recall { stream, partition } => self.recall_event(stream, partition, event),
            RouteKey::Claim { stream } => self.claim_event(stream, event),
        }
    }

    /// A role connection closing under `stream`'s request, a read's next compute process apart: code 17 (`PROTOCOL.md` §11.12).
    pub(super) fn unavailable(&self) -> ErrorReply {
        ErrorReply::new(
            ErrorCode::Unavailable,
            "the role connection closed under the request, or could not be opened",
        )
    }
}

impl<A> Drop for ClientSession<A> {
    /// A connection that closes aborts its open transactions: their Bind streams and reads cancelled. Its commits already relayed
    /// proceed, each session held in flight until its partition's stream ends, so a full registry does not evict it first.
    fn drop(&mut self) {
        let owner = self.core.owner();
        for transaction in self.transactions.values() {
            let route = match &transaction.bind {
                BindState::Bound { link, stream, .. } | BindState::Binding { link, stream, .. } => {
                    Some((link, *stream))
                }
                BindState::None => None,
            };
            match (route, transaction.commit_out.is_some()) {
                (Some((link, stream)), true) if transaction.session_held != 0 => {
                    link.hold(
                        stream,
                        CommitHold::new(self.core.owner_weak(), transaction.session_held),
                    );
                    continue;
                }
                (Some((link, stream)), false) => link.cancel(stream),
                (Some(_) | None, _) => {}
            }
            if transaction.session_held != 0 {
                owner.commit_ended(transaction.session_held);
            }
        }
        for read in self.reads.values() {
            if let Some((link, stream)) = &read.route {
                link.cancel(*stream);
            }
        }
        for one in self.one_frames.values() {
            match &one.route {
                Some((link, stream)) if one.session != 0 => {
                    link.hold(
                        *stream,
                        CommitHold::new(self.core.owner_weak(), one.session),
                    );
                }
                Some((link, stream)) => {
                    let _unopened = link.forget(*stream);
                }
                None if one.session != 0 => owner.commit_ended(one.session),
                None => {}
            }
        }
        for dropping in self.drops.values() {
            if dropping.sent && dropping.session != 0 {
                dropping.link.hold(
                    dropping.stream,
                    CommitHold::new(self.core.owner_weak(), dropping.session),
                );
                continue;
            }
            // Its Bind stream is let go at the partition, as a transaction's is.
            dropping.link.cancel(dropping.stream);
            if dropping.session != 0 {
                owner.commit_ended(dropping.session);
            }
        }
        for recall in self.recalls.values() {
            owner.commit_ended(recall.session);
        }
    }
}

impl<A: Authorizer + Clone> ClientSession<A> {
    /// The audit event of an exchange's `outcome`, which names no credential (`SECURITY.md`).
    fn audit_authentication(&self, outcome: &str) {
        tracing::info!(
            target: "audit",
            event = "authentication",
            mechanism = MECHANISM,
            outcome,
            name = self.asked_name.as_deref().unwrap_or_default(),
            principal = self.principal,
            source = ?self.peer,
        );
    }
}
