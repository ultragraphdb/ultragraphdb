/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Commit-time operations' results (`PROTOCOL.md` §6.3), 16 bytes each: `value` i64, `node` u32, `created` u8, 3 zero
//! bytes. A Commit reply's section 0 and a write-set record's section 9 hold them alike.

use ultragraphdb_core_body::as_u32;
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::operation_result::OperationResult;
use crate::utils::constants::OPERATION_RESULT_SIZE;

/// Checks `bytes`, section `section` of a body of `command` at body offset `offset`, as §6.3 checks each result in
/// order: whole results ([`ProtocolError::section_length`]); bytes 13 to 15 zero ([`ProtocolError::padding_not_zero`]);
/// `created` at most 1 ([`ProtocolError::field_value`] `"created"`); `node` [`crate::ABSENT`] or below `terms`, the
/// term table's length, when the body holds one ([`ProtocolError::index_out_of_range`]); `value` 0 where a node is
/// named and `created` 0 where none is ([`ProtocolError::field_value`] `"result"`).
pub(crate) fn check_results(
    command: u16,
    section: usize,
    offset: usize,
    bytes: &[u8],
    terms: Option<usize>,
) -> Result<(), ProtocolError> {
    let (results, rest) = bytes.as_chunks::<OPERATION_RESULT_SIZE>();
    if !rest.is_empty() {
        return Err(ProtocolError::section_length(
            command,
            section,
            bytes.len(),
            OPERATION_RESULT_SIZE,
        ));
    }
    for (at, result) in results.iter().enumerate() {
        let [.., created, z0, z1, z2] = *result;
        if let Some(j) = [z0, z1, z2].iter().position(|b| *b != 0) {
            let start = offset.saturating_add(OPERATION_RESULT_SIZE.saturating_mul(at));
            return Err(ProtocolError::padding_not_zero(command, start + 13 + j));
        }
        if created > 1 {
            return Err(ProtocolError::field_value(
                command,
                "created",
                created.into(),
            ));
        }
        let decoded = OperationResult::decode(result);
        check_node(command, section, at, decoded, terms)?;
    }
    Ok(())
}

/// The node rules of result `at`: its node below `terms`, when the body holds a term table; no value beside a node,
/// no creation without one.
fn check_node(
    command: u16,
    section: usize,
    at: usize,
    result: OperationResult,
    terms: Option<usize>,
) -> Result<(), ProtocolError> {
    match result.node() {
        Some(node) => {
            // Lossless: a `usize` is at least 32 bits.
            if let Some(limit) = terms.filter(|limit| node as usize >= *limit) {
                return Err(ProtocolError::index_out_of_range(
                    command,
                    section,
                    at,
                    node,
                    as_u32(limit),
                ));
            }
            if result.value() != 0 {
                // The field's bits as they lie, two's complement.
                return Err(ProtocolError::field_value(
                    command,
                    "result",
                    result.value().cast_unsigned(),
                ));
            }
        }
        None => {
            if result.created() {
                return Err(ProtocolError::field_value(command, "result", 1));
            }
        }
    }
    Ok(())
}

/// Result `index` of checked `bytes`, `None` past the last.
pub(crate) fn result_at(bytes: &[u8], index: usize) -> Option<OperationResult> {
    bytes
        .as_chunks::<OPERATION_RESULT_SIZE>()
        .0
        .get(index)
        .map(OperationResult::decode)
}

/// The bytes of `results`.
pub(crate) fn encode_results(results: &[OperationResult]) -> Vec<u8> {
    results.iter().flat_map(OperationResult::encode).collect()
}
