/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A write-set record's sections 3 and 4: operations of 24 bytes and registrations of 32 (`PROTOCOL.md` §12.2).

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::record_operation::RecordOperation;
use crate::types::record_registration::RecordRegistration;
use crate::utils::constants::{RECORD_OPERATION_SIZE, RECORD_REGISTRATION_SIZE};

/// Checks `bytes`, section `section` of a record of `command` at record offset `offset`: whole operations
/// ([`ProtocolError::section_length`]), each `operation` 1 to 7 ([`ProtocolError::field_value`] `"operation"`), its
/// bytes 2 to 7 zero ([`ProtocolError::padding_not_zero`]) and its `argument` as its code takes
/// ([`ProtocolError::field_value`] `"argument"`, naming the operation's index).
pub(crate) fn check_operations(
    command: u16,
    section: usize,
    offset: usize,
    bytes: &[u8],
) -> Result<(), ProtocolError> {
    let (operations, rest) = bytes.as_chunks::<RECORD_OPERATION_SIZE>();
    if !rest.is_empty() {
        return Err(ProtocolError::section_length(
            command,
            section,
            bytes.len(),
            RECORD_OPERATION_SIZE,
        ));
    }
    for (at, operation) in operations.iter().enumerate() {
        let (code, zeros, argument) = parts(operation);
        if !(1..=7).contains(&code) {
            return Err(ProtocolError::field_value(
                command,
                "operation",
                code.into(),
            ));
        }
        if let Some(j) = zeros.iter().position(|b| *b != 0) {
            let start = offset.saturating_add(RECORD_OPERATION_SIZE.saturating_mul(at));
            return Err(ProtocolError::padding_not_zero(command, start + 2 + j));
        }
        if RecordOperation::from_parts(code, argument).is_none() {
            return Err(ProtocolError::field_value(command, "argument", at as u64));
        }
    }
    Ok(())
}

/// An operation's `code`, its 6 zero bytes and its `argument`.
fn parts(operation: &[u8; RECORD_OPERATION_SIZE]) -> (u16, [u8; 6], [u8; 16]) {
    let [c0, c1, z0, z1, z2, z3, z4, z5, argument @ ..] = *operation;
    (
        u16::from_le_bytes([c0, c1]),
        [z0, z1, z2, z3, z4, z5],
        argument,
    )
}

/// Operation `index` of checked `bytes`, `None` past the last.
pub(crate) fn operation_at(bytes: &[u8], index: usize) -> Option<RecordOperation> {
    let (code, _, argument) = parts(bytes.as_chunks::<RECORD_OPERATION_SIZE>().0.get(index)?);
    RecordOperation::from_parts(code, argument)
}

/// Checks `bytes`, section `section` of a record of `command`: whole registrations
/// ([`ProtocolError::section_length`]), each `entity` a valid system ID, `group` below 2¹⁸, bytes 20 to 23 zero and
/// `generation` at least 1 ([`ProtocolError::field_value`] `"registration"`, naming the registration's index).
pub(crate) fn check_registrations(
    command: u16,
    section: usize,
    bytes: &[u8],
) -> Result<(), ProtocolError> {
    let (registrations, rest) = bytes.as_chunks::<RECORD_REGISTRATION_SIZE>();
    if !rest.is_empty() {
        return Err(ProtocolError::section_length(
            command,
            section,
            bytes.len(),
            RECORD_REGISTRATION_SIZE,
        ));
    }
    match registrations
        .iter()
        .position(|r| RecordRegistration::decode(r).is_none())
    {
        Some(at) => Err(ProtocolError::field_value(
            command,
            "registration",
            at as u64,
        )),
        None => Ok(()),
    }
}

/// Registration `index` of checked `bytes`, `None` past the last.
pub(crate) fn registration_at(bytes: &[u8], index: usize) -> Option<RecordRegistration> {
    RecordRegistration::decode(bytes.as_chunks::<RECORD_REGISTRATION_SIZE>().0.get(index)?)
}
