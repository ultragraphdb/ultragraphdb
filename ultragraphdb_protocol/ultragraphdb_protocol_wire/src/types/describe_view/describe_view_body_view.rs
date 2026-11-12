/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{StringList, as_u32};
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::declared::Declared;
use crate::types::describe::Describe;
use crate::types::describe_entry_view::DescribeEntryView;
use crate::types::describe_view::DescribeView;
use crate::types::entry_kind::EntryKind;
use crate::types::frame_view::FrameView;
use crate::types::retired_name_view::RetiredNameView;
use crate::utils::body::{read_sections, read_string_list, records, u32s};
use crate::utils::changes::{RecordKinds, check_records, decode_declaration};
use crate::utils::command_header::{CommandHeader, expect_command};
use crate::utils::constants::DESCRIBE_ENTRY_SIZE;
use crate::utils::fields::count;
use crate::utils::le::bytes_at;

impl<'a> BodyView<'a> for DescribeView<'a> {
    const COMMAND: Command = Command::Describe;

    type Body = Describe;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let header = expect_command(frame, Self::COMMAND)?;
        Self::read(header, frame.body())
    }

    fn to_body(&self) -> Result<Describe, ProtocolError> {
        let owned = |list: Vec<&str>| {
            list.into_iter()
                .map(str::to_string)
                .collect::<Vec<String>>()
        };
        let (mut predicates, mut entity_types, mut relations) =
            (Vec::new(), Vec::new(), Vec::new());
        for index in 0..self.declarations() {
            match decode_declaration(self.records, index, &self.args, &self.strings) {
                Some((Declared::Predicate(p), _)) => predicates.push(p),
                Some((Declared::EntityType(t), preorder)) => entity_types.push((t, preorder)),
                Some((Declared::Relation(r), _)) => relations.push(r),
                None => {}
            }
        }
        Ok(Describe::new(self.tenant, self.read_version)
            .with_labels(owned(self.labels().collect()))
            .with_relationship_types(owned(self.relationship_types().collect()))
            .with_property_keys(owned(self.property_keys().collect()))
            .with_predicates(predicates)
            .with_entity_types(entity_types)
            .with_relations(relations)
            .with_entries(self.entries().map(DescribeEntryView::to_entry).collect()))
    }
}

impl<'a> DescribeView<'a> {
    /// The answer of command header `header` and `body`, a frame's or the continuation frames' sections
    /// concatenated, checked in `PROTOCOL.md` §8.3's order: unused bytes 16–127 zero; the nine sections; the string
    /// list (0, 1); sections 2, 3 and 4, each whole u32s, each index below the string count; the args (6) whole u32s;
    /// the declarations (5); the entries (7, 8).
    pub(crate) fn read(header: CommandHeader<'_>, body: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        header.unused_zero(&[(0, 16)])?;
        let sections = read_sections::<9>(command, body, 0)?;
        let strings = read_string_list(command, (0, 1), sections.get(0), sections.get(1))?;
        let limit = count(command, "strings", strings.len())?;
        let names = |section: usize| -> Result<Vec<u32>, ProtocolError> {
            let indices = u32s(command, section, sections.get(section))?;
            match indices.iter().enumerate().find(|(_, i)| **i >= limit) {
                Some((at, index)) => Err(ProtocolError::index_out_of_range(
                    command, section, at, *index, limit,
                )),
                None => Ok(indices),
            }
        };
        let labels = names(2)?;
        let relationship_types = names(3)?;
        let property_keys = names(4)?;
        let args = u32s(command, 6, sections.get(6))?;
        check_records(
            command,
            (5, 6),
            sections.get(5),
            &args,
            0,
            &strings,
            RecordKinds::Declarations,
        )?;
        let (entries, entry_bytes) = (sections.get(7), sections.get(8));
        check_entries(
            command,
            sections.offset(7),
            (entries, entry_bytes),
            &strings,
        )?;
        Ok(Self {
            tenant: header.u64_at(0),
            read_version: header.u64_at(8),
            strings,
            labels,
            relationship_types,
            property_keys,
            records: sections.get(5),
            args,
            entries,
            entry_bytes,
        })
    }
}

