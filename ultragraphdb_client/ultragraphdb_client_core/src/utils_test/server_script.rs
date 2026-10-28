/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a [`crate::ScriptedServer`] does beyond answering: the Result frames it answers a read with, the commits it
/// applies and then drops the connection on without answering, the retries of such a commit it answers busy before
/// the commit is applied, and its namespace's schema id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerScript {
    pub results: u32,
    pub drop_commits: u32,
    pub busy_retries: u32,
    pub schema: u64,
}

impl Default for ServerScript {
    /// Three Result frames a read, no dropped commits, no busy retries, schema id 1.
    fn default() -> Self {
        Self {
            results: 3,
            drop_commits: 0,
            busy_retries: 0,
            schema: 1,
        }
    }
}
