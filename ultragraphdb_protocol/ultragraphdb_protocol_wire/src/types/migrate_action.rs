/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The action a Migrate asks of a migration's run, its `action` byte (`PROTOCOL.md` §8.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MigrateAction {
    /// 1: start the run, through `checked`.
    Start,
    /// 2: a dry run, stopping at `checked` for good.
    DryRun,
    /// 3: cut over: the rounds, the fence and the cutover.
    CutOver,
    /// 4: cancel the run before `cut_over`.
    Cancel,
    /// 5: read the run's state.
    ReadState,
}

impl MigrateAction {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            MigrateAction::Start => 1,
            MigrateAction::DryRun => 2,
            MigrateAction::CutOver => 3,
            MigrateAction::Cancel => 4,
            MigrateAction::ReadState => 5,
        }
    }

    /// The action the byte `number` names; `None` outside 1 to 5.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(MigrateAction::Start),
            2 => Some(MigrateAction::DryRun),
            3 => Some(MigrateAction::CutOver),
            4 => Some(MigrateAction::Cancel),
            5 => Some(MigrateAction::ReadState),
            _ => None,
        }
    }
}
