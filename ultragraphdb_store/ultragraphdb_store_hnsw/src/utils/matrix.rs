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
    if matrix.is_empty() {
        return Vec::new();
    }
    let ids: usize = matrix.iter().map(Vec::len).sum();
    let mut data = Vec::with_capacity(COUNT_LEN * (1 + matrix.len()) + SystemId::BYTES * ids);
    data.extend_from_slice(&(matrix.len() as u64).to_le_bytes());
    for row in matrix {
        data.extend_from_slice(&(row.len() as u64).to_le_bytes());
        row.iter()
            .for_each(|id| data.extend_from_slice(&id.to_u128().to_le_bytes()));
    }
    data
}

/// The length of a count, a little-endian `u64`.
const COUNT_LEN: usize = 8;

/// The matrix [`encode_id_matrix`] wrote; no bytes decode as no rows.
///
/// # Errors
///
/// Why the bytes are no matrix: fewer bytes than the row count, a row's length or a row's IDs need, bytes left after
/// the last row, or an ID that is no valid system ID. A corruption that leaves a valid matrix decodes.
pub fn decode_id_matrix(data: &[u8]) -> Result<Vec<Vec<SystemId>>, &'static str> {
    // A count is 8 bytes and an ID 16, so a matrix is whole words.
    if !data.len().is_multiple_of(COUNT_LEN) {
        return Err("a neighbour matrix whose length is not a multiple of 8 bytes");
    }
    // No bytes are no rows.
    let Some((rows, mut rest)) = read_count(data) else {
        return Ok(Vec::new());
    };
    // Each row takes at least its count's bytes, so the bytes bound the rows whatever the count says.
    if rows > rest.len() / COUNT_LEN {
        return Err("a neighbour matrix with more rows than its bytes hold");
    }
    let mut matrix = Vec::with_capacity(rows);
    for _ in 0..rows {
        let (len, after) = read_count(rest).ok_or("a neighbour matrix shorter than its rows")?;
        if len > after.len() / SystemId::BYTES {
            return Err("a neighbour row longer than the matrix holds");
        }
        let (row, after) = after.split_at(len * SystemId::BYTES);
        let ids = row
            .as_chunks::<{ SystemId::BYTES }>()
            .0
            .iter()
            .map(|b| SystemId::from_u128(u128::from_le_bytes(*b)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "a neighbour matrix holding no valid system ID")?;
        matrix.push(ids);
        rest = after;
    }
    if !rest.is_empty() {
        return Err("bytes after a neighbour matrix's last row");
    }
    Ok(matrix)
}

/// The count `data` begins with, and the bytes after it; `None` when fewer than a count's bytes remain. A count past
/// the platform's sizes reads as the largest, which no bytes hold.
fn read_count(data: &[u8]) -> Option<(usize, &[u8])> {
    let (count, rest) = data.split_first_chunk::<COUNT_LEN>()?;
    Some((
        usize::try_from(u64::from_le_bytes(*count)).unwrap_or(usize::MAX),
        rest,
    ))
}
