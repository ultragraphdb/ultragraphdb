/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The body layout and the string list of `PROTOCOL.md` §4.1 and §4.3, read in place and written, with no I/O.
//!
//! A body is a fixed part, a table of sections, each an `offset` u32 and a `len` u32, then the sections, each at the
//! next multiple of 8 after the one before, the bytes between zero. [`Sections::read`] checks a body against the
//! layout once and then gives any section in place; [`write_sections`] writes one. A [`StringList`] is two sections,
//! the end offset of each string and the text, read in place; a [`StringWriter`] writes one, interning. The wire's
//! frames and the catalog's versions (`CATALOG.md`) both take this layout, so one reading discipline serves both, and
//! neither depends on the other. A refusal is a [`BodyError`], naming the rule, the section and the offset; the caller
//! names the body.

// Arithmetic on decoded wire/store input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

mod errors;
mod types;
mod utils;

pub use crate::errors::body_error::{BodyError, BodyErrorEnum};
pub use crate::types::sections::Sections;
pub use crate::types::string_list::StringList;
pub use crate::types::string_writer::StringWriter;
pub use crate::utils::layout::{
    align8, as_u32, records, text, u32_at, u32_bytes, u32s, write_sections, write_sections_into,
};
