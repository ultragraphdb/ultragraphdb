/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    ErrorReply, PatternSlot, QuadPattern, ReadView, RequestTerm, RequestTermView, TermTableView,
};

use crate::types::graph_slot::GraphSlot;
use crate::utils::refusals::term_refused;

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

/// What a client's Read asks of the store, its frame given back, taken once when the read arrives and held while the read
/// waits its turn in its transaction: the pattern's bound positions by their text, its `limit`, its resume key and the
/// pattern's hash a resume key holds (`PROTOCOL.md` §11.8). An Execute is read as the Read of its statement with its
/// parameters bound. [`crate::ReadRequest::new`] adds what the read takes of its transaction when it starts.
#[derive(Debug, Clone)]
pub struct ReadPattern {
    pub(crate) graph: GraphSlot,
    pub(crate) subject: Option<String>,
    pub(crate) predicate: Option<String>,
    pub(crate) object: Option<RequestTerm>,
    pub(crate) limit: u32,
    pub(crate) resume: Vec<u8>,
    pub(crate) pattern_hash: u64,
}

impl ReadPattern {
    /// What `read` asks: each bound position's term by its text, a graph or subject or predicate slot holding an IRI and an
    /// object slot an IRI or a literal (`PROTOCOL.md` §4.4).
    ///
    /// # Errors
    ///
    /// Error 7 (Refused), `detail` the term's index, for a slot holding a term of another kind.
    pub fn of_read(read: &ReadView<'_>) -> Result<Self, ErrorReply> {
        Self::of_parts(
            (read.terms(), read.pattern()),
            read.limit(),
            read.resume(),
            read.pattern_hash(),
        )
    }

    /// What a request of `terms` and `pattern` asks, at most `limit` rows from `resume`, its pattern's hash `pattern_hash`.
    pub(crate) fn of_parts(
        (terms, pattern): (&TermTableView<'_>, &QuadPattern),
        limit: u32,
        resume: &[u8],
        pattern_hash: u64,
    ) -> Result<Self, ErrorReply> {
        let graph = match pattern.graph() {
            PatternSlot::DefaultGraph => GraphSlot::Default,
            slot @ PatternSlot::Bound(_) => GraphSlot::Named(
                iri_of(terms, slot, "a pattern's graph slot holds an IRI")?.unwrap_or_default(),
            ),
            PatternSlot::Unbound | PatternSlot::Parameter(_) => GraphSlot::Unbound,
        };
        Ok(Self {
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
            limit,
            resume: resume.to_vec(),
            pattern_hash,
        })
    }
}
