/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::pin::pin;

use futures::StreamExt;
use futures::future::{Either, join, select};
use ultragraphdb_common_role::{Answer, ParkedBytes, Push, Session};
use ultragraphdb_protocol_scram::{MECHANISM, system_nonce};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};

use super::{ClientSession, now};
use crate::traits::authorizer::Authorizer;
use crate::types::bind_state::BindState;
use crate::types::commit_hold::CommitHold;
use crate::types::link_event::LinkEvent;
use crate::types::read_executor::ReadExecutor;
use crate::types::route_key::RouteKey;
use crate::types::session_event::SessionEvent;

impl<A: Authorizer + Clone> Session for ClientSession<A> {
    /// Relays the exchange's step `step` to the system group (design D8): the client-first-message's name in a Challenge, asked
    /// beside the throttle's delay for the source, which it then waits; the client-final-message's AuthMessage and proof in a
    /// Verify. The answer is prepared for the connection's machine. A step the system group cannot be asked for ends the
    /// connection with code 17, counting no failure (`PROTOCOL.md` §11.12, *While the system group cannot be reached*).
    /// A future dropped at an answer or at the delay leaves the step unprepared, which fails the exchange.
    async fn authenticating_step(&mut self, step: u32, message: &[u8]) -> Result<(), ErrorReply> {
        if step == 0 {
            let Ok(nonce) = system_nonce() else {
                self.relay.refused();
                return Ok(());
            };
            let Ok(name) = self.relay.first(message, &nonce) else {
                self.relay.refused();
                return Ok(());
            };
            self.asked_name = Some(name.clone());
            let (asked, delay) = join(
                self.core.system().challenge(&name),
                self.core.logins().delay(self.peer),
            )
            .await;
            match asked {
                Ok((iterations, salt)) => {
                    if !delay.is_zero() {
                        compio::time::sleep(delay).await;
                    }
                    self.relay.challenged(&salt, iterations);
                }
                Err(refusal) if refusal.code() == ErrorCode::Unavailable => return Err(refusal),
                Err(_) => self.relay.refused(),
            }
            return Ok(());
        }
        let (Ok((auth_message, proof)), Some(name)) = (self.relay.last(message), self.relay.name())
        else {
            self.relay.refused();
            return Ok(());
        };
        match self
            .core
            .system()
            .verify(&name, &auth_message, &proof)
            .await
        {
            Ok(Some((principal, signature))) => self.relay.verified(principal, &signature),
            Ok(None) => self.relay.refused(),
            Err(refusal) if refusal.code() == ErrorCode::Unavailable => return Err(refusal),
            Err(_) => self.relay.refused(),
        }
        Ok(())
    }

    fn authenticated(&mut self, principal: u64) {
        self.principal = principal;
        if let Some(peer) = self.peer {
            self.core.logins().outcome(peer, true);
        }
        self.audit_authentication("accepted");
    }

    fn authentication_refused(&mut self) {
        if let Some(peer) = self.peer {
            self.core.logins().outcome(peer, false);
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
        // An event loop: it ends with a push, or when the connection drops the wait (Ultra rule 2).
        loop {
            self.recount();
            if let Some(pushed) = self.out.pop_front() {
                return (pushed.0, Push::Frame(pushed.1));
            }
            let wait = self
                .next_deadline()
                .map(|deadline| deadline.saturating_duration_since(now()));
            let woke = {
                let events = pin!(self.events.next());
                let reads = pin!(next_read(self.reads_run.as_mut()));
                let either = async {
                    match select(events, reads).await {
                        Either::Left((event, _)) => Woke::Event(event),
                        Either::Right(((read, frame), _)) => Woke::Read(read, frame),
                    }
                };
                match wait {
                    Some(wait) => compio::time::timeout(wait, either)
                        .await
                        .unwrap_or(Woke::Event(None)),
                    None => either.await,
                }
            };
            match woke {
                Woke::Event(Some(event)) => self.event(event),
                Woke::Event(None) => self.timers(now()),
                Woke::Read(read, frame) => self.read_frame(read, frame),
            }
        }
    }

    fn cancelled(&mut self, stream: u64) {
        if self.describes.remove(&stream).is_some() {
            return;
        }
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
                        CommitHold::new(self.core.system().clone(), one.session),
                    );
                }
                Some((link, relayed)) => {
                    let _unopened = link.forget(*relayed);
                }
                None if one.session != 0 => self.core.system().ended(one.session.to_be_bytes()),
                None => {}
            }
        }
    }

    /// The Result frame last sent on `stream` has left with credit to spare: a read's next batch may be built.
    fn ready(&mut self, stream: u64) {
        if self.reads.contains_key(&stream)
            && let Some(executor) = self.reads_run.as_mut()
        {
            executor.ready(stream);
        }
    }

    fn credited(&mut self, stream: u64, frames: u32) {
        if self.describes.contains_key(&stream) {
            self.describe_credited(stream, frames);
            return;
        }
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
            SessionEvent::SecondCheck { stream, outcome } => self.recall_session(stream, outcome),
        }
    }

    fn role_event(&mut self, key: RouteKey, event: LinkEvent) {
        match key {
            RouteKey::Bind {
                transaction,
                attempt,
            } => self.bind_event(transaction, attempt, event),
            RouteKey::OneFrame { stream, attempt } => self.one_frame_event(stream, attempt, event),
            RouteKey::Drop { stream } => self.drop_event(stream, event),
            RouteKey::Recall { stream, partition } => self.recall_event(stream, partition, event),
            RouteKey::Claim { stream } => self.claim_event(stream, &event),
        }
    }

    /// A role connection closing under `stream`'s request: code 17 (`PROTOCOL.md` §11.12).
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
        let owner = self.core.system();
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
                        CommitHold::new(self.core.system().clone(), transaction.session_held),
                    );
                    continue;
                }
                (Some((link, stream)), false) => link.cancel(stream),
                (Some(_) | None, _) => {}
            }
            if transaction.session_held != 0 {
                owner.ended(transaction.session_held.to_be_bytes());
            }
        }
        // The reads end with the executor, which cancels each one's Scans as it drops.
        for one in self.one_frames.values() {
            match &one.route {
                Some((link, stream)) if one.session != 0 => {
                    link.hold(
                        *stream,
                        CommitHold::new(self.core.system().clone(), one.session),
                    );
                }
                Some((link, stream)) => {
                    let _unopened = link.forget(*stream);
                }
                None if one.session != 0 => owner.ended(one.session.to_be_bytes()),
                None => {}
            }
        }
        for dropping in self.drops.values() {
            if dropping.sent && dropping.session != 0 {
                dropping.link.hold(
                    dropping.stream,
                    CommitHold::new(self.core.system().clone(), dropping.session),
                );
                continue;
            }
            // Its Bind stream is let go at the partition, as a transaction's is.
            dropping.link.cancel(dropping.stream);
            if dropping.session != 0 {
                owner.ended(dropping.session.to_be_bytes());
            }
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

/// What woke a session waiting for something to send.
enum Woke {
    /// An event from outside the connection; `None` for a timer due.
    Event(Option<SessionEvent>),
    /// A frame of a read the executor runs, and the read's stream.
    Read(u64, Vec<u8>),
}

/// The next frame of a read `executor` runs; pending for a core executing no read.
/// A future dropped at its wait leaves the frames to come to the next call.
async fn next_read(executor: Option<&mut ReadExecutor>) -> (u64, Vec<u8>) {
    match executor {
        Some(executor) => executor.next().await,
        None => std::future::pending().await,
    }
}
