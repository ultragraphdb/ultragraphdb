/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use ultragraphdb_core_body::as_u32;
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::direction::Direction;
use crate::types::request_term::RequestTerm;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::constants::MAX_TERM_DEPTH;

/// The size of a term record.
pub(crate) const TERM_RECORD_SIZE: usize = 24;

/// The datatype a literal without one has, which a table never states.
pub(crate) const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// A term kind's byte.
pub(crate) mod kind {
    pub(crate) const IRI: u8 = 1;
    pub(crate) const BLANK_NODE: u8 = 2;
    pub(crate) const LITERAL: u8 = 3;
    pub(crate) const LANGUAGE_LITERAL: u8 = 4;
    pub(crate) const TRIPLE: u8 = 5;
}

/// The terms a request names, each once, by text: the request bodies' term encoding, two sections of a body.
///
/// | Section | Contents |
/// |---|---|
/// | records | one 24-byte record per term, in table order |
/// | text | the terms' texts, UTF-8, back to back in table order: each record's first text, then its second |
///
/// | Record offset | Field | Type |
/// |---|---|---|
/// | 0 | `kind` | u8: 1 IRI, 2 blank node, 3 literal, 4 language-tagged literal, 5 triple term |
/// | 1 | `direction` | u8: 0 none, 1 left to right, 2 right to left; nonzero only for kind 4 |
/// | 2 | reserved | 6 bytes, zero |
/// | 8 | `first` | u32: the first text's offset in the text section; a triple term's subject index |
/// | 12 | `first_len` | u32: the first text's length; a triple term's predicate index |
/// | 16 | `second` | u32: the second text's offset, where the first ends; a triple term's object index |
/// | 20 | `second_len` | u32: the second text's length; zero for a triple term |
///
/// The first text is an IRI's text, a blank node's label or a literal's lexical form; the second a literal's datatype
/// IRI, empty for `xsd:string`, or a language tag, never empty; IRIs and blank nodes have no second text. A triple
/// term's indices name earlier entries, so the table has no cycle, and it nests at most [`crate::MAX_TERM_DEPTH`] deep. A term has one encoding: texts follow each other
/// with no gap, `xsd:string` is never stated, and no term appears twice.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TermTable {
    terms: Vec<RequestTerm>,
    depths: Vec<usize>,
    index: HashMap<RequestTerm, u32>,
}

