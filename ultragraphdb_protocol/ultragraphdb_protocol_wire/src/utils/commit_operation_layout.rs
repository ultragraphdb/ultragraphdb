/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A commit-time operation's layout (`PROTOCOL.md` §7.3): its 24 bytes of fields, an Operation's command header bytes 0
//! to 23 and a ConditionalWrite's section 6, and its args, as an Operation's section 0 and a ConditionalWrite's section 1
//! hold them. Bytes: `kind` u8, 7 zero bytes, `delta` i64, `properties` u32, `links` u32.

use ultragraphdb_core_body::as_u32;
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::commit_operation::CommitOperation;
use crate::types::link::Link;
use crate::utils::constants::ABSENT;
use crate::utils::le::bytes_at;

/// The size of an operation's fields.
pub(crate) const OPERATION_FIELDS_SIZE: usize = 24;

/// An operation's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OperationFields {
    pub(crate) kind: u8,
    pub(crate) delta: i64,
    pub(crate) properties: u32,
    pub(crate) links: u32,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<OperationFields>() <= 64);

impl OperationFields {
    /// The fields of `operation`.
    pub(crate) fn of(operation: &CommitOperation) -> Self {
        let (kind, delta, properties, links) = match operation {
            CommitOperation::InsertIfAbsent { properties, .. } => (1, 0, properties.len(), 0),
            CommitOperation::UpsertAndLink {
                properties, links, ..
            } => (2, 0, properties.len(), links.len()),
            CommitOperation::CompareAndSet { .. } => (3, 0, 0, 0),
            CommitOperation::CounterAdd { delta, .. } => (4, *delta, 0, 0),
            CommitOperation::DeleteInstance { .. } => (5, 0, 0, 0),
            CommitOperation::RetractInstance { .. } => (6, 0, 0, 0),
        };
        Self {
            kind,
            delta,
            properties: as_u32(properties),
            links: as_u32(links),
        }
    }

    /// The fields' 24 bytes.
    pub(crate) fn encode(self) -> [u8; OPERATION_FIELDS_SIZE] {
        let mut out = [0; OPERATION_FIELDS_SIZE];
        let fields = [self.kind]
            .into_iter()
            .chain([0; 7])
            .chain(self.delta.to_le_bytes())
            .chain(self.properties.to_le_bytes())
            .chain(self.links.to_le_bytes());
        out.iter_mut()
            .zip(fields)
            .for_each(|(byte, field)| *byte = field);
        out
    }

    /// The fields at the start of `bytes`, unchecked.
    pub(crate) fn read(bytes: &[u8]) -> Self {
        Self {
            kind: bytes.first().copied().unwrap_or(0),
            // The delta is an i64 carried in the u64 field's bits, two's complement.
            delta: bytes_at(bytes, 8).map_or(0, i64::from_le_bytes),
            properties: bytes_at(bytes, 16).map_or(0, u32::from_le_bytes),
            links: bytes_at(bytes, 20).map_or(0, u32::from_le_bytes),
        }
    }

    /// Refused as [`ProtocolError::field_value`], in order: a `kind` other than 1 to 6, then a `delta`, `properties` or
    /// `links` the kind does not take.
    pub(crate) fn check(self, command: u16) -> Result<(), ProtocolError> {
        if !(1..=6).contains(&self.kind) {
            return Err(ProtocolError::field_value(
                command,
                "kind",
                self.kind.into(),
            ));
        }
        if self.delta != 0 && self.kind != 4 {
            return Err(ProtocolError::field_value(
                command,
                "delta",
                // The field's eight bytes as the refusal names a value: the delta's two's complement bits.
                self.delta.cast_unsigned(),
            ));
        }
        if self.properties != 0 && self.kind > 2 {
            return Err(ProtocolError::field_value(
                command,
                "properties",
                self.properties.into(),
            ));
        }
        if self.links != 0 && self.kind != 2 {
            return Err(ProtocolError::field_value(
                command,
                "links",
                self.links.into(),
            ));
        }
        Ok(())
    }

    /// The number of args a checked operation takes.
    pub(crate) fn arg_count(self) -> u64 {
        match self.kind {
            // Counts of a u32 field each, so the sum stays within a u64 and never saturates.
            1 | 2 => u64::from(self.properties)
                .saturating_mul(2)
                .saturating_add(u64::from(self.links).saturating_mul(3))
                .saturating_add(3),
            3 => 5,
            4 => 3,
            // Kinds 5 and 6: the graph and the instance.
            _ => 2,
        }
    }

    /// Refuses an arg of `args` that is neither a link direction of 0 or 1 nor an index below `limit` into the term
    /// table; the first arg may also be [`ABSENT`]. `place` is the args' section and where `args` start in it.
    pub(crate) fn check_args(
        self,
        command: u16,
        place: (usize, usize),
        args: &[u32],
        limit: u32,
    ) -> Result<(), ProtocolError> {
        // Lossless: a `usize` is at least 32 bits.
        let link_start = (self.properties as usize)
            .checked_mul(2)
            .and_then(|properties| properties.checked_add(3))
            .ok_or_else(|| {
                ProtocolError::field_value(command, "properties", self.properties.into())
            })?;
        for (at, arg) in args.iter().enumerate() {
            let direction = self.kind == 2
                && at
                    .checked_sub(link_start)
                    .is_some_and(|in_links| in_links % 3 == 2);
            if direction {
                if *arg > 1 {
                    return Err(ProtocolError::field_value(
                        command,
                        "link direction",
                        (*arg).into(),
                    ));
                }
            } else if !(at == 0 && *arg == ABSENT) && *arg >= limit {
                return Err(ProtocolError::index_out_of_range(
                    command,
                    place.0,
                    place.1.saturating_add(at),
                    *arg,
                    limit,
                ));
            }
        }
        Ok(())
    }

