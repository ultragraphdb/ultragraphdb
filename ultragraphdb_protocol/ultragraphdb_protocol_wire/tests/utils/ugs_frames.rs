/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Frames of the bodies stage 6 adds or changes, laid out by hand from `PROTOCOL.md`'s tables, independently of the
//! builders under test: a command header from its fields' offsets, a body from the layout of §4.1 (`join_sections`),
//! string lists from §4.3, warnings from §4.9 and Describe's entries from §8.3. Only the frame around the body is the
//! crate's: `Header::encode_frame`, which the golden frames already pin.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Command, FLAG_LAST, Header};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{CLUSTER, frame_of, join_sections};

/// A command header of 128 bytes holding `fields`, each a command-header offset and its bytes; every other byte zero.
pub fn command_header(fields: &[(usize, &[u8])]) -> [u8; 128] {
    let mut out = [0u8; 128];
    for (at, bytes) in fields {
        for (i, byte) in bytes.iter().enumerate() {
            if let Some(slot) = out.get_mut(at + i) {
                *slot = *byte;
            }
        }
    }
    out
}

/// The frame of `command` with the command header `fields` and the body of `sections`, no fixed part, on stream 3;
/// flagged `LAST` when `last`.
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn frame(
    command: Command,
    fields: &[(usize, &[u8])],
    sections: &[Vec<u8>],
    last: bool,
) -> TestResult<Vec<u8>> {
    let flags = if last { FLAG_LAST } else { 0 };
    Ok(Header::new(CLUSTER, command)
        .with_stream(3)
        .with_flags(flags)
        .with_command_header(command_header(fields))
        .encode_frame(&join_sections(&[], sections))?)
}

/// The refusal the view `V` gives `bytes`, or the view's acceptance as `None`.
///
/// # Errors
///
/// When the frame does not validate.
pub fn outcome<'a, V: BodyView<'a>>(bytes: &'a [u8]) -> TestResult<Option<ProtocolErrorEnum>> {
    let frame = frame_of(bytes)?;
    Ok(V::from_frame(&frame).err().map(|e| e.0))
}

/// The refusal the view `V` gives `bytes`.
///
/// # Errors
///
/// When the frame does not validate, or the view accepts it.
pub fn refused<'a, V: BodyView<'a>>(bytes: &'a [u8]) -> TestResult<ProtocolErrorEnum> {
    outcome::<V>(bytes)?.context("a refusal")
}

/// `values` as little-endian u32s.
pub fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A string list's two sections (§4.3): the end of each string, then the strings back to back.
pub fn strings(list: &[&str]) -> [Vec<u8>; 2] {
    let mut ends = Vec::new();
    let mut text = Vec::new();
    for s in list {
        text.extend_from_slice(s.as_bytes());
        ends.extend_from_slice(&(text.len() as u32).to_le_bytes());
    }
    [ends, text]
}

/// A warning's 24 bytes (§4.9): `diagnostic`, `source`, `offset`, `length`, `message`, then 4 zero bytes.
pub fn warning(diagnostic: u32, source: u32, offset: u32, length: u32, message: u32) -> Vec<u8> {
    u32s(&[diagnostic, source, offset, length, message, 0])
}

/// The three warning sections of `warnings`, each a diagnostic, a source and its message, the location `offset` 10
/// and `length` 7, messages in order.
pub fn warning_sections(warnings: &[(u32, u32, &str)]) -> [Vec<u8>; 3] {
    let records: Vec<u8> = warnings
        .iter()
        .enumerate()
        .flat_map(|(i, (diagnostic, source, _))| warning(*diagnostic, *source, 10, 7, i as u32))
        .collect();
    let messages: Vec<&str> = warnings.iter().map(|(_, _, m)| *m).collect();
    let [ends, text] = strings(&messages);
    [records, ends, text]
}

/// A request term table's two sections holding the IRIs `iris`, each a record of kind 1 (§4.4).
pub fn iri_terms(iris: &[&str]) -> [Vec<u8>; 2] {
    let entries: Vec<crate::utils::term_records::Entry<'_>> = iris
        .iter()
        .map(|iri| crate::utils::term_records::Entry::Text(1, iri, ""))
        .collect();
    let (records, text) = crate::utils::term_records::term_sections(&entries);
    [records, text]
}

/// UGS0902, `UGS` the language 4 in bits 16–23 and 902 the number in bits 0–15 (§9.4's table of languages).
pub const UGS0902: u32 = (4 << 16) | 902;

/// The message `UGS0902`'s warning carries in these tests, its code, reason and fix as §4.9 orders them.
pub const UGS0902_MESSAGE: &str = "UGS0902 @index(ann) is declared and unenforced until stage 8; drop the index or wait for stage 8";

/// An Install reply in tenant 3 of version 12 at epoch 44, the source `s` holding `t`, no report, and the warning
/// sections `warnings` (§8.4's sections 4 to 6).
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn install_reply(warnings: [Vec<u8>; 3]) -> TestResult<Vec<u8>> {
    let [ends, text] = strings(&["s", "t"]);
    let [records, message_ends, message_text] = warnings;
    frame(
        Command::Install,
        &[
            (0, &3u64.to_le_bytes()),
            (8, &12u64.to_le_bytes()),
            (16, &44u64.to_le_bytes()),
        ],
        &[
            ends,
            text,
            vec![],
            vec![],
            records,
            message_ends,
            message_text,
        ],
        true,
    )
}

