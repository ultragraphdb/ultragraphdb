/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::declared::Declared;
use crate::types::describe::Describe;
use crate::utils::changes::ChangeWriter;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::u32_bytes;
use crate::utils::sections::write_sections;

impl BodyBuilder for Describe {
    const COMMAND: Command = Command::Describe;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.namespace().to_le_bytes()),
            (8, &self.read_version().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut writer = ChangeWriter::default();
        let names = |list: &[String], writer: &mut ChangeWriter| -> Vec<u8> {
            let indices: Vec<u32> = list.iter().map(|s| writer.names.intern(s)).collect();
            u32_bytes(&indices)
        };
        let labels = names(self.labels(), &mut writer);
        let relationship_types = names(self.relationship_types(), &mut writer);
        let property_keys = names(self.property_keys(), &mut writer);
        for predicate in self.predicates() {
            writer.declaration(&Declared::Predicate(predicate.clone()), 0);
        }
        for (entity_type, preorder) in self.entity_types() {
            writer.declaration(&Declared::EntityType(entity_type.clone()), *preorder);
        }
        for relation in self.relations() {
            writer.declaration(&Declared::Relation(relation.clone()), 0);
        }
        let (ends, text) = writer.names.sections();
        write_sections(
            &[],
            &[
                &ends,
                &text,
                &labels,
                &relationship_types,
                &property_keys,
                &writer.records,
                &u32_bytes(&writer.args),
            ],
        )
    }
}
