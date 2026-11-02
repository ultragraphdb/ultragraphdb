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

/// Where a term table's entry sits, for the errors naming it.
#[derive(Debug, Clone, Copy)]
struct Entry {
    command: u16,
    record_section: usize,
    text_section: usize,
    at: usize,
}

impl Entry {
    /// The record's kind and direction, the kind known, the direction only on a language-tagged literal, and the
    /// reserved bytes zero; `base` is the records' body offset.
    fn kind_and_direction(self, base: usize, record: &[u8]) -> Result<(u8, u8), ProtocolError> {
        let command = self.command;
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
                base + TERM_RECORD_SIZE * self.at + 2 + j,
            ));
        }
        Ok((term_kind, direction))
    }

    /// A triple term's key and depth: its parts earlier entries, of `depths`, its unused field zero, and its depth at
    /// most [`MAX_TERM_DEPTH`].
    fn triple(
        self,
        fields: [u32; 4],
        depths: &[usize],
    ) -> Result<(Key<'static>, usize), ProtocolError> {
        let [first, first_len, second, second_len] = fields;
        let mut deepest = 0;
        for part in [first, first_len, second] {
            let depth = depths
                .get(part as usize)
                .ok_or(ProtocolError::IndexOutOfRange(
                    self.command,
                    self.record_section,
                    self.at,
                    part,
                    as_u32(self.at),
                ))?;
            deepest = deepest.max(*depth);
        }
        if second_len != 0 {
            return Err(ProtocolError::FieldValue(
                self.command,
                "triple term",
                second_len.into(),
            ));
        }
        if deepest + 1 > MAX_TERM_DEPTH {
            return Err(ProtocolError::TermTooDeep(
                self.command,
                self.record_section,
                self.at,
                deepest + 1,
                MAX_TERM_DEPTH,
            ));
        }
        Ok((Key::Triple(first, first_len, second), deepest + 1))
    }

    /// A text term's key and its texts' boundaries, the first's start and end and the second's end: the texts where
    /// the texts before end, at `position`, inside `text`, and the second as the kind takes it.
    fn text_term<'a>(
        self,
        term_kind: u8,
        direction: u8,
        fields: [u32; 4],
        position: usize,
        text: &'a [u8],
    ) -> Result<(Key<'a>, [usize; 3]), ProtocolError> {
        let [first, first_len, second, second_len] = fields;
        let limit = text.len();
        let span = |start: u32, len: u32| -> Result<(usize, usize), ProtocolError> {
            let start = start as usize;
            let end = start.saturating_add(len as usize);
            if end > limit {
                return Err(ProtocolError::IndexOutOfRange(
                    self.command,
                    self.text_section,
                    self.at,
                    as_u32(end),
                    as_u32(limit),
                ));
            }
            Ok((start, end))
        };
        if first as usize != position {
            return Err(ProtocolError::FieldValue(
                self.command,
                "term text offset",
                first.into(),
            ));
        }
        let (a, a_end) = span(first, first_len)?;
        if second as usize != a_end {
            return Err(ProtocolError::FieldValue(
                self.command,
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
            return Err(ProtocolError::BadTerm(
                self.command,
                self.record_section,
                self.at,
            ));
        }
        let first_text = text.get(a..a_end).unwrap_or_default();
        Ok((
            Key::Text(term_kind, direction, first_text, second_text),
            [a, a_end, b_end],
        ))
    }
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
            let entry = Entry {
                command,
                record_section,
                text_section,
                at,
            };
            let (term_kind, direction) = entry.kind_and_direction(base, record)?;
            let (key, depth) = if term_kind == kind::TRIPLE {
                entry.triple(fields(record), &depths)?
            } else {
                let (key, ends) =
                    entry.text_term(term_kind, direction, fields(record), position, text)?;
                boundaries.extend(ends);
                position = ends[2];
                (key, 0)
            };
            if let Some(earlier) = seen.get(&key) {
                return Err(ProtocolError::DuplicateTerm(
                    command,
                    record_section,
                    at,
                    *earlier,
                ));
            }
            let _absent: Option<usize> = seen.insert(key, at);
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

    /// The number of terms.
    pub fn len(&self) -> usize {
        self.records.len() / TERM_RECORD_SIZE
    }

    /// Whether the table holds no term.
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
                let _index: Option<u32> = table.intern(term.to_term()).ok();
            }
        }
        table
    }
}
