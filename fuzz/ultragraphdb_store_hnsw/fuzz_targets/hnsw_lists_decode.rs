/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The decoders of the HNSW lists' stored values over arbitrary bytes, each given the same bytes: the neighbour
//! matrix of system IDs, the entry point's 16 bytes and the dead list's JSON array of IDs as 32 hexadecimal digits.
//! Each decodes or refuses; nothing panics. A decoded matrix encodes back to its bytes, but for no rows, which 8 zero
//! bytes decode to as well as none; a decoded entry point is its ID's bytes; a decoded dead list's IDs, written again
//! as a dead list, read back alike, and a refusal names the text without its outer spaces.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_store_hnsw::{
    add_dead_node, bytes_to_id, decode_id_matrix, encode_id_matrix, parse_edges,
};

fuzz_target!(|data: &[u8]| {
    match decode_id_matrix(data) {
        Ok(matrix) if matrix.is_empty() => assert!(
            data.is_empty() || data == [0; 8],
            "no rows decode from no bytes or a zero row count"
        ),
        Ok(matrix) => assert_eq!(
            encode_id_matrix(&matrix),
            data,
            "an accepted matrix encodes back to its bytes"
        ),
        Err(reason) => assert!(!reason.is_empty()),
    }
    if let Some(id) = bytes_to_id(data) {
        assert_eq!(id.to_be_bytes(), data, "an entry point is its ID's bytes");
    }
    let text = String::from_utf8_lossy(data);
    match parse_edges(&text) {
        Ok(ids) => {
            let rewritten = ids
                .iter()
                .try_fold(None, |list: Option<Vec<u8>>, &id| {
                    add_dead_node(list.as_deref(), id).map(Some)
                })
                .map(Option::unwrap_or_default);
            let reread = rewritten.and_then(|list| parse_edges(&String::from_utf8_lossy(&list)));
            assert_eq!(reread.ok(), Some(ids), "a dead list's IDs read back alike");
        }
        Err(refused) => {
            let named = text.replace(['\n', '\t'], " ");
            assert_eq!(
                refused.to_string(),
                format!("Cannot convert {} to uint slice", named.trim()),
                "a refusal names the text"
            );
        }
    }
});
