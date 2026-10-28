/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a [`crate::Framer`] asks of its shell after a part of a frame arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameStep {
    /// Read on until the frame's buffer holds this many bytes: the frame's `size`, its header verified.
    Read(usize),
    /// The frame is whole; [`crate::Framer::finish`] hands it over.
    Frame,
}
