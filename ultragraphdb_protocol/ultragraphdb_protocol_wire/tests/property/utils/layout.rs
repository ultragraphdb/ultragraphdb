/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Frames laid out by hand for the property tests, from the tables of `PROTOCOL.md`: a command header from its fields'
//! offsets (§4.2), a body from the layout of §4.1, a term table from §4.4, change records from §4.6 and a string list
//! from §4.3. Only the frame around the body is the crate's, `Header::encode_frame`, which the golden frames pin.
//! Nothing here returns a `Result` a test would have to unwrap: a property folds what it reads into an `Option` or a
//! `Result` that an assertion compares.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyBuilder, BodyView, Command, FrameView, Header};

/// The cluster every frame of the property tests names.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// The largest frame, 1 MiB (`PROTOCOL.md` §2.1).
pub const FRAME_MAX: usize = 1 << 20;

/// The largest epoch, 2⁴² − 1 (`PROTOCOL.md` §1).
pub const EPOCH_MAX: u64 = (1 << 42) - 1;

/// The header every frame is laid out and built under: stream 3, no flag.
pub fn header(command: Command) -> Header {
    Header::new(CLUSTER, command).with_stream(3)
}

/// A command header of 128 bytes holding `fields`, each a command-header offset and its bytes; every other byte zero.
pub fn command_header(fields: &[(usize, &[u8])]) -> [u8; 128] {
    let mut out = [0u8; 128];
    for (at, bytes) in fields {
        for (slot, byte) in out.iter_mut().skip(*at).zip(bytes.iter()) {
            *slot = *byte;
        }
    }
    out
}

