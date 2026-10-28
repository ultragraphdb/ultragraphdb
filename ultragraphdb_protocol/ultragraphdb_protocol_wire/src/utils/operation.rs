/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A write-set record's section 10, its operation: empty, or `operation` u16, 6 zero bytes, `argument` u64.

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::record_operation::RecordOperation;

/// The size of an operation.
const OPERATION_SIZE: usize = 16;

/// The operation `bytes` hold, section `section` of a body of `command` at body offset `offset`: `None` when empty.
pub(crate) fn read_operation(
    command: u16,
    section: usize,
    offset: usize,
    bytes: &[u8],
) -> Result<Option<RecordOperation>, ProtocolError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let Some(operation) = bytes.first_chunk::<OPERATION_SIZE>().filter(|_| bytes.len() == OPERATION_SIZE) else {
        return Err(ProtocolError::SectionLength(command, section, bytes.len(), OPERATION_SIZE));
    };
    let code = u16::from_le_bytes([operation[0], operation[1]]);
    let mut argument = [0; 8];
    argument.copy_from_slice(&operation[8..]);
    let Some(read) = RecordOperation::from_parts(code, u64::from_le_bytes(argument)) else {
        return Err(ProtocolError::FieldValue(command, "operation", code.into()));
    };
    if let Some(j) = operation[2..8].iter().position(|b| *b != 0) {
        return Err(ProtocolError::PaddingNotZero(command, offset + 2 + j));
    }
    Ok(Some(read))
}

/// The bytes of `operation`, empty for none.
pub(crate) fn encode_operation(operation: Option<RecordOperation>) -> Vec<u8> {
    operation.map_or_else(Vec::new, |o| {
        let mut bytes = o.code().to_le_bytes().to_vec();
        bytes.extend_from_slice(&[0; 6]);
        bytes.extend_from_slice(&o.argument().to_le_bytes());
        bytes
    })
}