/// A term of a hand-laid Result batch: its encoding and its text, `None` for `ABSENT`.
pub type BatchTerm = (Vec<u8>, Option<&'static str>);

/// The encoding of an IRI or a blank node in a Result (§10.1): `tag`, then the first 16 bytes of the SHA-256 digest of
/// `text`, from `ultragraphdb_core_terms`' hash.
pub fn named(tag: u8, text: &'static str) -> BatchTerm {
    let hash = ultragraphdb_core_terms::Hash128::of(text.as_bytes());
    let mut encoding = vec![tag];
    encoding.extend_from_slice(hash.as_bytes());
    (encoding, Some(text))
}

/// A Result batch laid out by hand from §9.1: `columns` and `rows` in its command header, read at no epoch in no
/// partition, `cells` column by column, `terms` in order with their texts in the string list, and `rest`, sections 6 to
/// 11: the resume key, the column names' ends and text, the warnings and their messages' ends and text. Flagged
/// `LAST` when `last`.
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn result_frame(
    columns: u32,
    rows: u32,
    cells: &[u32],
    terms: &[BatchTerm],
    rest: [Vec<u8>; 6],
    last: bool,
) -> TestResult<Vec<u8>> {
    let mut offsets = Vec::new();
    let mut encodings = Vec::new();
    let mut pairs = Vec::new();
    let mut texts: Vec<&str> = Vec::new();
    for (encoding, text) in terms {
        offsets.push(encodings.len() as u32);
        encodings.extend_from_slice(encoding);
        let index = match text {
            Some(text) => {
                texts.push(text);
                (texts.len() - 1) as u32
            }
            None => u32::MAX,
        };
        pairs.extend_from_slice(&[index, u32::MAX]);
    }
    let [ends, text] = strings(&texts);
    let mut sections = vec![
        u32s(cells),
        u32s(&offsets),
        encodings,
        u32s(&pairs),
        ends,
        text,
    ];
    sections.extend(rest);
    frame(
        Command::Result,
        &[(0, &columns.to_le_bytes()), (4, &rows.to_le_bytes())],
        &sections,
        last,
    )
}

/// Six empty sections, a Result's 6 to 11 holding nothing.
pub fn no_rest() -> [Vec<u8>; 6] {
    [vec![], vec![], vec![], vec![], vec![], vec![]]
}

/// A Prepare laid out by hand from §8.1: tenant 1, `statement`, `schema`, `language`, and the ten `sections`.
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn prepare_frame(
    statement: u64,
    schema: u64,
    language: u8,
    sections: &[Vec<u8>],
) -> TestResult<Vec<u8>> {
    frame(
        Command::Prepare,
        &[
            (0, &1u64.to_le_bytes()),
            (8, &statement.to_le_bytes()),
            (16, &schema.to_le_bytes()),
            (24, &[language]),
        ],
        sections,
        false,
    )
}

/// A Query laid out by hand from §7.4: transaction 5, `language`, no limit, the text `text`, no parameter, no term and
/// no resume key.
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn query_frame(language: u8, text: &str) -> TestResult<Vec<u8>> {
    frame(
        Command::Query,
        &[(0, &5u64.to_le_bytes()), (8, &[language])],
        &[
            text.as_bytes().to_vec(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ],
        false,
    )
}

/// A Describe entry as section 7 lays it out (§8.3): `kind`, three zero bytes, the `name` index, then its `bytes`, its
/// offset where the entry before ends.
pub type EntryRow<'b> = (u8, u32, &'b [u8]);

/// Sections 7 and 8 of `entries`: their 16-byte records, each `offset` where the bytes before end, and their bytes back
/// to back.
pub fn entry_sections(entries: &[EntryRow<'_>]) -> [Vec<u8>; 2] {
    let mut records = Vec::new();
    let mut bytes = Vec::new();
    for (kind, name, entry) in entries {
        records.extend_from_slice(&[*kind, 0, 0, 0]);
        records.extend_from_slice(&u32s(&[*name, bytes.len() as u32, entry.len() as u32]));
        bytes.extend_from_slice(entry);
    }
    [records, bytes]
}

/// A retired name's 24 bytes (§8.3): `item`, `fate`, two zero bytes, `current`, `version`, `until`.
pub fn retired(item: u8, fate: u8, current: u32, version: u64, until: u64) -> Vec<u8> {
    let mut out = vec![item, fate, 0, 0];
    out.extend_from_slice(&current.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&until.to_le_bytes());
    out
}

/// A Describe answer of tenant 9 read at 42, laid out by hand: the strings `names`, no label, relationship type,
/// property key or declaration, and the entry sections `entries`; flagged `LAST` when `last`.
///
/// # Errors
///
/// When the frame cannot be encoded.
pub fn describe_frame(names: &[&str], entries: [Vec<u8>; 2], last: bool) -> TestResult<Vec<u8>> {
    let [ends, text] = strings(names);
    let [records, bytes] = entries;
    frame(
        Command::Describe,
        &[(0, &9u64.to_le_bytes()), (8, &42u64.to_le_bytes())],
        &[
            ends,
            text,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            records,
            bytes,
        ],
        last,
    )
}
