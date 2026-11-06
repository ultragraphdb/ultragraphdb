/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::StreamExt;
use oxrdf::{Literal, NamedNode};
use ultragraphdb_common_role::Answer;
use ultragraphdb_core_ids::{Bucket, Tenant};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, CommitOperation, Declared, Direction, ErrorCode, ErrorReply,
    FrameView, Header, MAX_FRAME_SIZE, OperationView, Reference, ReferenceView, RequestTermView,
    StatementTarget, TermTableView, WriteChange, WriteView,
};

use super::client_session_bind::command_of;
use super::{ClientSession, now};
use crate::traits::authorizer::Authorizer;
use crate::types::client_transaction::{BindState, Claim, Destination, Queued};
use crate::types::link_event::LinkEvent;
use crate::types::route_key::RouteKey;
use crate::utils::iri_place::{IriPlace, iri_place};
use crate::utils::term_rules::{check_change, check_operation, check_table};

/// A Write's changes refused because a transaction's declarations hold nothing else: code 7, `detail` `index`.
fn declaring_refused(index: u64, why: &str) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Refused,
        format!("a transaction holding declarations holds nothing else: {why}"),
    )
    .with_detail(index)
}

/// Where a Write's or one-frame write's changes go: every subject it names by an IRI or a skolem IRI, else in turn; a request
/// naming no subject, the whole run's partition.
pub(super) fn changes_destination(
    table: &TermTableView<'_>,
    tenant: u64,
    changes: impl Iterator<Item = WriteChange>,
) -> Destination {
    let mut buckets = Vec::new();
    let mut blank = false;
    for change in changes {
        let subject = match &change {
            WriteChange::Insert { subject, .. }
            | WriteChange::InsertEdge { subject, .. }
            | WriteChange::Delete { subject, .. }
            | WriteChange::DeletePattern { subject, .. } => Some(*subject),
            WriteChange::DeleteStatement { reifier, .. } => Some(*reifier),
            WriteChange::SetProperty { statement, .. }
            | WriteChange::DeleteProperty { statement, .. } => Some(match statement {
                StatementTarget::Reifier { reifier, .. } => *reifier,
                StatementTarget::Triple { subject, .. } => *subject,
            }),
            WriteChange::Declare(_) => None,
        };
        note_subject(table, tenant, subject, &mut buckets, &mut blank);
    }
    if !buckets.is_empty() {
        return Destination::Named(buckets);
    }
    if blank {
        return Destination::InTurn;
    }
    Destination::Run
}

fn note_subject(
    table: &TermTableView<'_>,
    tenant: u64,
    subject: Option<u32>,
    buckets: &mut Vec<Bucket>,
    blank: &mut bool,
) {
    match subject.and_then(|subject| table.get(subject)) {
        Some(RequestTermView::Iri(iri)) => match iri_place(tenant, iri) {
            IriPlace::Ordinary(bucket) | IriPlace::Skolem(bucket) => buckets.push(bucket),
            IriPlace::Nobody | IriPlace::TooLong => {}
        },
        Some(RequestTermView::BlankNode(_)) => *blank = true,
        Some(
            RequestTermView::Literal { .. }
            | RequestTermView::LanguageLiteral { .. }
            | RequestTermView::Triple { .. },
        )
        | None => {}
    }
}

/// Where an operation goes: its subject's partition, or the claim of a value it claims.
pub(super) fn operation_destination(
    table: &TermTableView<'_>,
    tenant: u64,
    operation: &CommitOperation,
) -> Destination {
    match operation {
        CommitOperation::CompareAndSet { subject, .. }
        | CommitOperation::CounterAdd { subject, .. } => {
            let (mut buckets, mut blank) = (Vec::new(), false);
            note_subject(table, tenant, Some(*subject), &mut buckets, &mut blank);
            if buckets.is_empty() {
                Destination::InTurn
            } else {
                Destination::Named(buckets)
            }
        }
        CommitOperation::InsertIfAbsent {
            predicate, value, ..
        }
        | CommitOperation::UpsertAndLink {
            predicate, value, ..
        } => claim_of(table, *predicate, *value).map_or(Destination::InTurn, Destination::Claim),
    }
}

