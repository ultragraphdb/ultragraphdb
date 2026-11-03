/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Request term tables laid out by hand from `PROTOCOL.md` §4.4's record table, for bodies a builder would not write,
//! and the refusal a view gives a frame.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::BodyView;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::frame_of;

/// One entry of a hand-made term table.
#[derive(Debug, Clone, Copy)]
pub enum Entry<'s> {
    /// A text term: its kind byte (1 to 4), its first and second texts.
    Text(u8, &'s str, &'s str),
    /// A triple term: its subject, predicate and object indices.
    Triple(u32, u32, u32),
}

/// The term table's two sections, records then text, of `entries` in order: each text term's texts where the texts
/// before end, a triple term's indices in place of offsets.
pub fn term_sections(entries: &[Entry<'_>]) -> (Vec<u8>, Vec<u8>) {
    let mut records = Vec::new();
    let mut text = Vec::new();
    for entry in entries {
        let (kind, fields) = match entry {
            Entry::Text(kind, first, second) => {
                let a = text.len() as u32;
                text.extend_from_slice(first.as_bytes());
                let b = text.len() as u32;
                text.extend_from_slice(second.as_bytes());
                (*kind, [a, first.len() as u32, b, second.len() as u32])
            }
            Entry::Triple(s, p, o) => (5, [*s, *p, *o, 0]),
        };
        records.extend_from_slice(&[kind, 0, 0, 0, 0, 0, 0, 0]);
        fields
            .iter()
            .for_each(|f| records.extend_from_slice(&f.to_le_bytes()));
    }
    (records, text)
}

/// The refusal the view `V` gives the frame `bytes`.
///
/// # Errors
///
/// When the frame does not validate, or the view accepts it.
pub fn refusal<'a, V: BodyView<'a>>(bytes: &'a [u8]) -> TestResult<ProtocolErrorEnum> {
    let frame = frame_of(bytes)?;
    Ok(V::from_frame(&frame).err().context("a refusal")?.0)
}
