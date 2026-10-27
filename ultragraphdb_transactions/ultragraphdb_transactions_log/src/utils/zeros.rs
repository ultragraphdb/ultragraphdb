/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use crate::traits::durable_file::DurableFile;
use crate::types::aligned_buf::AlignedBuf;

/// Zeros are written in pieces of at most this many bytes.
const ZEROS: u64 = 1 << 20;

/// Writes zeros over `file` from `from` to `to`, whole blocks at block offsets, in pieces of at most 1 MiB, unsynced:
/// a torn tail's stale blocks, a trimmed segment before it is recycled, a new segment where no blocks can be reserved.
pub(crate) async fn write_zeros<F: DurableFile>(
    file: &mut F,
    from: u64,
    to: u64,
) -> io::Result<()> {
    let mut at = from;
    while at < to {
        let piece = (to - at).min(ZEROS);
        file.write_at(at, AlignedBuf::zeroed(piece as usize))
            .await?;
        at += piece;
    }
    Ok(())
}
