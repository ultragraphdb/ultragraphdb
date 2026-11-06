/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::runtime::spawn;
use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BindView, BodyBuilder, BodyView, Command, ErrorCode, ErrorReply,
    ErrorReplyView, FrameView, Header, HeaderView, MAX_FRAME_SIZE,
};

use super::ClientSession;
use crate::traits::authorizer::Authorizer;
use crate::types::client_transaction::{BindState, Claim, Destination, Queued};
use crate::types::link_event::LinkEvent;
use crate::types::route_key::RouteKey;
use crate::types::session_event::SessionEvent;

/// What a transaction does next.
enum Step {
    /// Binds to this partition.
    Bind(u32),
    /// Learns where its first request's claim lies.
    Claim(Claim),
    /// Sends the read at the queue's front.
    Read,
    /// Sends the Write or Operation at the queue's front on its Bind stream.
    Write,
    /// Relays the Commit at the queue's front on its Bind stream.
    Commit,
    /// Answers the Commit at the queue's front itself: nothing bound it.
    EmptyCommit,
    /// Ends with this refusal.
    Refuse(ErrorReply),
}

/// Code 23's `detail`: the transaction's partition, or the lowest it reaches, then the lowest other it reaches.
pub(super) fn cross_partition(partition: u32, other: u32) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::CrossPartition,
        format!("the request reaches partitions {partition} and {other}"),
    )
    .with_detail(u64::from(partition) | (u64::from(other) << 32))
}

/// An Error frame's reply.
pub(super) fn error_of(cluster: u64, frame: &[u8]) -> Option<ErrorReply> {
    let view = FrameView::validate(frame, cluster, MAX_FRAME_SIZE).ok()?;
    ErrorReplyView::from_frame(&view).ok()?.to_body().ok()
}

/// The command `frame` holds, by its header.
pub(super) fn command_of(frame: &[u8]) -> Option<Command> {
    let header = HeaderView::new(frame).ok()?;
    Command::from_number(header.command()).ok()
}

impl<A: Authorizer + Clone> ClientSession<A> {
    /// Sends what the transaction `transaction` may send now, in the order its client sent it: a transaction's first request
    /// answered before any later one goes, a read only once earlier writes have answered, a write only once earlier reads have.
    pub(super) fn pump(&mut self, transaction: u64) {
        while let Some(step) = self.next_step(transaction) {
            match step {
                Step::Bind(partition) => {
                    self.send_bind(transaction, partition);
                    return;
                }
                Step::Claim(claim) => {
                    self.ask_claim(transaction, &claim);
                    return;
                }
                Step::Read => self.send_next_read(transaction),
                Step::Write => self.send_next_write(transaction),
                Step::Commit => self.relay_commit(transaction),
                Step::EmptyCommit => {
                    self.answer_empty_commit(transaction);
                    return;
                }
                Step::Refuse(refusal) => {
                    self.end_transaction(transaction, &refusal);
                    return;
                }
            }
        }
    }

    fn next_step(&self, transaction: u64) -> Option<Step> {
        let txn = self.transactions.get(&transaction)?;
        let waits = txn.awaiting_map
            || txn.awaiting_claim
            || txn.first_open
            || txn.write_out.is_some()
            || txn.commit_out.is_some()
            || txn.ended.is_some();
        if waits {
            return None;
        }
        match txn.queue.front()? {
            Queued::Read(read) => {
                if txn.read_only {
                    return Some(Step::Read);
                }
                let subject = self.reads.get(read)?.subject;
                let destination = match subject {
                    Some(bucket) => Destination::Named(vec![bucket]),
                    None => Destination::Run,
                };
                self.placed(transaction, &destination, Step::Read)
            }
            Queued::Write { destination, .. } => {
                if !txn.reads_out.is_empty() {
                    return None;
                }
                self.placed(transaction, destination, Step::Write)
            }
            Queued::Commit { .. } => match &txn.bind {
                BindState::None => Some(Step::EmptyCommit),
                BindState::Binding { .. } => None,
                BindState::Bound { .. } => Some(Step::Commit),
            },
        }
    }

    /// `then`, once the transaction is bound where `destination` lies: a bound transaction's request reaching another partition
    /// is refused by the map, and an unbound one's first binds it.
    fn placed(&self, transaction: u64, destination: &Destination, then: Step) -> Option<Step> {
        let txn = self.transactions.get(&transaction)?;
        match &txn.bind {
            BindState::Binding { .. } => None,
            BindState::Bound { partition, .. } => {
                let other = self
                    .owners(destination)
                    .into_iter()
                    .find(|p| p != partition);
                Some(match other {
                    Some(other) => Step::Refuse(cross_partition(*partition, other)),
                    None => then,
                })
            }
            BindState::None => Some(self.first_binding(transaction, destination)?),
        }
    }

