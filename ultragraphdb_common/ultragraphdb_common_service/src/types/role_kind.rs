/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

/// Which role a binary runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleKind {
    /// The retired frontend role, whose binary is removed by the stage's task 6.5.
    Frontend,
    /// The client endpoint and the execution of its reads.
    Compute,
    /// Partitions.
    Storage,
}

impl fmt::Display for RoleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Frontend => "frontend",
            Self::Compute => "compute",
            Self::Storage => "storage",
        })
    }
}