    /// The operation checked fields and `args` describe.
    pub(crate) fn operation(self, args: &[u32]) -> CommitOperation {
        let arg = |i: usize| args.get(i).copied().unwrap_or(0);
        let graph = (arg(0) != ABSENT).then(|| arg(0));
        // After the graph, the predicate and the value: the property pairs, then the links' triples, read in order; an
        // arg past the last reads 0.
        let mut rest = args.iter().copied().skip(3);
        let mut next = || rest.next().unwrap_or(0);
        match self.kind {
            1 => CommitOperation::InsertIfAbsent {
                graph,
                predicate: arg(1),
                value: arg(2),
                properties: (0..self.properties).map(|_| (next(), next())).collect(),
            },
            2 => {
                let properties = (0..self.properties).map(|_| (next(), next())).collect();
                let links = (0..self.links)
                    .map(|_| {
                        let (node, other, direction) = (next(), next(), next());
                        if direction == 0 {
                            Link::from_node(node, other)
                        } else {
                            Link::to_node(node, other)
                        }
                    })
                    .collect();
                CommitOperation::UpsertAndLink {
                    graph,
                    predicate: arg(1),
                    value: arg(2),
                    properties,
                    links,
                }
            }
            3 => CommitOperation::CompareAndSet {
                graph,
                subject: arg(1),
                predicate: arg(2),
                expected: arg(3),
                new: arg(4),
            },
            4 => CommitOperation::CounterAdd {
                graph,
                subject: arg(1),
                predicate: arg(2),
                delta: self.delta,
            },
            5 => CommitOperation::DeleteInstance {
                graph,
                instance: arg(1),
            },
            _ => CommitOperation::RetractInstance {
                graph,
                instance: arg(1),
            },
        }
    }
}

/// Appends `operation`'s args to `args`.
pub(crate) fn push_args(operation: &CommitOperation, args: &mut Vec<u32>) {
    let graph = |g: &Option<u32>| g.unwrap_or(ABSENT);
    match operation {
        CommitOperation::InsertIfAbsent {
            graph: g,
            predicate,
            value,
            properties,
        } => {
            args.extend([graph(g), *predicate, *value]);
            properties.iter().for_each(|(p, v)| args.extend([*p, *v]));
        }
        CommitOperation::UpsertAndLink {
            graph: g,
            predicate,
            value,
            properties,
            links,
        } => {
            args.extend([graph(g), *predicate, *value]);
            properties.iter().for_each(|(p, v)| args.extend([*p, *v]));
            links.iter().for_each(|l| {
                args.extend([l.predicate(), l.other(), u32::from(!l.node_is_subject())]);
            });
        }
        CommitOperation::CompareAndSet {
            graph: g,
            subject,
            predicate,
            expected,
            new,
        } => args.extend([graph(g), *subject, *predicate, *expected, *new]),
        CommitOperation::CounterAdd {
            graph: g,
            subject,
            predicate,
            ..
        } => args.extend([graph(g), *subject, *predicate]),
        CommitOperation::DeleteInstance { graph: g, instance }
        | CommitOperation::RetractInstance { graph: g, instance } => {
            args.extend([graph(g), *instance]);
        }
    }
}

/// `bytes`, a ConditionalWrite's whole operations at body offset `offset`, checked in order against `args` from arg
/// `first`, where the records ended: bytes 1 to 7 zero ([`ProtocolError::padding_not_zero`] at the body offset);
/// [`OperationFields::check`]; [`OperationFields::check_args`] over its count of args, within section `args_section`
/// and below `limit`. An operation running past the args, or args left after the last, is refused as
/// [`ProtocolError::field_value`] `"args_start"`, naming the arg they begin at. The arg each operation starts at.
pub(crate) fn consume_operations(
    command: u16,
    args_section: usize,
    offset: usize,
    bytes: &[u8],
    args: &[u32],
    first: usize,
    limit: u32,
) -> Result<Vec<usize>, ProtocolError> {
    let mut next = first;
    let mut starts = Vec::with_capacity(bytes.len() / OPERATION_FIELDS_SIZE);
    let operations = bytes.as_chunks::<OPERATION_FIELDS_SIZE>().0;
    for (at, operation) in operations.iter().enumerate() {
        if let Some(j) = operation
            .get(1..8)
            .and_then(|zero| zero.iter().position(|b| *b != 0))
        {
            let start = offset.saturating_add(OPERATION_FIELDS_SIZE.saturating_mul(at));
            return Err(ProtocolError::padding_not_zero(
                command,
                start.saturating_add(1).saturating_add(j),
            ));
        }
        let fields = OperationFields::read(operation);
        fields.check(command)?;
        let end = usize::try_from(fields.arg_count())
            .ok()
            .and_then(|count| next.checked_add(count));
        let Some((end, run)) = end.and_then(|end| Some((end, args.get(next..end)?))) else {
            return Err(ProtocolError::field_value(
                command,
                "args_start",
                next as u64, // lossless: a `usize` is at most 64 bits
            ));
        };
        fields.check_args(command, (args_section, next), run, limit)?;
        starts.push(next);
        next = end;
    }
    if next != args.len() {
        return Err(ProtocolError::field_value(
            command,
            "args_start",
            next as u64, // lossless: a `usize` is at most 64 bits
        ));
    }
    Ok(starts)
}
