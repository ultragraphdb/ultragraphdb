/*
 * SPDX-FileCopyrightText: © 2023 Damian Gryski <damian@gryski.com>
 * SPDX-License-Identifier: MIT
 *
 * Ported from github.com/dgryski/go-groupvarint (encode.go, decode.go, bytesused.go) at
 * v0.0.0-20230630160417-2bfb7969fb3c, the version Dgraph v25.5.0 uses. Changed for ultragraphdb: translated to Rust;
 * the BytesUsed table is computed from the header.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Group-varint encoding of four u32 values: a header byte holding each value's byte length less one in two bits,
//! lowest value in the lowest bits, followed by each value's little-endian bytes. Zero takes one byte.

/// Appends the group encoding of `src` to `dst`: Go's `Encode4`.
pub(crate) fn encode4(dst: &mut Vec<u8>, src: [u32; 4]) {
    let header_at = dst.len();
    dst.push(0);
    let mut header = 0u8;
    for (i, n) in src.into_iter().enumerate() {
        let b = 3 - (n | 1).leading_zeros() / 8;
        header |= (b as u8) << (2 * i);
        dst.extend_from_slice(&n.to_le_bytes()[..=b as usize]);
    }
    dst[header_at] = header;
}

/// Decodes the group at the start of `src`: Go's `Decode4`. Bytes past the end of `src` read as zero, as Dgraph's
/// decoder pads a short buffer with zeros.
pub(crate) fn decode4(src: &[u8]) -> [u32; 4] {
    let byte = |i: usize| src.get(i).copied().unwrap_or(0);
    let header = byte(0);
    let mut offs = 1;
    let mut out = [0u32; 4];
    for (i, value) in out.iter_mut().enumerate() {
        let len = usize::from((header >> (2 * i)) & 3) + 1;
        *value = (0..len).fold(0u32, |acc, k| acc | u32::from(byte(offs + k)) << (8 * k));
        offs += len;
    }
    out
}

/// The length of the group whose header is `header`, header included: Go's `BytesUsed[header]`.
pub(crate) fn bytes_used(header: u8) -> usize {
    1 + (0..4)
        .map(|i| usize::from((header >> (2 * i)) & 3) + 1)
        .sum::<usize>()
}