/// The claim of `value` of `predicate`: `None` for a value whose encoding names something by the store's ID, whose partition then
/// answers code 23 when another holds the claim.
fn claim_of(table: &TermTableView<'_>, predicate: u32, value: u32) -> Option<Claim> {
    let RequestTermView::Iri(predicate) = table.get(predicate)? else {
        return None;
    };
    let literal = match table.get(value)? {
        RequestTermView::Literal {
            lexical,
            datatype: None,
        } => Literal::new_simple_literal(lexical),
        RequestTermView::Literal {
            lexical,
            datatype: Some(datatype),
        } => Literal::new_typed_literal(lexical, NamedNode::new(datatype).ok()?),
        RequestTermView::LanguageLiteral {
            lexical,
            language,
            direction,
        } => match direction {
            None => Literal::new_language_tagged_literal(lexical, language).ok()?,
            Some(direction) => Literal::new_directional_language_tagged_literal(
                lexical,
                language,
                match direction {
                    Direction::Ltr => oxrdf::BaseDirection::Ltr,
                    Direction::Rtl => oxrdf::BaseDirection::Rtl,
                },
            )
            .ok()?,
        },
        RequestTermView::Iri(_)
        | RequestTermView::BlankNode(_)
        | RequestTermView::Triple { .. } => return None,
    };
    let encoded = EncodedTerm::literal::<TermError>(literal.as_ref(), |_| {
        Err(TermError::malformed(
            Vec::new(),
            "a datatype the store names by its ID",
        ))
    })
    .ok()?;
    Some(Claim {
        predicate: predicate.to_string(),
        value: encoded.term().as_bytes().to_vec(),
    })
}

