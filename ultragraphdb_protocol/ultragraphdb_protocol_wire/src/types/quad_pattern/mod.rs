/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::column_type::ColumnType;
use crate::types::pattern_slot::PatternSlot;
use crate::utils::le::bytes_at;
use crate::utils::sections::as_u32;

/// The size of a pattern on the wire: four slots of 8 bytes.
pub(crate) const PATTERN_SIZE: usize = 32;

/// A quad pattern: graph, subject, predicate and object, each a [`PatternSlot`], its bound terms indices into the
/// body's term table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuadPattern {
    graph: PatternSlot,
    subject: PatternSlot,
    predicate: PatternSlot,
    object: PatternSlot,
}

impl QuadPattern {
    pub fn new(
        graph: PatternSlot,
        subject: PatternSlot,
        predicate: PatternSlot,
        object: PatternSlot,
    ) -> Self {
        Self {
            graph,
            subject,
            predicate,
            object,
        }
    }

    pub fn graph(&self) -> PatternSlot {
        self.graph
    }

    pub fn subject(&self) -> PatternSlot {
        self.subject
    }

    pub fn predicate(&self) -> PatternSlot {
        self.predicate
    }

    pub fn object(&self) -> PatternSlot {
        self.object
    }

    /// The slots in order: graph, subject, predicate, object.
    pub fn slots(&self) -> [PatternSlot; 4] {
        [self.graph, self.subject, self.predicate, self.object]
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

    /// The 32 bytes of its slots.
    pub(crate) fn write(&self) -> Vec<u8> {
        self.slots()
            .iter()
            .flat_map(|slot| {
                let (kind, value) = match slot {
                    PatternSlot::Unbound => (0u32, 0u32),
                    PatternSlot::DefaultGraph => (1, 0),
                    PatternSlot::Bound(index) => (2, *index),
                    PatternSlot::Parameter(n) => (3, *n),
                };
                kind.to_le_bytes().into_iter().chain(value.to_le_bytes())
            })
            .collect()
    }

    /// The pattern of `slots`, section `section` of a body of `command` whose term table holds `terms` entries,
    /// checked: 32 bytes ([`ProtocolError::CountMismatch`]); each kind known, the default graph only at the graph, a
    /// parameter only where `parameters` allows it, numbered in order of appearance, and an unused value zero
    /// ([`ProtocolError::FieldValue`]); each bound term's index in the table ([`ProtocolError::IndexOutOfRange`]).
    pub(crate) fn read(
        command: u16,
        section: usize,
        slots: &[u8],
        terms: usize,
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
        let mut parameters_seen = 0u32;
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
                2 if value as usize >= terms => {
                    return Err(ProtocolError::IndexOutOfRange(
                        command,
                        section,
                        i,
                        value,
                        as_u32(terms),
                    ));
                }
                2 => PatternSlot::Bound(value),
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
        let [graph, subject, predicate, object] = read;
        Ok(QuadPattern::new(graph, subject, predicate, object))
    }
}
