/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::declare::Declare;
use crate::types::declare_view::DeclareView;
use crate::types::drop_kind::DropKind;
use crate::types::frame_view::FrameView;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::body::{read_sections, read_string_list, u32s};
use crate::utils::changes::{CHANGE_RECORD_SIZE, RecordKinds, check_records};
use crate::utils::command_header::{CommandHeader, expect_command};
use crate::utils::constants::VERSION_SEQUENCE_MASK;
use crate::utils::term_kind::term_kind;

/// A Declare's number, which its refusals name.
fn command() -> u16 {
    Command::Declare.number()
}

/// The record kinds of a declaration: a predicate's 8 to a relation's 10 (`PROTOCOL.md` §4.6).
const DECLARATION_KINDS: std::ops::RangeInclusive<u8> = 8..=10;

/// The command header's fields, checked.
#[derive(Debug, Clone, Copy)]
struct Fields {
    drop: Option<DropKind>,
    tenant: u64,
    built_on: u64,
    principal: u64,
    version: u64,
    commit_version: u64,
}

/// `header` checked in the protocol's order: unused bytes 2–7 and 48–127 zero; `kind`; `drop`; `built_on`;
/// `commit_version`.
fn fields(header: CommandHeader<'_>) -> Result<Fields, ProtocolError> {
    let refused =
        |field: &'static str, value: u64| Err(ProtocolError::field_value(command(), field, value));
    header.unused_zero(&[(0, 2), (8, 48)])?;
    let (kind, drop_number) = (header.u8_at(0), header.u8_at(1));
    let drop = match kind {
        1 if drop_number == 0 => None,
        2 => match DropKind::from_number(drop_number) {
            Some(drop) => Some(drop),
            None => return refused("drop", drop_number.into()),
        },
        1 => return refused("drop", drop_number.into()),
        _ => return refused("kind", kind.into()),
    };
    let (built_on, commit_version) = (header.u64_at(16), header.u64_at(40));
    if drop.is_some() && built_on != 0 {
        return refused("built_on", built_on);
    }
    // A Drop names no commit version, and a declaration's is the first version of an epoch.
    let within_an_epoch = commit_version & VERSION_SEQUENCE_MASK != 0;
    if within_an_epoch || (drop.is_some() && commit_version != 0) {
        return refused("commit_version", commit_version);
    }
    Ok(Fields {
        drop,
        tenant: header.u64_at(8),
        built_on,
        principal: header.u64_at(24),
        version: header.u64_at(32),
        commit_version,
    })
}

/// The records of a `kind` 1 body: at least one, each a declaration's.
fn check_declarations(records: &[u8]) -> Result<(), ProtocolError> {
    if records.is_empty() {
        return Err(ProtocolError::count_mismatch(
            command(),
            "declarations",
            0,
            1,
        ));
    }
    let (whole, _) = records.as_chunks::<CHANGE_RECORD_SIZE>();
    let other = whole
        .iter()
        .position(|record| !DECLARATION_KINDS.contains(&record[0]));
    // Lossless: a `usize` is at most 64 bits.
    other.map_or(Ok(()), |index| {
        Err(ProtocolError::field_value(
            command(),
            "declaration",
            index as u64,
        ))
    })
}

/// The target of a `kind` 2 body of `drop`: no record, the term table's entries one for a predicate or a graph and
/// none otherwise, that one an IRI.
fn drop_target<'a>(
    drop: DropKind,
    records: &[u8],
    terms: &TermTableView<'a>,
) -> Result<Option<&'a str>, ProtocolError> {
    // The casts are lossless: a `usize` is at most 64 bits.
    let count = (records.len() / CHANGE_RECORD_SIZE) as u64;
    if count != 0 {
        return Err(ProtocolError::count_mismatch(
            command(),
            "declarations",
            count,
            0,
        ));
    }
    let expected = usize::from(drop.names_target());
    if terms.len() != expected {
        return Err(ProtocolError::count_mismatch(
            command(),
            "drop terms",
            terms.len() as u64,
            expected as u64,
        ));
    }
    match terms.get(0) {
        None => Ok(None),
        Some(RequestTermView::Iri(iri)) => Ok(Some(iri)),
        Some(other) => Err(ProtocolError::field_value(
            command(),
            "drop term",
            term_kind(&other).into(),
        )),
    }
}

impl<'a> DeclareView<'a> {
    /// The body of `header` and `body`, checked as [`BodyView::from_frame`] states.
    pub(crate) fn read(header: CommandHeader<'a>, body: &'a [u8]) -> Result<Self, ProtocolError> {
        let fields = fields(header)?;
        let sections = read_sections::<6>(command(), body, 0)?;
        let terms = TermTableView::read(
            command(),
            (2, 3),
            sections.offset(2),
            sections.get(2),
            sections.get(3),
        )?;
        let names = read_string_list(command(), (4, 5), sections.get(4), sections.get(5))?;
        let args = u32s(command(), 1, sections.get(1))?;
        let records = sections.get(0);
        let kinds = RecordKinds::Every;
        check_records(
            command(),
            (0, 1),
            records,
            &args,
            terms.len(),
            &names,
            kinds,
        )?;
        let target = match fields.drop {
            Some(drop) => drop_target(drop, records, &terms)?,
            None => {
                check_declarations(records)?;
                // A reply names its version and its commit version both, and a request neither.
                if (fields.version == 0) != (fields.commit_version == 0) {
                    return Err(ProtocolError::field_value(
                        command(),
                        "version",
                        fields.version,
                    ));
                }
                None
            }
        };
        Ok(Self {
            tenant: fields.tenant,
            built_on: fields.built_on,
            principal: fields.principal,
            version: fields.version,
            commit_version: fields.commit_version,
            drop: fields.drop,
            target,
            records,
            args,
            names,
        })
    }
}

impl<'a> BodyView<'a> for DeclareView<'a> {
    const COMMAND: Command = Command::Declare;

    type Body = Declare;

    /// Checks, in `PROTOCOL.md` §13.27's order: unused bytes 2–7 and 48–127 zero; `kind` 1 or 2; `drop` 0 for `kind` 1
    /// and 1 to 4 for `kind` 2; `built_on` 0 for `kind` 2; `commit_version` 0 for `kind` 2 and its low 22 bits zero
    /// for `kind` 1, each as [`ProtocolError::field_value`] naming its field; the sections as a Write's are checked
    /// (§7.2); for `kind` 1 at least one record ([`ProtocolError::count_mismatch`] `"declarations"`), each of kind 8
    /// to 10 ([`ProtocolError::field_value`] `"declaration"`, the record's index); for `kind` 2 no record
    /// (`"declarations"`), the term table's entries one for `drop` 1 or 2 and none for 3 or 4
    /// ([`ProtocolError::count_mismatch`] `"drop terms"`), that one an IRI ([`ProtocolError::field_value`]
    /// `"drop term"`, its kind); in a `kind` 1 reply `version` and `commit_version` at least 1
    /// ([`ProtocolError::field_value`] `"version"`).
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(expect_command(frame, Self::COMMAND)?, frame.body())
    }

    fn to_body(&self) -> Result<Declare, ProtocolError> {
        let request = match self.drop {
            Some(drop) => Declare::drop(
                self.tenant,
                self.principal,
                drop,
                self.target.map(str::to_string),
            ),
            None => Declare::declarations(
                self.tenant,
                self.built_on,
                self.principal,
                (0..self.len())
                    .filter_map(|index| self.declaration(index))
                    .collect(),
            ),
        };
        Ok(if self.is_request() {
            request
        } else {
            request.with_version(self.version, self.commit_version)
        })
    }
}
