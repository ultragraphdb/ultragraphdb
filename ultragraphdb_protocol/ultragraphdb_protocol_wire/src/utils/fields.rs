/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Command-header field rules shared by the role commands and the session frames (`PROTOCOL.md` §1, §4.8, §13): an
//! epoch, a boolean, a value at least 1, a session or a transaction's identity, each refused as
//! [`ProtocolError::field_value`] naming the field.

use ultragraphdb_error_protocol::ProtocolError;

use crate::utils::constants::MAX_EPOCH;
use crate::utils::le::bytes_at;

/// `value` as an epoch field: 1 to [`MAX_EPOCH`], or 0 where `zero` allows none.
pub(crate) fn epoch(
    command: u16,
    field: &'static str,
    value: u64,
    zero: bool,
) -> Result<u64, ProtocolError> {
    if value > MAX_EPOCH || (value == 0 && !zero) {
        return Err(ProtocolError::field_value(command, field, value));
    }
    Ok(value)
}

/// `value` as a boolean byte: 0 or 1.
pub(crate) fn boolean(command: u16, field: &'static str, value: u8) -> Result<bool, ProtocolError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(ProtocolError::field_value(command, field, other.into())),
    }
}

/// `value`, at least 1.
pub(crate) fn at_least_one(
    command: u16,
    field: &'static str,
    value: u64,
) -> Result<u64, ProtocolError> {
    if value == 0 {
        return Err(ProtocolError::field_value(command, field, 0));
    }
    Ok(value)
}

/// The 16 bytes at `at` of `header`, a session's or a transaction's identity, read as they are.
pub(crate) fn identity(header: &[u8], at: usize) -> [u8; 16] {
    bytes_at(header, at).unwrap_or([0; 16])
}
