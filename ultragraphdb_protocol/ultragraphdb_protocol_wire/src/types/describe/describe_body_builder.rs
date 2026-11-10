/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::Range;

use ultragraphdb_core_body::{as_u32, u32_bytes, write_sections};

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::declared::Declared;
use crate::types::describe::Describe;
use crate::types::entry_kind::EntryKind;
use crate::utils::changes::ChangeWriter;
use crate::utils::command_header::command_header;
use crate::utils::constants::{COMMAND_HEADER_SIZE, DESCRIBE_ENTRY_SIZE};

/// An answer's nine sections, and where section 8 holds a source's text, which a frame splits only on a character
/// boundary.
pub(super) struct Layout {
    pub(super) sections: [Vec<u8>; 9],
    pub(super) texts: Vec<Range<usize>>,
}

/// `describe`'s sections (`PROTOCOL.md` §8.3): the strings interned in the order the body uses them (§4.7), the labels,
/// relationship types and property keys, the declarations, each `extra` 0 (§4.6), then the entries' names and the
/// retired names' current names; the entries in sections 7 and 8, a retired name laid out with its current name's
/// index.
pub(super) fn layout(describe: &Describe) -> Layout {
    let mut writer = ChangeWriter::default();
    let names = |list: &[String], interner: &mut ChangeWriter| -> Vec<u8> {
        let indices: Vec<u32> = list.iter().map(|s| interner.names.intern(s)).collect();
        u32_bytes(&indices)
    };
    let labels = names(describe.labels(), &mut writer);
    let relationship_types = names(describe.relationship_types(), &mut writer);
    let property_keys = names(describe.property_keys(), &mut writer);
    for predicate in describe.predicates() {
        writer.declaration(&Declared::Predicate(predicate.clone()), 0);
    }
    // The preorder number stays beside the type in the owned answer; the record names no place in the hierarchy.
    for (entity_type, _preorder) in describe.entity_types() {
        writer.declaration(&Declared::EntityType(entity_type.clone()), 0);
    }
    for relation in describe.relations() {
        writer.declaration(&Declared::Relation(relation.clone()), 0);
    }
    let entries = describe.entries();
    let entry_names: Vec<u32> = entries
        .iter()
        .map(|entry| writer.names.intern(entry.name()))
        .collect();
    let mut records = Vec::with_capacity(DESCRIBE_ENTRY_SIZE * entries.len());
    let mut bytes = Vec::new();
    let mut texts = Vec::new();
    for (entry, name) in entries.iter().zip(entry_names) {
        let start = bytes.len();
        match entry.retired_name() {
            Some(retired) => {
                let current = retired
                    .current()
                    .map(|current| writer.names.intern(current));
                bytes.extend_from_slice(&retired.encode(current));
            }
            None => bytes.extend_from_slice(entry.bytes()),
        }
        if entry.kind() == EntryKind::Source {
            texts.push(start..bytes.len());
        }
        records.extend_from_slice(&[entry.kind().number(), 0, 0, 0]);
        for field in [name, as_u32(start), as_u32(bytes.len() - start)] {
            records.extend_from_slice(&field.to_le_bytes());
        }
    }
    let (ends, text) = writer.names.sections();
    Layout {
        sections: [
            ends,
            text,
            labels,
            relationship_types,
            property_keys,
            writer.records,
            u32_bytes(&writer.args),
            records,
            bytes,
        ],
        texts,
    }
}

impl BodyBuilder for Describe {
    const COMMAND: Command = Command::Describe;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.tenant().to_le_bytes()),
            (8, &self.read_version().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let layout = layout(self);
        let sections: Vec<&[u8]> = layout.sections.iter().map(Vec::as_slice).collect();
        write_sections(&[], &sections)
    }
}
