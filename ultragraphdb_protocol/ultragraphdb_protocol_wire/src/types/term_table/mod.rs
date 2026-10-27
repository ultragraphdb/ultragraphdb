/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::direction::Direction;
use crate::types::request_term::RequestTerm;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::constants::MAX_TERM_DEPTH;
use crate::utils::sections::as_u32;

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
    pub fn new() -> Self {
        Self::default()
    }

    /// The index of `term`, appended the first time only. Refused, as a table outside any frame names command 0, with
    /// [`ProtocolError::IndexOutOfRange`] for a triple term naming an index not in the table,
    /// [`ProtocolError::TermTooDeep`] for one nested past [`crate::MAX_TERM_DEPTH`], and [`ProtocolError::BadTerm`] for
    /// a literal stating `xsd:string` or an empty language tag.
    pub fn intern(&mut self, term: RequestTerm) -> Result<u32, ProtocolError> {
        if let Some(index) = self.index.get(&term) {
            return Ok(*index);
        }
        let at = self.terms.len();
        let depth = match &term {
            RequestTerm::Literal {
                datatype: Some(datatype),
                ..
            } if datatype == XSD_STRING => return Err(ProtocolError::BadTerm(0, 0, at)),
            RequestTerm::LanguageLiteral { language, .. } if language.is_empty() => {
                return Err(ProtocolError::BadTerm(0, 0, at));
            }
            RequestTerm::Triple {
                subject,
                predicate,
                object,
            } => {
                let mut deepest = 0;
                for part in [*subject, *predicate, *object] {
                    let depth = self
                        .depths
                        .get(part as usize)
                        .ok_or(ProtocolError::IndexOutOfRange(0, 0, at, part, as_u32(at)))?;
                    deepest = deepest.max(*depth);
                }
                if deepest + 1 > MAX_TERM_DEPTH {
                    return Err(ProtocolError::TermTooDeep(
                        0,
                        0,
                        at,
                        deepest + 1,
                        MAX_TERM_DEPTH,
                    ));
                }
                deepest + 1
            }
            _ => 0,
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

    pub fn len(&self) -> usize {
        self.terms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Term `index`, `None` past the last.
    pub fn get(&self, index: u32) -> Option<&RequestTerm> {
        self.terms.get(index as usize)
    }

    pub fn terms(&self) -> &[RequestTerm] {
        &self.terms
    }

    /// Appends `term`, of `depth`, which the caller checked; its index.
    fn insert(&mut self, term: RequestTerm, depth: usize) -> u32 {
        let index = as_u32(self.terms.len());
        self.index.insert(term.clone(), index);
        self.terms.push(term);
        self.depths.push(depth);
        index
    }

    /// The index of term `index` of `from`, interned with the parts of a triple term before it; `None` when `from`
    /// has no such term. A checked table nests at most [`MAX_TERM_DEPTH`] deep, which bounds the recursion.
    pub(crate) fn import(&mut self, from: &TermTableView<'_>, index: u32) -> Option<u32> {
        let term = match from.get(index)? {
            RequestTermView::Triple {
                subject,
                predicate,
                object,
            } => RequestTerm::Triple {
                subject: self.import(from, subject)?,
                predicate: self.import(from, predicate)?,
                object: self.import(from, object)?,
            },
            other => other.to_term(),
        };
        self.intern(term).ok()
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
