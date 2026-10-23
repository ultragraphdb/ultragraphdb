/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/tok.go (the tokenizer identifiers). Changed for ultragraphdb: translated to
 * Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Tokenizer identifiers are unique and can't be reused. The range 0x00 to 0x7f is reserved for the system, 0x80 to
//! 0xff for custom tokenizers.

/// No tokenizer.
pub const IDENT_NONE: u8 = 0x0;
/// The term tokenizer.
pub const IDENT_TERM: u8 = 0x1;
/// The exact tokenizer.
pub const IDENT_EXACT: u8 = 0x2;
/// The exact tokenizer with a language.
pub const IDENT_EXACT_LANG: u8 = 0x3;
/// The year tokenizer.
pub const IDENT_YEAR: u8 = 0x4;
/// The month tokenizer.
pub const IDENT_MONTH: u8 = 0x41;
/// The day tokenizer.
pub const IDENT_DAY: u8 = 0x42;
/// The hour tokenizer.
pub const IDENT_HOUR: u8 = 0x43;
/// The geo tokenizer.
pub const IDENT_GEO: u8 = 0x5;
/// The int tokenizer.
pub const IDENT_INT: u8 = 0x6;
/// The float tokenizer.
pub const IDENT_FLOAT: u8 = 0x7;
/// The fulltext tokenizer.
pub const IDENT_FULL_TEXT: u8 = 0x8;
/// The bool tokenizer.
pub const IDENT_BOOL: u8 = 0x9;
/// The trigram tokenizer.
pub const IDENT_TRIGRAM: u8 = 0xA;
/// The hash tokenizer.
pub const IDENT_HASH: u8 = 0xB;
/// The sha256 tokenizer.
pub const IDENT_SHA: u8 = 0xC;
/// The bigfloat tokenizer.
pub const IDENT_BIG_FLOAT: u8 = 0xD;
/// The vector index factory.
pub const IDENT_VFLOAT: u8 = 0xE;
/// The ngram tokenizer.
pub const IDENT_NGRAM: u8 = 0xF;
/// The first identifier of custom tokenizers.
pub const IDENT_CUSTOM: u8 = 0x80;
/// ASCII 31, the unit separator: between the language and the collation key of an exact token with a language.
pub const IDENT_DELIMITER: u8 = 0x1f;
