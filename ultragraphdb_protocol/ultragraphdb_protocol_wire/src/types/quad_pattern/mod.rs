/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::column_type::ColumnType;
use crate::types::pattern_slot::PatternSlot;
use crate::types::string_list::StringList;
use crate::utils::le::bytes_at;
use crate::utils::sections::as_u32;
use crate::utils::string_writer::StringWriter;

/// The size of a pattern on the wire: four slots of 8 bytes.
pub(crate) const PATTERN_SIZE: usize = 32;

/// A quad pattern: graph, subject, predicate and object, each a [`PatternSlot`]. `T` is the bound terms' text,
/// `String` in a request being built and `&str` borrowing a received one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuadPattern<T> {
    graph: PatternSlot<T>,
    subject: PatternSlot<T>,
    predicate: PatternSlot<T>,
    object: PatternSlot<T>,
}

impl<T> QuadPattern<T> {
    pub fn new(
        graph: PatternSlot<T>,
        subject: PatternSlot<T>,
        predicate: PatternSlot<T>,
        object: PatternSlot<T>,
    ) -> Self {
        Self {
            graph,
            subject,
            predicate,
            object,
        }
    }

    pub fn graph(&self) -> &PatternSlot<T> {
        &self.graph
    }

    pub fn subject(&self) -> &PatternSlot<T> {
        &self.subject
    }

    pub fn predicate(&self) -> &PatternSlot<T> {
        &self.predicate
    }

    pub fn object(&self) -> &PatternSlot<T> {
        &self.object
    }

    /// The slots in order: graph, subject, predicate, object.
    pub fn slots(&self) -> [&PatternSlot<T>; 4] {
        [&self.graph, &self.subject, &self.predicate, &self.object]
    }

    /// The number of parameter slots.
    pub fn parameters(&self) -> usize {
        self.slots()
            .iter()
            .filter(|slot| matches!(slot, PatternSlot::Parameter(_)))
            .count()
    }

    /// The type of each slot's value: a node at the graph, subject and predicate, a term at the object.
    pub fn slot_types() -> [ColumnType; 4] {
        [
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Term,
        ]
    }
}

impl<T: AsRef<str>> QuadPattern<T> {
    /// The pattern with its terms as owned strings.
    pub fn to_owned_pattern(&self) -> QuadPattern<String> {
        let owned = |slot: &PatternSlot<T>| match slot {
            PatternSlot::Unbound => PatternSlot::Unbound,
            PatternSlot::DefaultGraph => PatternSlot::DefaultGraph,
            PatternSlot::Bound(text) => PatternSlot::Bound(text.as_ref().to_string()),
            PatternSlot::Parameter(n) => PatternSlot::Parameter(*n),
        };
        QuadPattern::new(
            owned(&self.graph),
            owned(&self.subject),
            owned(&self.predicate),
            owned(&self.object),
        )
    }

    /// The 32 bytes of its slots, its bound terms appended to `terms`.
    pub(crate) fn write(&self, terms: &mut StringWriter) -> Vec<u8> {
        self.slots()
            .iter()
            .flat_map(|slot| {
                let (kind, value) = match slot {
                    PatternSlot::Unbound => (0u32, 0u32),
                    PatternSlot::DefaultGraph => (1, 0),
                    PatternSlot::Bound(text) => (2, terms.push(text.as_ref())),
                    PatternSlot::Parameter(n) => (3, *n),
                };
                kind.to_le_bytes().into_iter().chain(value.to_le_bytes())
            })
            .collect()
    }
}

impl<'a> QuadPattern<&'a str> {
    /// The pattern of `slots`, section `sections.0` of a body of `command`, its terms in `terms` (whose sections are
    /// `sections.1` and `sections.2`), checked: 32 bytes ([`ProtocolError::CountMismatch`]); each kind known, the
    /// default graph only at the graph, a parameter only where `parameters` allows it, numbered in order of
    /// appearance, and an unused value zero ([`ProtocolError::FieldValue`]); each bound term's index in range
    /// ([`ProtocolError::IndexOutOfRange`]).
    pub(crate) fn read(
        command: u16,
        sections: (usize, usize, usize),
        slots: &[u8],
        terms: &StringList<'a>,
        parameters: bool,
    ) -> Result<Self, ProtocolError> {
        if slots.len() != PATTERN_SIZE {
            return Err(ProtocolError::CountMismatch(
                command,
                "pattern",
                slots.len() as u64,
                PATTERN_SIZE as u64,
            ));
        }
        let mut read = [PatternSlot::Unbound; 4];
        let (mut bound, mut parameters_seen) = (0u32, 0u32);
        for (i, out) in read.iter_mut().enumerate() {
            let kind = bytes_at(slots, 8 * i).map_or(0, u32::from_le_bytes);
            let value = bytes_at(slots, 8 * i + 4).map_or(0, u32::from_le_bytes);
            *out = match kind {
                0 | 1 if value != 0 => {
                    return Err(ProtocolError::FieldValue(
                        command,
                        "slot value",
                        value.into(),
                    ));
                }
                0 => PatternSlot::Unbound,
                1 if i != 0 => {
                    return Err(ProtocolError::FieldValue(
                        command,
                        "default graph",
                        i as u64,
                    ));
                }
                1 => PatternSlot::DefaultGraph,
                2 => {
                    let limit = as_u32(terms.len());
                    let text = terms
                        .get(value as usize)
                        .ok_or(ProtocolError::IndexOutOfRange(
                            command, sections.0, i, value, limit,
                        ))?;
                    if value != bound {
                        return Err(ProtocolError::FieldValue(
                            command,
                            "term index",
                            value.into(),
                        ));
                    }
                    bound += 1;
                    PatternSlot::Bound(text)
                }
                3 if !parameters => {
                    return Err(ProtocolError::FieldValue(
                        command,
                        "parameter slot",
                        i as u64,
                    ));
                }
                3 => {
                    if value != parameters_seen {
                        return Err(ProtocolError::FieldValue(
                            command,
                            "parameter",
                            value.into(),
                        ));
                    }
                    parameters_seen += 1;
                    PatternSlot::Parameter(value)
                }
                other => {
                    return Err(ProtocolError::FieldValue(
                        command,
                        "slot kind",
                        other.into(),
                    ));
                }
            };
        }
        if bound as usize != terms.len() {
            return Err(ProtocolError::CountMismatch(
                command,
                "pattern terms",
                terms.len() as u64,
                bound.into(),
            ));
        }
        let [graph, subject, predicate, object] = read;
        Ok(QuadPattern::new(graph, subject, predicate, object))
    }
}
