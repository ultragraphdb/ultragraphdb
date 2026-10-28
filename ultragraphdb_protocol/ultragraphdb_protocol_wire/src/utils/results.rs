/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Commit-time operations' results, 16 bytes each: `uid` u64, `created` u8 (0 or 1), 7 zero bytes. A Commit reply's
//! section 0 and a write-set record's section 9 hold them alike.

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::operation_result::OperationResult;
use crate::utils::sections::records;

/// The size of one result.
pub(crate) const RESULT_SIZE: usize = 16;

/// Checks `bytes`, section `section` of a body of `command` at body offset `offset`: whole results, each `created`
/// at most 1 and its last 7 bytes zero.
pub(crate) fn check_results(
    command: u16,
    section: usize,
    offset: usize,
    bytes: &[u8],
) -> Result<(), ProtocolError> {
    for (i, record) in records(command, section, bytes, RESULT_SIZE)?.enumerate() {
        let created = record.get(8).copied().unwrap_or(0);
        if created > 1 {
            return Err(ProtocolError::FieldValue(
                command,
                "created",
                created.into(),
            ));
        }
        if let Some(j) = record
            .get(9..)
            .and_then(|zeros| zeros.iter().position(|b| *b != 0))
        {
            return Err(ProtocolError::PaddingNotZero(
                command,
                offset + RESULT_SIZE * i + 9 + j,
            ));
        }
    }
    Ok(())
}

/// Result `index` of checked `bytes`, `None` past the last.
pub(crate) fn result_at(bytes: &[u8], index: usize) -> Option<OperationResult> {
    let record = bytes
        .get(index.checked_mul(RESULT_SIZE)?..index.checked_add(1)?.checked_mul(RESULT_SIZE)?)?;
    let uid = u64::from_le_bytes(record.get(..8)?.try_into().ok()?);
    Some(OperationResult::new(uid, record.get(8) == Some(&1)))
}

/// The bytes of `results`.
pub(crate) fn encode_results(results: &[OperationResult]) -> Vec<u8> {
    results
        .iter()
        .flat_map(|r| {
            r.uid()
                .to_le_bytes()
                .into_iter()
                .chain([u8::from(r.created())])
                .chain([0; 7])
        })
        .collect()
}
