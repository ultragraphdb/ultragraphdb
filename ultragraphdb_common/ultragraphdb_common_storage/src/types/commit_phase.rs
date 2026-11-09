/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A phase of a commit's way through a partition's core, from the core taking its request to the core handing out its reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitPhase {
    /// From its request taken to its reply handed out.
    Total,
    /// From its request taken to its transaction in the commit step: its execution and its references' wait.
    BeforeStep,
    /// In the commit step: its round's wait, append, sync and apply.
    InStep,
    /// The log's writes while it was in the core.
    LogWrite,
    /// The log's syncs while it was in the core.
    LogSync,
    /// The core waiting on its inbox while it was in the core, its connections running meanwhile.
    Waited,
}

impl CommitPhase {
    /// Every phase, in the order above.
    pub const ALL: [Self; 6] = [
        Self::Total,
        Self::BeforeStep,
        Self::InStep,
        Self::LogWrite,
        Self::LogSync,
        Self::Waited,
    ];
}
