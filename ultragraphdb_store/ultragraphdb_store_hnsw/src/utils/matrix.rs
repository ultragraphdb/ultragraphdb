/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (encodeUint64MatrixUnsafe, decodeUint64MatrixUnsafe). Changed for
 * ultragraphdb: translated to Rust without unsafe; the counts are little-endian u64 words, as Go's native order is
 * on amd64 and arm64, and each ID is a 128-bit system ID as its 16 little-endian bytes, where Go's were u64 uids;
 * the decoder refuses a matrix that does not fill its bytes exactly and a word that is no valid system ID, where Go
 * reads past a short one and ignores trailing bytes.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_ids::SystemId;

/// A node's neighbour lists, every level's, as one value: the number of rows, then each row's length, each a
/// little-endian `u64`, and after each row's length its IDs, each the 16 little-endian bytes of its 128-bit value.
/// An empty matrix encodes as no bytes, Go's nil.
pub fn encode_id_matrix(matrix: &[Vec<SystemId>]) -> Vec<u8> {
    unimplemented!()
}

/// The matrix [`encode_id_matrix`] wrote; no bytes decode as no rows.
///
/// # Errors
///
/// Why the bytes are no matrix: fewer bytes than the row count, a row's length or a row's IDs need, bytes left after
/// the last row, or an ID that is no valid system ID. A corruption that leaves a valid matrix decodes.
pub fn decode_id_matrix(data: &[u8]) -> Result<Vec<Vec<SystemId>>, &'static str> {
    unimplemented!()
}