impl TermTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// The index of `term`, appended the first time only. Refused, as a table outside any frame names command 0, with
    /// [`ProtocolError::index_out_of_range`] for a triple term naming an index not in the table,
    /// [`ProtocolError::term_too_deep`] for one nested past [`crate::MAX_TERM_DEPTH`], and [`ProtocolError::bad_term`] for
    /// a literal stating `xsd:string` or an empty datatype, which is written as `xsd:string`'s, or an empty language
    /// tag.
    pub fn intern(&mut self, term: RequestTerm) -> Result<u32, ProtocolError> {
        if let Some(index) = self.index.get(&term) {
            return Ok(*index);
        }
        let at = self.terms.len();
        let depth =
            match &term {
                RequestTerm::Literal {
                    datatype: Some(datatype),
                    ..
                } if datatype == XSD_STRING || datatype.is_empty() => {
                    return Err(ProtocolError::bad_term(0, 0, at));
                }
                RequestTerm::LanguageLiteral { language, .. } if language.is_empty() => {
                    return Err(ProtocolError::bad_term(0, 0, at));
                }
                RequestTerm::Triple {
                    subject,
                    predicate,
                    object,
                } => {
                    let mut deepest = 0;
                    for part in [*subject, *predicate, *object] {
                        // Lossless: a `usize` is at least 32 bits.
                        let depth = self.depths.get(part as usize).ok_or(
                            ProtocolError::index_out_of_range(0, 0, at, part, as_u32(at)),
                        )?;
                        deepest = deepest.max(*depth);
                    }
                    if deepest + 1 > MAX_TERM_DEPTH {
                        return Err(ProtocolError::term_too_deep(
                            0,
                            0,
                            at,
                            deepest + 1,
                            MAX_TERM_DEPTH,
                        ));
                    }
                    deepest + 1
                }
                RequestTerm::Iri(_)
                | RequestTerm::BlankNode(_)
                | RequestTerm::Literal { .. }
                | RequestTerm::LanguageLiteral { .. } => 0,
            };
        Ok(self.insert(term, depth))
    }

    /// The index of the IRI `iri`, interned.
    pub fn iri(&mut self, iri: &str) -> u32 {
        let term = RequestTerm::Iri(iri.to_string());
        match self.index.get(&term) {
            Some(index) => *index,
            None => self.insert(term, 0),
        }
    }

    /// The number of terms.
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Whether the table holds no term.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Term `index`, `None` past the last.
    pub fn get(&self, index: u32) -> Option<&RequestTerm> {
        self.terms.get(index as usize)
    }

    /// The terms, in order.
    pub fn terms(&self) -> &[RequestTerm] {
        &self.terms
    }

    /// Appends `term`, of `depth`, which the caller checked; its index.
    fn insert(&mut self, term: RequestTerm, depth: usize) -> u32 {
        let index = as_u32(self.terms.len());
        let _absent: Option<u32> = self.index.insert(term.clone(), index);
        self.terms.push(term);
        self.depths.push(depth);
        index
    }

    /// The index of term `index` of `from`, interned with the parts of a triple term before it, a triple's subject's
    /// parts first, then its predicate's, then its object's; `None` when `from` has no such term, or when this table
    /// refuses one of the terms it interns.
    ///
    /// The walk keeps an explicit stack and imports each entry of `from` once, so a triple naming one part twice at
    /// every level of [`MAX_TERM_DEPTH`] costs one step per entry, not one per path. Each entry is pushed once per
    /// parent naming it and a triple is expanded once, so the walk ends within `4 × from.len() + 1` steps.
    pub(crate) fn import(&mut self, from: &TermTableView<'_>, index: u32) -> Option<u32> {
        let steps_max = from.len().checked_mul(4)?.checked_add(1)?;
        let mut imported: HashMap<u32, u32> = HashMap::new();
        let mut stack = vec![index];
        for _ in 0..steps_max {
            let Some(top) = stack.last().copied() else {
                return imported.get(&index).copied();
            };
            if imported.contains_key(&top) {
                let _done: Option<u32> = stack.pop();
                continue;
            }
            let term = match from.get(top)? {
                RequestTermView::Triple {
                    subject,
                    predicate,
                    object,
                } => {
                    let parts = [subject, predicate, object];
                    let pending: Vec<u32> = parts
                        .iter()
                        .rev()
                        .filter(|part| !imported.contains_key(part))
                        .copied()
                        .collect();
                    if !pending.is_empty() {
                        stack.extend(pending);
                        continue;
                    }
                    let [into_subject, into_predicate, into_object] =
                        parts.map(|part| imported.get(&part).copied());
                    RequestTerm::Triple {
                        subject: into_subject?,
                        predicate: into_predicate?,
                        object: into_object?,
                    }
                }
                other @ (RequestTermView::Iri(_)
                | RequestTermView::BlankNode(_)
                | RequestTermView::Literal { .. }
                | RequestTermView::LanguageLiteral { .. }) => other.to_term(),
            };
            let _first: Option<u32> = imported.insert(top, self.intern(term).ok()?);
            let _done: Option<u32> = stack.pop();
        }
        None
    }

    /// The two sections, the records and the text.
    pub(crate) fn sections(&self) -> (Vec<u8>, Vec<u8>) {
        let mut records = Vec::with_capacity(TERM_RECORD_SIZE * self.terms.len());
        let mut text = Vec::new();
        for term in &self.terms {
            let mut texts = |kind: u8, direction: u8, first: &str, second: &str| {
                records.extend_from_slice(&[kind, direction, 0, 0, 0, 0, 0, 0]);
                for part in [first, second] {
                    records.extend_from_slice(&as_u32(text.len()).to_le_bytes());
                    records.extend_from_slice(&as_u32(part.len()).to_le_bytes());
                    text.extend_from_slice(part.as_bytes());
                }
            };
            match term {
                RequestTerm::Iri(iri) => texts(kind::IRI, 0, iri, ""),
                RequestTerm::BlankNode(label) => texts(kind::BLANK_NODE, 0, label, ""),
                RequestTerm::Literal { lexical, datatype } => texts(
                    kind::LITERAL,
                    0,
                    lexical,
                    datatype.as_deref().unwrap_or_default(),
                ),
                RequestTerm::LanguageLiteral {
                    lexical,
                    language,
                    direction,
                } => {
                    let direction = match direction {
                        None => 0,
                        Some(Direction::Ltr) => 1,
                        Some(Direction::Rtl) => 2,
                    };
                    texts(kind::LANGUAGE_LITERAL, direction, lexical, language)
                }
                RequestTerm::Triple {
                    subject,
                    predicate,
                    object,
                } => {
                    records.extend_from_slice(&[kind::TRIPLE, 0, 0, 0, 0, 0, 0, 0]);
                    for field in [*subject, *predicate, *object, 0] {
                        records.extend_from_slice(&field.to_le_bytes());
                    }
                }
            }
        }
        (records, text)
    }
}
