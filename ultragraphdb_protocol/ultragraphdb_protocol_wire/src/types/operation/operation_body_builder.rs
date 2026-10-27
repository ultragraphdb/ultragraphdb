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
        let graph = |g: &Option<u32>| g.unwrap_or(ABSENT);
        let mut args = Vec::new();
        match self.operation() {
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
        }
        let (records, text) = self.terms().sections();
        write_sections(&[], &[&u32_bytes(&args), &records, &text])
    }
}
