/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Frames of the bodies stage 5.8 adds, laid out by hand from the tables of `PROTOCOL.md` §13.23 to §13.28,
//! independently of the builders under test, with the refusals a view gives them written out field by field.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::Command;
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::term_records::{Entry, term_sections};
use crate::utils::ugs_frames::{frame, strings, u32s};

/// The session `generate_roles.py` names `SESSION`: bytes `0x11` to `0x20`.
pub const SESSION_A: [u8; 16] = [
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
];

/// The session `generate_roles.py` names `SESSION_B`: bytes `0x31` to `0x40`.
pub const SESSION_B: [u8; 16] = [
    0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40,
];

/// The session `generate_roles.py` takes from `TRANSACTION`: bytes `0xA1` to `0xB0`.
pub const SESSION_C: [u8; 16] = [
    0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF, 0xB0,
];

/// The bytes of the golden role frame `name`.
///
/// # Errors
///
/// When no golden role frame has the name, or its file cannot be read.
pub fn role(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// `FieldValue` of `command` naming `field` with `value`.
pub fn field(command: u16, field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command,
        field,
        value,
    }
}

/// `CountMismatch` of `command` naming `field`.
pub fn count(command: u16, field: &'static str, count: u64, expected: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::CountMismatch {
        command,
        field,
        count,
        expected,
    }
}

/// `ReservedNotZero` at the frame offset of command-header byte `at`.
pub fn reserved(at: usize) -> ProtocolErrorEnum {
    ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
}

/// A session of 15 zero bytes and then `last`.
pub fn session_ending(last: u8) -> [u8; 16] {
    let mut session = [0u8; 16];
    session[15] = last;
    session
}

/// One check's 32 bytes (§13.24): `session`, `principal`, `kind`, `outcome`, 6 zero bytes.
pub fn check_record(session: [u8; 16], principal: u64, kind: u8, outcome: u8) -> Vec<u8> {
    let mut record = session.to_vec();
    record.extend_from_slice(&principal.to_le_bytes());
    record.extend_from_slice(&[kind, outcome, 0, 0, 0, 0, 0, 0]);
    record
}

/// A Check frame whose section 0 is `section`, its command header all zero.
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn check_frame(section: Vec<u8>) -> TestResult<Vec<u8>> {
    frame(Command::Check, &[], &[section], false)
}

/// An Ended frame whose section 0 is `section`, its command header all zero, flagged `LAST`.
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn ended_frame(section: Vec<u8>) -> TestResult<Vec<u8>> {
    frame(Command::Ended, &[], &[section], true)
}

/// A Session frame of `session` and `principal` (§13.23).
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn session_frame(session: [u8; 16], principal: u64) -> TestResult<Vec<u8>> {
    frame(
        Command::Session,
        &[(0, &session), (16, &principal.to_le_bytes())],
        &[],
        false,
    )
}

/// A Declare's command header fields (§13.27).
#[derive(Debug, Clone, Copy, Default)]
pub struct DeclareFields {
    /// `kind`, at 0.
    pub kind: u8,
    /// `drop`, at 1.
    pub drop: u8,
    /// `tenant`, at 8.
    pub tenant: u64,
    /// `built_on`, at 16.
    pub built_on: u64,
    /// `principal`, at 24.
    pub principal: u64,
    /// `version`, at 32.
    pub version: u64,
    /// `commit_version`, at 40.
    pub commit_version: u64,
}

/// A Declare frame of `fields` and the six `sections` of a Write (§7.2).
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn declare_frame(fields: DeclareFields, sections: &[Vec<u8>]) -> TestResult<Vec<u8>> {
    frame(
        Command::Declare,
        &[
            (0, &[fields.kind, fields.drop]),
            (8, &fields.tenant.to_le_bytes()),
            (16, &fields.built_on.to_le_bytes()),
            (24, &fields.principal.to_le_bytes()),
            (32, &fields.version.to_le_bytes()),
            (40, &fields.commit_version.to_le_bytes()),
        ],
        sections,
        false,
    )
}

/// One change record's 16 bytes (§4.6): `kind`, `flags`, 2 zero bytes, `args_start`, `args_len`, `extra` 0.
pub fn change_record(kind: u8, flags: u8, args_start: u32, args_len: u32) -> Vec<u8> {
    let mut record = vec![kind, flags, 0, 0];
    record.extend_from_slice(&u32s(&[args_start, args_len, 0]));
    record
}

/// The six sections of a Declare of `kind` 1 holding entity types named `types`, each with no parent: one record of
/// kind 9 per type over its two args, its name's index and `ABSENT`.
pub fn entity_type_sections(types: &[&str]) -> Vec<Vec<u8>> {
    let mut records = Vec::new();
    let mut args = Vec::new();
    for (index, _) in (0u32..).zip(types) {
        records.extend_from_slice(&change_record(9, 0, index * 2, 2));
        args.extend_from_slice(&[index, u32::MAX]);
    }
    let [ends, text] = strings(types);
    vec![records, u32s(&args), vec![], vec![], ends, text]
}

/// The six sections of a Declare of `kind` 2 whose term table holds `entries` and nothing else.
pub fn drop_sections(entries: &[Entry<'_>]) -> Vec<Vec<u8>> {
    let (records, text) = term_sections(entries);
    vec![vec![], vec![], records, text, vec![], vec![]]
}

/// A Defaults frame of `tenant` and `position` whose term table holds `entries` (§13.28), flagged `LAST` when `last`.
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn defaults_frame(
    tenant: u64,
    position: u64,
    entries: &[Entry<'_>],
    last: bool,
) -> TestResult<Vec<u8>> {
    let (records, text) = term_sections(entries);
    frame(
        Command::Defaults,
        &[(0, &tenant.to_le_bytes()), (8, &position.to_le_bytes())],
        &[records, text],
        last,
    )
}

/// The IRIs `iris` as term-table entries of kind 1.
pub fn iris<'s>(iris: &[&'s str]) -> Vec<Entry<'s>> {
    iris.iter().map(|iri| Entry::Text(1, iri, "")).collect()
}
