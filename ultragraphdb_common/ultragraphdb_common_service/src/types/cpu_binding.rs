/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use std::fmt;

/// What became of a runtime's binding to a CPU, as the role logs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuBinding {
    /// The settings did not ask for one.
    NotAsked,
    /// The platform cannot bind a thread or say what it holds, so the runtime runs unpinned.
    Unavailable,
    /// The binding, read back from the operating system: the CPU asked for and the CPUs the thread may run on.
    ReadBack {
        /// The CPU asked for.
        asked: usize,
        /// What the thread holds.
        held: BTreeSet<usize>,
    },
}

impl fmt::Display for CpuBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAsked => write!(f, "unpinned: pin_cores is not set"),
            Self::Unavailable => write!(f, "unpinned: this platform cannot bind a thread to a CPU"),
            Self::ReadBack { asked, held } if held.len() == 1 && held.contains(asked) => {
                write!(f, "pinned to CPU {asked}")
            }
            Self::ReadBack { asked, held } => {
                write!(
                    f,
                    "unpinned: asked for CPU {asked}, the thread holds {held:?}"
                )
            }
        }
    }
}
