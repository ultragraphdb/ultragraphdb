/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod credit_body_builder;

/// More credit for the frame's result stream: `frames` more Result frames may be sent.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `frames` | u32, at least 1 |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Credit {
    frames: u32,
}

impl Credit {
    pub fn new(frames: u32) -> Self {
        Self { frames }
    }

    pub fn frames(&self) -> u32 {
        self.frames
    }
}