    /// Where a transaction's first request binds it (`PROTOCOL.md` §11.12, *Writes*).
    fn first_binding(&self, transaction: u64, destination: &Destination) -> Option<Step> {
        let txn = self.transactions.get(&transaction)?;
        if let Some(partition) = txn.forced.or(txn.claimed) {
            return Some(Step::Bind(partition));
        }
        let links = self.core.links();
        Some(match destination {
            Destination::Named(_) => {
                let owners = self.owners(destination);
                match owners.as_slice() {
                    [] => Step::Refuse(self.unavailable()),
                    [only] => Step::Bind(*only),
                    [lowest, other, ..] => Step::Refuse(cross_partition(*lowest, *other)),
                }
            }
            Destination::InTurn => match links.next_in_turn(txn.tenant) {
                Some(partition) => Step::Bind(partition),
                None => Step::Refuse(self.unavailable()),
            },
            Destination::Claim(claim) => Step::Claim(claim.clone()),
            Destination::Run => match links.run_partitions(txn.tenant).as_slice() {
                [] => Step::Refuse(self.unavailable()),
                [only] => Step::Bind(*only),
                [lowest, other, ..] => Step::Refuse(cross_partition(*lowest, *other)),
            },
        })
    }

    /// The partitions `destination` names by the core's map, ascending.
    pub(super) fn owners(&self, destination: &Destination) -> Vec<u32> {
        let Destination::Named(buckets) = destination else {
            return Vec::new();
        };
        let links = self.core.links();
        let mut owners: Vec<u32> = buckets
            .iter()
            .filter_map(|bucket| links.owner_of(*bucket))
            .collect();
        owners.sort_unstable();
        owners.dedup();
        owners
    }

    fn send_bind(&mut self, transaction: u64, partition: u32) {
        let Some(link) = self.core.links().partition(partition) else {
            let refusal = self.unavailable();
            self.end_transaction(transaction, &refusal);
            return;
        };
        let principal = self.principal;
        let Some(txn) = self.transactions.get_mut(&transaction) else {
            return;
        };
        txn.attempt += 1;
        let attempt = txn.attempt;
        let body = Bind::new(
            partition,
            BindKind::Transaction,
            txn.tenant,
            txn.epoch,
            principal,
            txn.identity,
        );
        let key = RouteKey::Bind {
            transaction,
            attempt,
        };
        let header = link.header(Command::Bind, link.next_stream());
        let Ok(frame) = body.frame(header) else {
            return;
        };
        let sink = self.sink(key);
        let stream = link.open(frame, sink);
        if let Some(binding) = self.transactions.get_mut(&transaction) {
            binding.bind = BindState::Binding {
                link,
                stream,
                partition,
            };
        }
    }

    /// What a transaction's Bind stream said.
    pub(super) fn bind_event(&mut self, transaction: u64, attempt: u32, event: LinkEvent) {
        let current = self
            .transactions
            .get(&transaction)
            .is_some_and(|txn| txn.attempt == attempt);
        if !current {
            return;
        }
        let frame = match event {
            LinkEvent::Frame(frame) => frame,
            LinkEvent::Closed => {
                let refusal = self.unavailable();
                self.stream_ended(transaction);
                self.end_transaction(transaction, &refusal);
                return;
            }
        };
        let command = command_of(&frame);
        if command == Some(Command::Bind) {
            self.bound(transaction, &frame);
        } else if command == Some(Command::Held) {
            if let Some(txn) = self.transactions.get_mut(&transaction) {
                txn.write_out = None;
                txn.first_open = false;
            }
            self.pump(transaction);
        } else if command == Some(Command::Commit) {
            self.commit_reply(transaction, frame);
        } else if command == Some(Command::Error) {
            self.stream_ended(transaction);
            self.bind_refused(transaction, &frame);
        }
    }

    /// The Bind's answer: the version the transaction reads at.
    fn bound(&mut self, transaction: u64, frame: &[u8]) {
        let cluster = self.cluster();
        let read_version = FrameView::validate(frame, cluster, MAX_FRAME_SIZE)
            .ok()
            .and_then(|view| {
                BindView::from_frame(&view)
                    .ok()
                    .map(|bind| bind.read_version())
            })
            .unwrap_or(0);
        if let Some(txn) = self.transactions.get_mut(&transaction)
            && let BindState::Binding {
                link,
                stream,
                partition,
            } = std::mem::replace(&mut txn.bind, BindState::None)
        {
            txn.bind = BindState::Bound {
                link,
                stream,
                partition,
                read_version,
            };
        }
        self.pump(transaction);
    }

