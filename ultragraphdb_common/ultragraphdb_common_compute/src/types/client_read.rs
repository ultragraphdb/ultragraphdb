/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Bucket;

use crate::types::read_pattern::ReadPattern;

/// A read of a transaction, held behind its transaction's order or run by the session's executor.
#[derive(Debug)]
pub(crate) struct ClientRead {
    /// The transaction's stream.
    pub transaction: u64,
    /// What the read asks, read once as it arrived; a read run again after a refreshed map starts from it again.
    pub request: ReadPattern,
    /// The bucket of the subject it binds, by which it routes; `None` for an unbound subject.
    pub subject: Option<Bucket>,
    /// The credit its stream holds, which the connection keeps and a past limit's message names.
    pub credit: u32,
}
