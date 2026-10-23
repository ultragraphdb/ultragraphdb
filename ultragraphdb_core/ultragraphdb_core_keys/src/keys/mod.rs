/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

pub(crate) mod key_builders;
pub(crate) mod namespace_attr;
pub(crate) mod parse;
pub(crate) mod reserved_namespace;
pub(crate) mod reserved_predicates;

// TODO(pawan) - Make this 2 bytes long. Right now ParsedKey has ByteType and
// bytePrefix. Change it so that it just has one field which has all the information.

/// Indicates the key stores data.
pub const BYTE_DATA: u8 = 0x00;
/// Indicates the key stores an index.
pub const BYTE_INDEX: u8 = 0x02;
/// Indicates the key stores a reverse index.
pub const BYTE_REVERSE: u8 = 0x04;
/// Indicates the key stores a count index.
pub const BYTE_COUNT: u8 = 0x08;
/// Indicates the key stores a reverse count index.
pub const BYTE_COUNT_REV: u8 = BYTE_COUNT | BYTE_REVERSE;
/// The prefix used for data, index and reverse keys so that relative order of data doesn't change keys of same
/// attributes are located together.
pub const DEFAULT_PREFIX: u8 = 0x00;
/// The prefix of schema keys.
pub const BYTE_SCHEMA: u8 = 0x01;
/// The prefix of type keys.
pub const BYTE_TYPE: u8 = 0x02;
/// Signals that the key stores an individual part of a multi-part list.
pub const BYTE_SPLIT: u8 = 0x04;
/// Specifies keys which need to be discarded.
pub const BYTE_UNUSED: u8 = 0xff;
/// The default namespace.
pub const ROOT_NAMESPACE: u64 = 0;
/// The byte range which will be ignored while prefix match in subscription.
pub const IGNORE_BYTES: &str = "1-8";
/// The offset in a key from which the next 8 bytes contain the namespace.
pub const NAMESPACE_OFFSET: usize = 1;
/// The separator between the namespace and attribute.
pub const NS_SEPARATOR: &str = "-";