    /// The Bind stream ended with its Error, so nothing is left to cancel there.
    pub(super) fn stream_ended(&mut self, transaction: u64) {
        if let Some(txn) = self.transactions.get_mut(&transaction) {
            txn.bind = BindState::None;
        }
    }

    /// A partition's Error on the Bind stream. A first request refused naming another owner is resent there once, what followed
    /// it kept; anything else is the client's.
    fn bind_refused(&mut self, transaction: u64, frame: &[u8]) {
        let Some(refusal) = error_of(self.cluster(), frame) else {
            let refusal = self.unavailable();
            self.end_transaction(transaction, &refusal);
            return;
        };
        let resend = self.transactions.get(&transaction).is_some_and(|txn| {
            refusal.code() == ErrorCode::WrongPartition && txn.first_open && !txn.resent
        });
        if !resend {
            self.end_transaction(transaction, &refusal);
            return;
        }
        let owner = (refusal.detail() >> 32) as u32;
        if let Some(txn) = self.transactions.get_mut(&transaction) {
            txn.resent = true;
            txn.forced = Some(owner);
            txn.first_open = false;
            txn.first_sent = false;
            if let Some(first) = txn.write_out.take() {
                // Its destination is the owner the refusal named, which `forced` holds.
                txn.queue.push_front(Queued::Write {
                    frame: first,
                    destination: Destination::InTurn,
                    behind: 0,
                });
            }
        }
        self.refresh_map(transaction);
    }

    /// Asks the owner for the placement map; `stream`'s request waits for it.
    pub(super) fn refresh_map(&mut self, stream: u64) {
        if let Some(txn) = self.transactions.get_mut(&stream) {
            txn.awaiting_map = true;
        }
        let owner = self.core.owner().clone();
        let events = self.events_in.clone();
        spawn(async move {
            let map = owner.map().await;
            // A session gone has no one to tell.
            drop(events.unbounded_send(SessionEvent::Map { stream, map }));
        })
        .detach();
    }

    /// The map a refusal asked for is here.
    pub(super) fn map_refreshed(&mut self, stream: u64, map: Option<PlacementMap>) {
        if let Some(map) = map {
            self.core.links().set_map(map);
        }
        if self.one_frames.contains_key(&stream) {
            self.resend_one_frame(stream);
            return;
        }
        if let Some(txn) = self.transactions.get_mut(&stream) {
            txn.awaiting_map = false;
        }
        self.after_refresh(stream);
        self.pump(stream);
    }

    /// Relays the Commit at the queue's front; it is no longer timed.
    fn relay_commit(&mut self, transaction: u64) {
        let Some(txn) = self.transactions.get_mut(&transaction) else {
            return;
        };
        let Some(Queued::Commit { frame, session }) = txn.queue.pop_front() else {
            return;
        };
        txn.commit_out = Some(session);
        if let BindState::Bound { link, stream, .. } = &txn.bind {
            let mut frame = frame;
            drop(Header::stamp_stream(&mut frame, *stream));
            link.send(frame);
        }
    }

    /// Ends the transaction `transaction` with `refusal`: its Bind stream cancelled, each read still open or held answered with it,
    /// then its stream, so a client reading the first learns of each.
    pub(super) fn end_transaction(&mut self, transaction: u64, refusal: &ErrorReply) {
        let Some(txn) = self.transactions.remove(&transaction) else {
            return;
        };
        match &txn.bind {
            BindState::Binding { link, stream, .. } | BindState::Bound { link, stream, .. } => {
                link.cancel(*stream);
            }
            BindState::None => {}
        }
        self.release_session(&txn);
        let behind: u64 = txn
            .queue
            .iter()
            .map(|queued| match queued {
                Queued::Write { behind, .. } => *behind,
                Queued::Read(_) | Queued::Commit { .. } => 0,
            })
            .sum();
        self.held = self.held.saturating_sub(behind);
        let reads: Vec<u64> = self
            .reads
            .iter()
            .filter(|(_, read)| read.transaction == transaction)
            .map(|(read, _)| *read)
            .collect();
        for read in reads {
            if let Some(ended) = self.reads.remove(&read) {
                if let Some((link, stream)) = &ended.route {
                    link.cancel(*stream);
                }
                self.push_error(read, refusal);
            }
        }
        self.push_error(transaction, refusal);
    }
}
