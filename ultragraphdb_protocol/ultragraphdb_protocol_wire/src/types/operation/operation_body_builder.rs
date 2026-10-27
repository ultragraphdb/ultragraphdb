/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::commit_operation::CommitOperation;
use crate::types::operation::Operation;
use crate::utils::command_header::command_header;
use crate::utils::constants::ABSENT;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;
use crate::utils::sections::{as_u32, u32_bytes};
use crate::utils::string_writer::StringWriter;
use crate::utils::terms::graph_text;
use oxrdf::GraphName;

impl BodyBuilder for Operation {
    const COMMAND: Command = Command::Operation;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        let (kind, delta, properties, links) = match self.operation() {
            CommitOperation::InsertIfAbsent { properties, .. } => (1u8, 0i64, properties.len(), 0),
            CommitOperation::UpsertAndLink {
                properties, links, ..
            } => (2, 0, properties.len(), links.len()),
            CommitOperation::CompareAndSet { .. } => (3, 0, 0, 0),
            CommitOperation::CounterAdd { delta, .. } => (4, *delta, 0, 0),
        };
        command_header(&[
            (0, &[kind]),
            (8, &delta.to_le_bytes()),
            (16, &as_u32(properties).to_le_bytes()),
            (20, &as_u32(links).to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut strings = StringWriter::default();
        let mut args = Vec::new();
        let graph = |g: &GraphName, strings: &mut StringWriter| {
            graph_text(g).map_or(ABSENT, |text| strings.intern(&text))
        };
        match self.operation() {
            CommitOperation::InsertIfAbsent {
                graph: g,
                predicate,
                value,
                properties,
            }
            | CommitOperation::UpsertAndLink {
                graph: g,
                predicate,
                value,
                properties,
                ..
            } => {
                args.push(graph(g, &mut strings));
                args.push(strings.intern(&predicate.to_string()));
                args.push(strings.intern(&value.to_string()));
                for (property, v) in properties {
                    args.push(strings.intern(&property.to_string()));
                    args.push(strings.intern(&v.to_string()));
                }
                if let CommitOperation::UpsertAndLink { links, .. } = self.operation() {
                    for link in links {
                        args.push(strings.intern(&link.predicate().to_string()));
                        args.push(strings.intern(&link.other().to_string()));
                        args.push(u32::from(!link.node_is_subject()));
                    }
                }
            }
            CommitOperation::CompareAndSet {
                graph: g,
                subject,
                predicate,
                expected,
                new,
            } => {
                args.push(graph(g, &mut strings));
                args.push(strings.intern(&subject.to_string()));
                args.push(strings.intern(&predicate.to_string()));
                args.push(strings.intern(&expected.to_string()));
                args.push(strings.intern(&new.to_string()));
            }
            CommitOperation::CounterAdd {
                graph: g,
                subject,
                predicate,
                ..
            } => {
                args.push(graph(g, &mut strings));
                args.push(strings.intern(&subject.to_string()));
                args.push(strings.intern(&predicate.to_string()));
            }
        }
        let (ends, text) = strings.sections();
        write_sections(&[], &[&u32_bytes(&args), &ends, &text])
    }
}
