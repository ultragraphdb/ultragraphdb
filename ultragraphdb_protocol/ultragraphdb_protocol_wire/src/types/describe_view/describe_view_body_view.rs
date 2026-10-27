/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::describe::Describe;
use crate::types::describe_view::DescribeView;
use crate::types::frame_view::FrameView;
use crate::types::string_list::StringList;
use crate::utils::changes::{check_records, decode_declaration};
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::{as_u32, u32s};
use ultragraphdb_core_mapping::Declaration;

impl<'a> BodyView<'a> for DescribeView<'a> {
    const COMMAND: Command = Command::Describe;

    type Body = Describe;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 16)])?;
        let sections = Sections::<7>::read(command, frame.body(), 0)?;
        let strings = StringList::read(command, (0, 1), sections.get(0), sections.get(1))?;
        let limit = as_u32(strings.len());
        let names = |section: usize| -> Result<Vec<u32>, ProtocolError> {
            let indices = u32s(command, section, sections.get(section))?;
            match indices.iter().enumerate().find(|(_, i)| **i >= limit) {
                Some((at, index)) => Err(ProtocolError::IndexOutOfRange(
                    command, section, at, *index, limit,
                )),
                None => Ok(indices),
            }
        };
        let labels = names(2)?;
        let relationship_types = names(3)?;
        let property_keys = names(4)?;
        let args = u32s(command, 6, sections.get(6))?;
        check_records(command, (5, 6, 1), sections.get(5), &args, &strings, true)?;
        Ok(Self {
            namespace: header.u64_at(0),
            read_version: header.u64_at(8),
            strings,
            labels,
            relationship_types,
            property_keys,
            records: sections.get(5),
            args,
        })
    }

    fn to_body(&self) -> Result<Describe, ProtocolError> {
        let command = Self::COMMAND.number();
        let owned = |list: Vec<&str>| {
            list.into_iter()
                .map(str::to_string)
                .collect::<Vec<String>>()
        };
        let (mut predicates, mut entity_types, mut relations) =
            (Vec::new(), Vec::new(), Vec::new());
        for index in 0..self.declarations() {
            match decode_declaration(
                command,
                (5, 6, 1),
                self.records,
                index,
                &self.args,
                &self.strings,
            )? {
                (Declaration::Predicate(p), _) => predicates.push(p),
                (Declaration::EntityType(t), preorder) => entity_types.push((t, preorder)),
                (Declaration::Relation(r), _) => relations.push(r),
            }
        }
        Ok(Describe::new(self.namespace, self.read_version)
            .with_labels(owned(self.labels().collect()))
            .with_relationship_types(owned(self.relationship_types().collect()))
            .with_property_keys(owned(self.property_keys().collect()))
            .with_predicates(predicates)
            .with_entity_types(entity_types)
            .with_relations(relations))
    }
}
