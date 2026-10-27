/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::direction::Direction;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table::{TERM_RECORD_SIZE, TermTable, XSD_STRING, kind};
use crate::utils::constants::MAX_TERM_DEPTH;
use crate::utils::le::bytes_at;
use crate::utils::sections::{as_u32, records as whole_records};

/// What makes two entries the same term: a text term's kind, direction and texts, or a triple term's parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key<'a> {
    Text(u8, u8, &'a [u8], &'a [u8]),
    Triple(u32, u32, u32),
}

/// The four u32 fields of a record.
fn fields(record: &[u8]) -> [u32; 4] {
    [8, 12, 16, 20].map(|at| bytes_at(record, at).map_or(0, u32::from_le_bytes))
}

/// A received request's term table, read in place; see [`TermTable`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermTableView<'a> {
    records: &'a [u8],
    text: &'a str,
}

impl<'a> TermTableView<'a> {
    /// The table of `records` and `text`, sections `sections.0` and `sections.1` of a body of `command` whose records
    /// start at body offset `base`, checked once, entry by entry in order: whole records
    /// ([`ProtocolError::SectionLength`]); the kind known ([`ProtocolError::FieldValue`]); the direction only on a
    /// language-tagged literal ([`ProtocolError::FieldValue`]); the reserved bytes zero
    /// ([`ProtocolError::PaddingNotZero`]); a text term's texts where the texts before end
    /// ([`ProtocolError::FieldValue`]) and inside the text section ([`ProtocolError::IndexOutOfRange`]); the second text
    /// as the kind takes it, none for an IRI or a blank node, never `xsd:string`, a language tag never empty
    /// ([`ProtocolError::BadTerm`]); a triple term's indices earlier than its own ([`ProtocolError::IndexOutOfRange`])
    /// and its unused field zero, and its depth, computed in the same pass, at most [`crate::MAX_TERM_DEPTH`]
    /// ([`ProtocolError::TermTooDeep`]); no entry equal to an earlier one ([`ProtocolError::DuplicateTerm`]); then the texts
    /// filling the section ([`ProtocolError::CountMismatch`]); the section UTF-8, checked once, and every text boundary
    /// on a character boundary ([`ProtocolError::NotUtf8`]).
    pub(crate) fn read(
        command: u16,
        sections: (usize, usize),
        base: usize,
        records: &'a [u8],
        text: &'a [u8],
    ) -> Result<Self, ProtocolError> {
        let (record_section, text_section) = sections;
        let limit = text.len();
        let mut position = 0usize;
        let mut depths: Vec<usize> = Vec::new();
        let mut seen: HashMap<Key<'a>, usize> = HashMap::new();
        let mut boundaries = Vec::new();
        for (at, record) in
            whole_records(command, record_section, records, TERM_RECORD_SIZE)?.enumerate()
        {
            let term_kind = record.first().copied().unwrap_or(0);
            let direction = record.get(1).copied().unwrap_or(0);
            if !(kind::IRI..=kind::TRIPLE).contains(&term_kind) {
                return Err(ProtocolError::FieldValue(
                    command,
                    "term kind",
                    term_kind.into(),
                ));
            }
            if direction > 2 || (direction != 0 && term_kind != kind::LANGUAGE_LITERAL) {
                return Err(ProtocolError::FieldValue(
                    command,
                    "direction",
                    direction.into(),
                ));
            }
            if let Some(j) = record
                .get(2..8)
                .and_then(|zeros| zeros.iter().position(|b| *b != 0))
            {
                return Err(ProtocolError::PaddingNotZero(
                    command,
                    base + TERM_RECORD_SIZE * at + 2 + j,
                ));
            }
            let [first, first_len, second, second_len] = fields(record);
            let (key, depth) = if term_kind == kind::TRIPLE {
                let mut deepest = 0;
                for part in [first, first_len, second] {
                    let depth = depths
                        .get(part as usize)
                        .ok_or(ProtocolError::IndexOutOfRange(
                            command,
                            record_section,
                            at,
                            part,
                            as_u32(at),
                        ))?;
                    deepest = deepest.max(*depth);
                }
                if second_len != 0 {
                    return Err(ProtocolError::FieldValue(
                        command,
                        "triple term",
                        second_len.into(),
                    ));
                }
                if deepest + 1 > MAX_TERM_DEPTH {
                    return Err(ProtocolError::TermTooDeep(
                        command,
                        record_section,
                        at,
                        deepest + 1,
                        MAX_TERM_DEPTH,
                    ));
                }
                (Key::Triple(first, first_len, second), deepest + 1)
            } else {
                let span = |start: u32, len: u32| -> Result<(usize, usize), ProtocolError> {
                    let start = start as usize;
                    let end = start.saturating_add(len as usize);
                    if end > limit {
                        return Err(ProtocolError::IndexOutOfRange(
                            command,
                            text_section,
                            at,
                            as_u32(end),
                            as_u32(limit),
                        ));
                    }
                    Ok((start, end))
                };
                if first as usize != position {
                    return Err(ProtocolError::FieldValue(
                        command,
                        "term text offset",
                        first.into(),
                    ));
                }
                let (a, a_end) = span(first, first_len)?;
                if second as usize != a_end {
                    return Err(ProtocolError::FieldValue(
                        command,
                        "term text offset",
                        second.into(),
                    ));
                }
                let (b, b_end) = span(second, second_len)?;
                let second_text = text.get(b..b_end).unwrap_or_default();
                let second_wrong = match term_kind {
                    kind::IRI | kind::BLANK_NODE => second_len != 0,
                    kind::LITERAL => second_text == XSD_STRING.as_bytes(),
                    _ => second_len == 0,
                };
                if second_wrong {
                    return Err(ProtocolError::BadTerm(command, record_section, at));
                }
                boundaries.extend([a, a_end, b_end]);
                position = b_end;
                let first_text = text.get(a..a_end).unwrap_or_default();
                (Key::Text(term_kind, direction, first_text, second_text), 0)
            };
            if let Some(earlier) = seen.get(&key) {
                return Err(ProtocolError::DuplicateTerm(
                    command,
                    record_section,
                    at,
                    *earlier,
                ));
            }
            seen.insert(key, at);
            depths.push(depth);
        }
        if position != limit {
            return Err(ProtocolError::CountMismatch(
                command,
                "term text",
                limit as u64,
                position as u64,
            ));
        }
        let text = std::str::from_utf8(text)
            .map_err(|e| ProtocolError::NotUtf8(command, text_section, e.valid_up_to()))?;
        if let Some(boundary) = boundaries.iter().find(|b| !text.is_char_boundary(**b)) {
            return Err(ProtocolError::NotUtf8(command, text_section, *boundary));
        }
        Ok(Self { records, text })
    }

