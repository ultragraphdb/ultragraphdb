/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A migration run's state, a Migrate reply's `state` byte (`PROTOCOL.md` §8.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MigrateState {
    /// 1: `staged`, the Install succeeded.
    Staged,
    /// 2: `copying`, the run started.
    Copying,
    /// 3: `validating`, every partition has copied.
    Validating,
    /// 4: `checked`, validation passed; a dry run stops here.
    Checked,
    /// 5: `converging`, the cutover was requested.
    Converging,
    /// 6: `fenced`, the last unfenced round ended.
    Fenced,
    /// 7: `catching_up`, the final round began.
    CatchingUp,
    /// 8: `cut_over`, the version passing the namespace to the target took effect.
    CutOver,
    /// 9: `failed`, a check failed or the run was cancelled before `cut_over`.
    Failed,
}

impl MigrateState {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            MigrateState::Staged => 1,
            MigrateState::Copying => 2,
            MigrateState::Validating => 3,
            MigrateState::Checked => 4,
            MigrateState::Converging => 5,
            MigrateState::Fenced => 6,
            MigrateState::CatchingUp => 7,
            MigrateState::CutOver => 8,
            MigrateState::Failed => 9,
        }
    }

    /// The state the byte `number` names; `None` outside 1 to 9.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(MigrateState::Staged),
            2 => Some(MigrateState::Copying),
            3 => Some(MigrateState::Validating),
            4 => Some(MigrateState::Checked),
            5 => Some(MigrateState::Converging),
            6 => Some(MigrateState::Fenced),
            7 => Some(MigrateState::CatchingUp),
            8 => Some(MigrateState::CutOver),
            9 => Some(MigrateState::Failed),
            _ => None,
        }
    }
}
