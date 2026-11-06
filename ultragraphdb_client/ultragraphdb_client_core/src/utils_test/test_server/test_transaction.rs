/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_wire::{ErrorCode, RequestTerm};

/// A transaction the server holds on a stream: the operations it asked for, its token's epoch and its deadline.
#[derive(Debug, Clone, Copy)]
pub(super) struct TestTransaction {
    pub operations: u32,
    pub epoch: Option<u64>,
    pub deadline: Option<Instant>,
}

/// A refusal scripted for the next commit, the commit applied first or not, with the value a failed condition found.
#[derive(Debug, Clone)]
pub(super) struct ScriptedAnswer {
    pub code: ErrorCode,
    pub detail: u64,
    pub apply: bool,
    pub value: Option<RequestTerm>,
}

/// A refusal scripted for the next read, after its Result frames.
#[derive(Debug, Clone, Copy)]
pub(super) struct ScriptedRead {
    pub batches: u32,
    pub code: ErrorCode,
    pub detail: u64,
}

/// A read waiting on an epoch not yet closed, until its transaction's deadline.
#[derive(Debug, Clone, Copy)]
pub(super) struct WaitingRead {
    pub stream: u64,
    pub epoch: u64,
    pub deadline: Instant,
}