/// `len` as the u32 a section table holds; a body of the property tests is far below 4 GiB.
fn u32_of(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// A body of `parts` in the layout of §4.1, no fixed part: the table of offsets and lengths, each section at the end
/// of the one before rounded up to 8, zero padding.
pub fn sections(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut at = 8 * parts.len();
    let mut table = Vec::new();
    for part in parts {
        at = at.div_ceil(8) * 8;
        table.push((at, part.len()));
        at += part.len();
    }
    let mut out = Vec::new();
    for (offset, len) in &table {
        out.extend_from_slice(&u32_of(*offset).to_le_bytes());
        out.extend_from_slice(&u32_of(*len).to_le_bytes());
    }
    for ((offset, _), part) in table.iter().zip(parts) {
        out.resize(*offset, 0);
        out.extend_from_slice(part);
    }
    out
}

/// The frame of `command` with the command header `fields` and `body`, under [`header`] with `flags`; `None` when it
/// cannot be encoded.
pub fn frame(command: Command, fields: [u8; 128], body: &[u8], flags: u8) -> Option<Vec<u8>> {
    header(command)
        .with_flags(flags)
        .with_command_header(fields)
        .encode_frame(body)
        .ok()
}

/// What the view `V` makes of the frame `bytes`: `read` of the view, or the refusal of the frame or the view.
pub fn viewed<'a, V: BodyView<'a>, T>(
    bytes: &'a [u8],
    read: impl FnOnce(&V) -> T,
) -> Result<T, ProtocolErrorEnum> {
    let frame = FrameView::validate(bytes, CLUSTER, FRAME_MAX).map_err(|refusal| refusal.0)?;
    V::from_frame(&frame)
        .map(|view| read(&view))
        .map_err(|refusal| refusal.0)
}

/// Whether the view `V` reads the frame `bytes`, or its refusal.
pub fn outcome<'a, V: BodyView<'a>>(bytes: &'a [u8]) -> Result<(), ProtocolErrorEnum> {
    viewed::<V, ()>(bytes, |_| ())
}

/// The owned body the view `V` reads `bytes` back to; `None` when the frame or the body is refused.
pub fn read_back<'a, V: BodyView<'a>>(bytes: &'a [u8]) -> Option<V::Body> {
    let frame = FrameView::validate(bytes, CLUSTER, FRAME_MAX).ok()?;
    V::from_frame(&frame).ok()?.to_body().ok()
}

/// The frame `body` builds under [`header`], or its builder's refusal.
pub fn built<B: BodyBuilder>(body: &B) -> Result<Vec<u8>, ProtocolErrorEnum> {
    body.frame(header(B::COMMAND)).map_err(|refusal| refusal.0)
}

/// The law every builder of stage 5.8 holds: it refuses exactly what its view refuses of the same fields laid out by
/// hand, with the same refusal, and what it builds is those bytes.
pub fn assert_builds_as_laid_out<'a, B: BodyBuilder, V: BodyView<'a>>(
    body: &B,
    laid_out: &'a Option<Vec<u8>>,
) {
    assert!(laid_out.is_some(), "the frame is laid out");
    let viewed = laid_out
        .as_deref()
        .map_or(Ok(()), |bytes| outcome::<V>(bytes));
    let built = built(body);
    assert_eq!(built.as_ref().map(|_| ()).map_err(Clone::clone), viewed);
    if let Ok(bytes) = built {
        assert_eq!(Some(bytes), laid_out.clone());
    }
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

/// `SectionLength` of `command` for `section` of `len` bytes holding units of `unit`.
pub fn section_length(command: u16, section: usize, len: usize, unit: usize) -> ProtocolErrorEnum {
    ProtocolErrorEnum::SectionLength {
        command,
        section,
        len,
        unit,
    }
}

/// A drawn number within `min..=max`.
pub fn within(tc: &TestCase, min: usize, max: usize) -> usize {
    tc.draw(gs::integers::<usize>().min_value(min).max_value(max))
}

/// A drawn nonzero byte.
pub fn nonzero_byte(tc: &TestCase) -> u8 {
    tc.draw(gs::integers::<u8>().min_value(1))
}

/// A drawn session, 16 bytes not all zero.
pub fn session(tc: &TestCase) -> [u8; 16] {
    let mut drawn: [u8; 16] = tc.draw(gs::arrays(gs::integers::<u8>()));
    if drawn == [0; 16] {
        drawn[15] = 1;
    }
    drawn
}

/// `min..=max` drawn sessions, distinct, none all zero, ascending byte by byte.
pub fn sessions(tc: &TestCase, min: usize, max: usize) -> Vec<[u8; 16]> {
    let mut drawn: Vec<[u8; 16]> = tc.draw(
        gs::vecs(gs::arrays(gs::integers::<u8>()))
            .min_size(min + 1)
            .max_size(max + 1)
            .unique(true),
    );
    drawn.retain(|session| *session != [0; 16]);
    drawn.truncate(max);
    drawn.sort_unstable();
    drawn
}

/// One IRI term-table record of 24 bytes (§4.4): kind 1, direction 0, 6 zero bytes, the text at `at` of `len` bytes,
/// an empty second text after it.
pub fn text_record(kind: u8, at: usize, first: usize, second: usize) -> Vec<u8> {
    let mut record = vec![kind, 0, 0, 0, 0, 0, 0, 0];
    for value in [at, first, at + first, second] {
        record.extend_from_slice(&u32_of(value).to_le_bytes());
    }
    record
}

/// The two sections of a term table holding `terms`, each a kind, a first text and a second text, back to back.
pub fn term_table(terms: &[(u8, &str, &str)]) -> [Vec<u8>; 2] {
    let mut records = Vec::new();
    let mut text = Vec::new();
    for (kind, first, second) in terms {
        records.extend(text_record(*kind, text.len(), first.len(), second.len()));
        text.extend_from_slice(first.as_bytes());
        text.extend_from_slice(second.as_bytes());
    }
    [records, text]
}

/// The two sections of a term table holding the IRIs `iris`.
pub fn iri_table(iris: &[String]) -> [Vec<u8>; 2] {
    let terms: Vec<(u8, &str, &str)> = iris.iter().map(|iri| (1, iri.as_str(), "")).collect();
    term_table(&terms)
}

/// `values` as little-endian u32s.
pub fn u32s(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// `min..=max` drawn IRIs, distinct, ascending by their texts' UTF-8 bytes: `urn:` and a drawn tail of any characters.
pub fn iris(tc: &TestCase, min: usize, max: usize) -> Vec<String> {
    let tails: Vec<String> = tc.draw(
        gs::vecs(gs::text().max_size(12))
            .min_size(min)
            .max_size(max)
            .unique(true),
    );
    let mut drawn: Vec<String> = tails.iter().map(|tail| format!("urn:{tail}")).collect();
    drawn.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    drawn
}
