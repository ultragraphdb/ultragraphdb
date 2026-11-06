/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_wire::{
    ComputeReadView, ErrorReply, PatternSlot, ReadLevel, RequestTerm, RequestTermView,
    TermTableView,
};
use xxhash_rust::xxh3::xxh3_64;

use crate::utils::clock::now;
use crate::utils::refusals::term_refused;

/// A read's graph position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GraphSlot {
    Unbound,
    Default,
    Named(String),
}

/// A ComputeRead, its frame given back: what the read needs of it for as long as it runs.
#[derive(Debug)]
pub(crate) struct ReadRequest {
    pub(crate) stream: u64,
    pub(crate) partition: u32,
    pub(crate) read_only: bool,
    pub(crate) level: ReadLevel,
    pub(crate) named: bool,
    pub(crate) tenant: u64,
    pub(crate) read_version: u64,
    pub(crate) epoch: u64,
    pub(crate) principal: u64,
    pub(crate) transaction: [u8; 16],
    pub(crate) limit: u32,
    pub(crate) deadline_ms: u32,
    pub(crate) max_frame: u32,
    pub(crate) graph: GraphSlot,
    pub(crate) subject: Option<String>,
    pub(crate) predicate: Option<String>,
    pub(crate) object: Option<RequestTerm>,
    pub(crate) resume: Vec<u8>,
    pub(crate) pattern_hash: u64,
    pub(crate) arrived: Instant,
}

/// The IRI of `slot`, `None` when it is unbound; a term of another kind refused (`PROTOCOL.md` §4.4).
fn iri_of(
    terms: &TermTableView<'_>,
    slot: PatternSlot,
    rule: &str,
) -> Result<Option<String>, ErrorReply> {
    match slot {
        PatternSlot::Unbound | PatternSlot::DefaultGraph | PatternSlot::Parameter(_) => Ok(None),
        PatternSlot::Bound(index) => match terms.get(index) {
            Some(RequestTermView::Iri(iri)) => Ok(Some(iri.to_string())),
            Some(
                RequestTermView::BlankNode(_)
                | RequestTermView::Literal { .. }
                | RequestTermView::LanguageLiteral { .. }
                | RequestTermView::Triple { .. },
            )
            | None => Err(term_refused(index, rule)),
        },
    }
}

/// The object of `slot`; a blank node's label or a triple term refused (`PROTOCOL.md` §4.4).
fn object_of(
    terms: &TermTableView<'_>,
    slot: PatternSlot,
) -> Result<Option<RequestTerm>, ErrorReply> {
    let PatternSlot::Bound(index) = slot else {
        return Ok(None);
    };
    match terms.get(index) {
        Some(
            term @ (RequestTermView::Iri(_)
            | RequestTermView::Literal { .. }
            | RequestTermView::LanguageLiteral { .. }),
        ) => Ok(Some(term.to_term())),
        Some(RequestTermView::BlankNode(_) | RequestTermView::Triple { .. }) | None => Err(
            term_refused(index, "a pattern's object slot holds an IRI or a literal"),
        ),
    }
}

impl ReadRequest {
    /// The read `view` asks on `stream`.
    pub(crate) fn of(view: &ComputeReadView<'_>, stream: u64) -> Result<Self, ErrorReply> {
        let terms = view.terms();
        let pattern = view.pattern();
        let graph = match pattern.graph() {
            PatternSlot::DefaultGraph => GraphSlot::Default,
            slot @ PatternSlot::Bound(_) => GraphSlot::Named(
                iri_of(terms, slot, "a pattern's graph slot holds an IRI")?.unwrap_or_default(),
            ),
            PatternSlot::Unbound | PatternSlot::Parameter(_) => GraphSlot::Unbound,
        };
        Ok(Self {
            stream,
            partition: view.partition(),
            read_only: view.read_only(),
            level: view.level(),
            named: view.named(),
            tenant: view.tenant(),
            read_version: view.read_version(),
            epoch: view.epoch(),
            principal: view.principal(),
            transaction: view.transaction(),
            limit: view.limit(),
            deadline_ms: view.deadline_ms(),
            max_frame: view.max_frame(),
            graph,
            subject: iri_of(
                terms,
                pattern.subject(),
                "a pattern's subject slot holds an IRI",
            )?,
            predicate: iri_of(
                terms,
                pattern.predicate(),
                "a pattern's predicate slot holds an IRI",
            )?,
            object: object_of(terms, pattern.object())?,
            resume: view.resume().to_vec(),
            pattern_hash: view.pattern_hash(),
            arrived: now(),
        })
    }

    /// Whether the read is a read-write transaction's.
    pub(crate) fn is_read_write(&self) -> bool {
        !self.read_only
    }

    /// The pattern's unbound slots and the reifier column.
    pub(crate) fn columns(&self) -> u32 {
        let unbound = [
            self.graph == GraphSlot::Unbound,
            self.subject.is_none(),
            self.predicate.is_none(),
            self.object.is_none(),
        ];
        unbound
            .iter()
            .fold(1, |columns, open| columns + u32::from(*open))
    }

    /// What binds the read's resume key to its transaction (`PROTOCOL.md` §11.8).
    pub(crate) fn scope(&self) -> u64 {
        if self.read_only {
            self.read_version
        } else {
            xxh3_64(&self.transaction)
        }
    }

    /// The milliseconds left until the transaction's deadline.
    pub(crate) fn deadline_left(&self) -> u32 {
        let spent =
            u32::try_from(now().duration_since(self.arrived).as_millis()).unwrap_or(u32::MAX);
        self.deadline_ms.saturating_sub(spent)
    }
}