    pub fn len(&self) -> usize {
        self.records.len() / TERM_RECORD_SIZE
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Term `index`, `None` past the last.
    pub fn get(&self, index: u32) -> Option<RequestTermView<'a>> {
        let at = (index as usize).checked_mul(TERM_RECORD_SIZE)?;
        let record = self.records.get(at..at.checked_add(TERM_RECORD_SIZE)?)?;
        let [first, first_len, second, second_len] = fields(record);
        let span = |start: u32, len: u32| {
            let start = start as usize;
            self.text.get(start..start.checked_add(len as usize)?)
        };
        Some(match record.first().copied()? {
            kind::TRIPLE => RequestTermView::Triple {
                subject: first,
                predicate: first_len,
                object: second,
            },
            kind::IRI => RequestTermView::Iri(span(first, first_len)?),
            kind::BLANK_NODE => RequestTermView::BlankNode(span(first, first_len)?),
            kind::LITERAL => RequestTermView::Literal {
                lexical: span(first, first_len)?,
                datatype: span(second, second_len).filter(|d| !d.is_empty()),
            },
            _ => RequestTermView::LanguageLiteral {
                lexical: span(first, first_len)?,
                language: span(second, second_len)?,
                direction: match record.get(1).copied()? {
                    1 => Some(Direction::Ltr),
                    2 => Some(Direction::Rtl),
                    _ => None,
                },
            },
        })
    }

    /// The owned table.
    pub fn to_table(&self) -> TermTable {
        let mut table = TermTable::new();
        for index in 0..as_u32(self.len()) {
            if let Some(term) = self.get(index) {
                // A checked table holds each term once, its triple terms over earlier entries: interning keeps
                // every index.
                let _ = table.intern(term.to_term());
            }
        }
        table
    }
}
