/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use super::partition_rows_frames::FrameTail;
use super::reading::Reading;
use crate::types::range_spec::RangeSpec;
use crate::types::scan_stream::ScanStream;

/// The Scan under way.
#[derive(Debug)]
pub(super) struct OpenScan {
    pub(crate) stream: ScanStream,
    pub(crate) reading: Reading,
    /// The Scan's ranges not ended yet, the first the one under way.
    pub(crate) ranges: VecDeque<RangeSpec>,
    /// The range under way's last key and that key's last object, and whether a next frame continues that key's list.
    pub(crate) last: Option<FrameTail>,
    pub(crate) continues: bool,
    /// Whether a frame of the Scan has given a key past `from`.
    pub(crate) gave: bool,
    /// The key and object a Scan resumed in a list goes on from, which the read returned already.
    pub(crate) from: Option<FrameTail>,
}
