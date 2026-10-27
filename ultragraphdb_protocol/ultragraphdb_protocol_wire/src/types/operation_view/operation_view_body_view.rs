/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::commit_operation::CommitOperation;
use crate::types::frame_view::FrameView;
use crate::types::link::Link;
use crate::types::operation::Operation;
use crate::types::operation_view::OperationView;
use crate::types::string_list::StringList;
use crate::utils::constants::ABSENT;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::{as_u32, u32s};
use crate::utils::terms::{parse_graph, parse_named, parse_subject, parse_term};
use oxrdf::{NamedNode, Term};

impl<'a> BodyView<'a> for OperationView<'a> {
    const COMMAND: Command = Command::Operation;

    type Body = Operation;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 1), (8, 24)])?;
        let kind = header.u8_at(0);
        let delta = header.u64_at(8) as i64;
        let properties = header.u32_at(16);
        let links = header.u32_at(20);
        if !(1..=4).contains(&kind) {
            return Err(ProtocolError::FieldValue(command, "kind", kind.into()));
        }
        if delta != 0 && kind != 4 {
            return Err(ProtocolError::FieldValue(command, "delta", delta as u64));
        }
        if properties != 0 && kind > 2 {
            return Err(ProtocolError::FieldValue(
                command,
                "properties",
                properties.into(),
            ));
        }
        if links != 0 && kind != 2 {
            return Err(ProtocolError::FieldValue(command, "links", links.into()));
        }
        let sections = Sections::<3>::read(command, frame.body(), 0)?;
        let strings = StringList::read(command, (1, 2), sections.get(1), sections.get(2))?;
        let args = u32s(command, 0, sections.get(0))?;
        let expected = match kind {
            1 | 2 => 3 + 2 * u64::from(properties) + 3 * u64::from(links),
            3 => 5,
            _ => 3,
        };
        if args.len() as u64 != expected {
            return Err(ProtocolError::CountMismatch(
                command,
                "args",
                args.len() as u64,
                expected,
            ));
        }
        let limit = as_u32(strings.len());
        let link_start = 3 + 2 * properties as usize;
        for (at, arg) in args.iter().enumerate() {
            let direction = kind == 2 && at >= link_start && (at - link_start) % 3 == 2;
            if direction {
                if *arg > 1 {
                    return Err(ProtocolError::FieldValue(
                        command,
                        "link direction",
                        (*arg).into(),
                    ));
                }
            } else if !(at == 0 && *arg == ABSENT) && *arg >= limit {
                return Err(ProtocolError::IndexOutOfRange(command, 0, at, *arg, limit));
            }
        }
        Ok(Self {
            kind,
            delta,
            properties,
            links,
            args,
            strings,
        })
    }

    fn to_body(&self) -> Result<Operation, ProtocolError> {
        let command = Self::COMMAND.number();
        let limit = as_u32(self.strings.len());
        let text = |i: usize| -> Result<(usize, &str), ProtocolError> {
            let index = self.args.get(i).copied().unwrap_or(ABSENT);
            self.strings
                .get(index as usize)
                .map(|s| (index as usize, s))
                .ok_or(ProtocolError::IndexOutOfRange(command, 0, i, index, limit))
        };
        let term = |i: usize| text(i).and_then(|(at, s)| parse_term(command, 2, at, s));
        let named = |i: usize| text(i).and_then(|(at, s)| parse_named(command, 2, at, s));
        let subject = |i: usize| text(i).and_then(|(at, s)| parse_subject(command, 2, at, s));
        let graph = if self.args.first() == Some(&ABSENT) {
            parse_graph(command, 2, 0, None)?
        } else {
            let (at, s) = text(0)?;
            parse_graph(command, 2, at, Some(s))?
        };
        let pairs = |from: usize, n: usize| -> Result<Vec<(NamedNode, Term)>, ProtocolError> {
            (0..n)
                .map(|k| Ok((named(from + 2 * k)?, term(from + 2 * k + 1)?)))
                .collect()
        };
        let properties = self.properties as usize;
        let operation = match self.kind {
            1 => CommitOperation::InsertIfAbsent {
                graph,
                predicate: named(1)?,
                value: term(2)?,
                properties: pairs(3, properties)?,
            },
            2 => {
                let start = 3 + 2 * properties;
                let links = (0..self.links as usize)
                    .map(|k| {
                        let at = start + 3 * k;
                        let (predicate, other) = (named(at)?, subject(at + 1)?);
                        Ok(if self.args.get(at + 2) == Some(&0) {
                            Link::from_node(predicate, other)
                        } else {
                            Link::to_node(predicate, other)
                        })
                    })
                    .collect::<Result<_, ProtocolError>>()?;
                CommitOperation::UpsertAndLink {
                    graph,
                    predicate: named(1)?,
                    value: term(2)?,
                    properties: pairs(3, properties)?,
                    links,
                }
            }
            3 => CommitOperation::CompareAndSet {
                graph,
                subject: subject(1)?,
                predicate: named(2)?,
                expected: term(3)?,
                new: term(4)?,
            },
            _ => CommitOperation::CounterAdd {
                graph,
                subject: subject(1)?,
                predicate: named(2)?,
                delta: self.delta,
            },
        };
        Ok(Operation::new(operation))
    }
}
