/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A section of fixed-size records read as arrays, so each record's fields are read without indexing.

use ultragraphdb_error_protocol::ProtocolError;

/// The `N`-byte records of section `section` of a body of `command`, refused as [`ProtocolError::section_length`] when
/// its length is not a multiple of `N`.
pub(crate) fn whole_records<const N: usize>(
    command: u16,
    section: usize,
    bytes: &[u8],
) -> Result<&[[u8; N]], ProtocolError> {
    let (records, rest) = bytes.as_chunks::<N>();
    if !rest.is_empty() {
        return Err(ProtocolError::section_length(
            command,
            section,
            bytes.len(),
            N,
        ));
    }
    Ok(records)
}

/// Record `index` of a section of `N`-byte records; `None` past the last.
pub(crate) fn record_at<const N: usize>(bytes: &[u8], index: usize) -> Option<&[u8; N]> {
    bytes.as_chunks::<N>().0.get(index)
}

/// The number of whole `N`-byte records `bytes` holds.
pub(crate) fn record_count<const N: usize>(bytes: &[u8]) -> usize {
    bytes.len() / N
}