impl<A: Authorizer + Clone> ClientSession<A> {
    /// A Write or an Operation on `stream`: refused by its terms and its transaction's rules, or kept in order, a read's held
    /// bytes bounded and otherwise held in step with the partition's answers.
    pub(super) async fn write(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
        bytes: &[u8],
    ) -> Result<Answer, ErrorReply> {
        let Some(txn) = self.transactions.get(&stream) else {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                "a write on a stream that holds no transaction",
            ));
        };
        if txn.read_only {
            let refusal = ErrorReply::new(
                ErrorCode::Protocol,
                format!("{:?} in a read-only transaction", view.command()),
            );
            self.end_transaction(stream, &refusal);
            return Ok(Answer::Open);
        }
        if let Some(refusal) = txn.ended.clone() {
            self.end_transaction(stream, &refusal);
            return Ok(Answer::Open);
        }
        let tenant = txn.tenant;
        let destination = match self.take_write(stream, view, tenant) {
            Ok(Some(destination)) => destination,
            Ok(None) => return Ok(Answer::Open),
            Err(refusal) => {
                self.end_transaction(stream, &refusal);
                return Ok(Answer::Open);
            }
        };
        if let Some(refusal) = self.hold(stream, bytes.to_vec(), destination) {
            self.end_transaction(stream, &refusal);
            return Ok(Answer::Open);
        }
        self.pump(stream);
        self.drain_unheld(stream).await;
        Ok(Answer::Open)
    }

    /// The write's destination; `None` for a declaration, which the transaction keeps for its Commit.
    fn take_write(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
        tenant: u64,
    ) -> Result<Option<Destination>, ErrorReply> {
        if view.command() == Command::Write {
            let write = WriteView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
            check_table(write.terms())?;
            let changes: Vec<WriteChange> = (0..write.len())
                .filter_map(|index| write.change(index))
                .collect();
            for change in &changes {
                check_change(write.terms(), tenant, change)?;
            }
            if self.take_declarations(stream, &changes)? {
                return Ok(None);
            }
            return Ok(Some(changes_destination(
                write.terms(),
                tenant,
                changes.into_iter(),
            )));
        }
        let operation =
            OperationView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        check_table(operation.terms())?;
        let operation_body = operation.operation();
        check_operation(operation.terms(), tenant, &operation_body)?;
        if self
            .transactions
            .get(&stream)
            .is_some_and(|txn| !txn.declared.is_empty())
        {
            return Err(declaring_refused(0, "an Operation"));
        }
        Ok(Some(operation_destination(
            operation.terms(),
            tenant,
            &operation_body,
        )))
    }

    /// Takes `changes` when they hold declarations: whether they did. A declaration mixes with nothing else (`PROTOCOL.md` §11.13).
    fn take_declarations(
        &mut self,
        stream: u64,
        changes: &[WriteChange],
    ) -> Result<bool, ErrorReply> {
        let txn = self.transactions.get_mut(&stream);
        let Some(txn) = txn else {
            return Ok(false);
        };
        let declares = changes
            .iter()
            .any(|change| matches!(change, WriteChange::Declare(_)));
        if !declares {
            if !txn.declared.is_empty() {
                return Err(declaring_refused(0, "a change of data"));
            }
            return Ok(false);
        }
        if txn.touched {
            return Err(declaring_refused(0, "the transaction changed or read data"));
        }
        for (index, change) in changes.iter().enumerate() {
            let WriteChange::Declare(declared) = change else {
                return Err(declaring_refused(index as u64, "a change of data"));
            };
            if let Declared::Predicate(predicate) = declared
                && predicate.vector_index().is_some()
            {
                return Err(ErrorReply::new(
                    ErrorCode::Refused,
                    format!(
                        "a vector index on {} arrives with stage 8",
                        predicate.predicate()
                    ),
                )
                .with_detail(index as u64));
            }
            txn.declared.push(declared.clone());
        }
        Ok(true)
    }

    /// Queues the frame; one a read holds back past `held_bytes` is refused with code 15.
    fn hold(
        &mut self,
        stream: u64,
        frame: Vec<u8>,
        destination: Destination,
    ) -> Option<ErrorReply> {
        let limit = self.core.limits().held_bytes();
        let txn = self.transactions.get_mut(&stream)?;
        txn.touched = true;
        let behind_read = !txn.reads_out.is_empty()
            || txn
                .queue
                .iter()
                .any(|queued| matches!(queued, Queued::Read(_)));
        let behind = if behind_read { frame.len() as u64 } else { 0 };
        if self.held.saturating_add(behind) > limit {
            return Some(
                ErrorReply::new(
                    ErrorCode::StreamLimit,
                    format!("the writes held behind this connection's reads pass {limit} bytes"),
                )
                .with_detail(limit),
            );
        }
        self.held += behind;
        txn.queue.push_back(Queued::Write {
            frame,
            destination,
            behind,
        });
        None
    }

    /// Lets the partition's answers in while what no read holds back passes `held_bytes`, so a client writing faster than its
    /// partition answers waits instead of growing this connection's memory. No client's Credit is needed for those answers.
    async fn drain_unheld(&mut self, stream: u64) {
        let limit = self.core.limits().held_bytes();
        loop {
            let unheld: u64 = self.transactions.get(&stream).map_or(0, |txn| {
                txn.queue
                    .iter()
                    .map(|queued| match queued {
                        Queued::Write {
                            frame, behind: 0, ..
                        } => frame.len() as u64,
                        Queued::Write { .. } | Queued::Read(_) | Queued::Commit { .. } => 0,
                    })
                    .sum()
            });
            if unheld <= limit {
                return;
            }
            // The session's own timers run meanwhile, so a partition that answers nothing ends the transaction with code 4.
            let wait = self
                .next_deadline()
                .map(|deadline| deadline.saturating_duration_since(now()));
            let event = match wait {
                Some(wait) => match compio::time::timeout(wait, self.events.next()).await {
                    Ok(Some(event)) => Some(event),
                    Ok(None) => return,
                    Err(_) => None,
                },
                None => match self.events.next().await {
                    Some(event) => Some(event),
                    None => return,
                },
            };
            match event {
                Some(event) => self.event(event),
                None => self.timers(now()),
            }
        }
    }

    /// Sends the Write or Operation at the queue's front on the Bind stream; its first keeps its frame until it is answered.
    pub(super) fn send_next_write(&mut self, transaction: u64) {
        let Some(txn) = self.transactions.get_mut(&transaction) else {
            return;
        };
        let Some(Queued::Write {
            mut frame, behind, ..
        }) = txn.queue.pop_front()
        else {
            return;
        };
        self.held = self.held.saturating_sub(behind);
        let first = !txn.first_sent;
        txn.first_sent = true;
        txn.first_open = first;
        if let BindState::Bound { link, stream, .. } = &txn.bind {
            // A frame built by this connection's peer holds its header, or the framer refused it.
            drop(Header::stamp_stream(&mut frame, *stream));
            link.send(frame.clone());
        }
        txn.write_out = Some(if first { frame } else { Vec::new() });
    }

    /// Asks the predicate's home for its system ID, what a claim's bucket is computed from.
    pub(super) fn ask_claim(&mut self, stream: u64, claim: &Claim) {
        let tenant = self.claim_tenant(stream);
        let home = match iri_place(tenant, &claim.predicate) {
            IriPlace::Ordinary(bucket) | IriPlace::Skolem(bucket) => {
                self.core.links().owner_of(bucket)
            }
            IriPlace::Nobody | IriPlace::TooLong => None,
        };
        let Some(link) = home.and_then(|home| self.core.links().partition(home)) else {
            let refusal = self.unavailable();
            self.claim_failed(stream, &refusal);
            return;
        };
        let mut ask = Reference::request().in_tenant(tenant);
        let _index = ask.register_iri(home.unwrap_or(0), None, &claim.predicate, 0);
        let header = link.header(Command::Reference, link.next_stream());
        let Ok(frame) = ask.frame(header) else {
            let refusal = self.unavailable();
            self.claim_failed(stream, &refusal);
            return;
        };
        if let Some(txn) = self.transactions.get_mut(&stream) {
            txn.awaiting_claim = true;
        }
        let sink = self.sink(RouteKey::Claim { stream });
        let _stream = link.open(frame, sink);
        self.keep_claim(stream, claim.clone());
    }

    fn claim_tenant(&self, stream: u64) -> u64 {
        self.transactions
            .get(&stream)
            .map(|txn| txn.tenant)
            .or_else(|| self.one_frames.get(&stream).map(|one| one.tenant))
            .unwrap_or(0)
    }

    fn keep_claim(&mut self, stream: u64, claim: Claim) {
        let _replaced = self.claims.insert(stream, claim);
    }

    fn claim_failed(&mut self, stream: u64, refusal: &ErrorReply) {
        if self.transactions.contains_key(&stream) {
            self.end_transaction(stream, refusal);
        } else {
            self.one_frame_answered(stream);
            self.push_error(stream, refusal);
        }
    }

    /// The home's answer: the partition owning the claim's bucket.
    pub(super) fn claim_event(&mut self, stream: u64, event: LinkEvent) {
        let Some(claim) = self.claims.remove(&stream) else {
            return;
        };
        let tenant = self.claim_tenant(stream);
        let registered = match event {
            LinkEvent::Frame(frame) if command_of(&frame) == Some(Command::Reference) => {
                FrameView::validate(&frame, self.cluster(), MAX_FRAME_SIZE)
                    .ok()
                    .and_then(|view| ReferenceView::from_frame(&view).ok()?.registration(0))
            }
            LinkEvent::Frame(_) | LinkEvent::Closed => None,
        };
        // A home refusing the claim's predicate refuses the write, as its partition would (`PROTOCOL.md` §13.15): code 7, not in
        // doubt.
        if registered.is_some_and(|held| held.outcome() == ErrorCode::Refused.number()) {
            let refusal = ErrorReply::new(
                ErrorCode::Refused,
                "the claim's home refused to register its predicate",
            );
            self.claim_failed(stream, &refusal);
            return;
        }
        let partition = registered.and_then(|held| {
            let id = held.entity()?;
            let bucket = Bucket::of_value(Tenant::new(tenant), id, &claim.value);
            self.core.links().owner_of(bucket)
        });
        let Some(partition) = partition else {
            let refusal = self.unavailable();
            self.claim_failed(stream, &refusal);
            return;
        };
        if let Some(txn) = self.transactions.get_mut(&stream) {
            txn.awaiting_claim = false;
            txn.claimed = Some(partition);
            self.pump(stream);
        } else {
            self.send_one_frame(stream, partition);
        }
    }
}