/// One entry of section 7 as it lies: its kind's byte, its name's index, its offset and its length.
fn fields(record: &[u8]) -> (u8, u32, u32, u32) {
    let field = |at: usize| bytes_at(record, at).map_or(0, u32::from_le_bytes);
    let kind = record.first().copied().unwrap_or_default();
    (kind, field(4), field(8), field(12))
}

/// The entries of section 7, at body offset `offset`, over their bytes in section 8, checked in §8.3's order: whole
/// 16-byte entries ([`ProtocolError::section_length`]); each entry's bytes 1–3 zero ([`ProtocolError::padding_not_zero`]),
/// `kind` 1 to 20 (`"entry kind"`), `name` below the string count ([`ProtocolError::index_out_of_range`], section 7),
/// `offset` where the bytes before end (`"entry offset"`), its bytes within section 8 (`IndexOutOfRange`, section 8),
/// after the entry before by kind, then by name (`"entry order"`), a source's text UTF-8 ([`ProtocolError::not_utf8`],
/// section 8), a retired name's 24 bytes as §8.3 lays them out (`"retired name"`); then the entries' bytes ending at
/// section 8's end ([`ProtocolError::count_mismatch`] `"entry bytes"`).
fn check_entries(
    command: u16,
    offset: usize,
    (entries, bytes): (&[u8], &[u8]),
    strings: &StringList<'_>,
) -> Result<(), ProtocolError> {
    let whole = records(command, 7, entries, DESCRIBE_ENTRY_SIZE)?;
    let mut end = 0usize;
    let mut before: Option<(EntryKind, &str)> = None;
    for (at, record) in whole.enumerate() {
        if let Some(j) = record
            .get(1..4)
            .and_then(|zero| zero.iter().position(|b| *b != 0))
        {
            let start = offset.saturating_add(DESCRIBE_ENTRY_SIZE.saturating_mul(at));
            return Err(ProtocolError::padding_not_zero(
                command,
                start.saturating_add(1).saturating_add(j),
            ));
        }
        let (number, name, first, len) = fields(record);
        let refused = |field| ProtocolError::field_value(command, field, at as u64); // lossless: a `usize` is at most 64 bits
        let kind = EntryKind::from_number(number)
            .ok_or_else(|| ProtocolError::field_value(command, "entry kind", number.into()))?;
        // Lossless: a `usize` is at least 32 bits.
        let name = strings.get(name as usize).ok_or_else(|| {
            ProtocolError::index_out_of_range(command, 7, at, name, as_u32(strings.len()))
        })?;
        if first as usize != end {
            return Err(ProtocolError::field_value(
                command,
                "entry offset",
                first.into(),
            ));
        }
        let stop = end.saturating_add(len as usize);
        let held = bytes.get(end..stop).ok_or_else(|| {
            ProtocolError::index_out_of_range(command, 8, at, as_u32(stop), as_u32(bytes.len()))
        })?;
        if before.is_some_and(|(kind_before, name_before)| {
            (kind_before, name_before.as_bytes()) >= (kind, name.as_bytes())
        }) {
            return Err(refused("entry order"));
        }
        if kind == EntryKind::Source
            && let Err(error) = std::str::from_utf8(held)
        {
            return Err(ProtocolError::not_utf8(
                command,
                8,
                end.saturating_add(error.valid_up_to()),
            ));
        }
        // Lossless: a `usize` is at least 32 bits.
        let current = |index: u32| strings.get(index as usize);
        if kind == EntryKind::RetiredName && RetiredNameView::read(held, current).is_none() {
            return Err(refused("retired name"));
        }
        before = Some((kind, name));
        end = stop;
    }
    if end != bytes.len() {
        // The casts are lossless: a `usize` is at most 64 bits.
        return Err(ProtocolError::count_mismatch(
            command,
            "entry bytes",
            end as u64,
            bytes.len() as u64,
        ));
    }
    Ok(())
}
