/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (encodeUint64MatrixUnsafe, decodeUint64MatrixUnsafe). Changed for
 * ultragraphdb: translated to Rust without unsafe; the words are little-endian, as Go's native order is on amd64 and
 * arm64; the decoder refuses a matrix that does not fill its bytes exactly, where Go reads past a short one and
 * ignores trailing bytes.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// A node's neighbour lists, every level's, as one value: the number of rows, then each row's length and uids, every
/// word a little-endian `u64`. An empty matrix encodes as no bytes, Go's nil.
pub fn encode_uint64_matrix(matrix: &[Vec<u64>]) -> Vec<u8> {
    if matrix.is_empty() {
        return Vec::new();
    }
    let words = 1 + matrix.iter().map(|row| 1 + row.len()).sum::<usize>();
    let mut data = Vec::with_capacity(8 * words);
    data.extend_from_slice(&(matrix.len() as u64).to_le_bytes());
    for row in matrix {
        data.extend_from_slice(&(row.len() as u64).to_le_bytes());
        row.iter()
            .for_each(|uid| data.extend_from_slice(&uid.to_le_bytes()));
    }
    data
}

/// The matrix [`encode_uint64_matrix`] wrote; no bytes decode as no rows.
///
/// # Errors
///
/// Why the bytes are no matrix: fewer bytes than a row count or a row length names, or bytes left after the last
/// row. A corruption that leaves a valid matrix decodes.
pub fn decode_uint64_matrix(data: &[u8]) -> Result<Vec<Vec<u64>>, &'static str> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let (chunks, rest) = data.as_chunks::<8>();
    if !rest.is_empty() {
        return Err("a neighbour matrix whose length is not a multiple of 8 bytes");
    }
    let mut words = chunks.iter().map(|w| u64::from_le_bytes(*w));
    let available = (data.len() / 8) as u64;
    let rows = words
        .next()
        .ok_or("a neighbour matrix shorter than its row count")?;
    if rows > available - 1 {
        return Err("a neighbour matrix with more rows than its bytes hold");
    }
    let mut left = available - 1;
    let mut matrix = Vec::with_capacity(rows as usize);
    for _ in 0..rows {
        let len = words
            .next()
            .ok_or("a neighbour matrix shorter than its rows")?;
        left -= 1;
        if len > left {
            return Err("a neighbour row longer than the matrix holds");
        }
        left -= len;
        matrix.push(words.by_ref().take(len as usize).collect());
    }
    if left > 0 {
        return Err("bytes after a neighbour matrix's last row");
    }
    Ok(matrix)
}
