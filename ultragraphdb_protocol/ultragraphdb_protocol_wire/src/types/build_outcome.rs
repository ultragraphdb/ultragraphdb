/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How an index build ended, as its completion record names it (`PROTOCOL.md` §12.2, operation 7, byte 16): a swap
/// puts its indexes in, an undo restores the previous declaration and removes the indexes it built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildOutcome {
    /// 1: the build's lists and new declaration are swapped in.
    Swap,
    /// 2: the build failed, or a drop cancelled it: its previous declaration is restored.
    Undo,
}

impl BuildOutcome {
    /// The outcome's byte, 1 or 2.
    #[must_use]
    pub fn byte(self) -> u8 {
        match self {
            BuildOutcome::Swap => 1,
            BuildOutcome::Undo => 2,
        }
    }

    /// The outcome of `byte`; `None` for a byte naming none.
    #[must_use]
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(BuildOutcome::Swap),
            2 => Some(BuildOutcome::Undo),
            _ => None,
        }
    }
}
