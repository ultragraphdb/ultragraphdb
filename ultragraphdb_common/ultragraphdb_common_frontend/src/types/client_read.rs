/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_core_ids::Bucket;
use ultragraphdb_protocol_wire::QuadPattern;

use crate::types::role_link::RoleLink;

/// A read of a transaction, held or at a compute process.
#[derive(Debug)]
pub(crate) struct ClientRead {
    /// The transaction's stream.
    pub transaction: u64,
    /// A ComputeRead's body, the read's sections, until a frame of it reached the client: what the next compute process gets.
    pub body: Vec<u8>,
    pub pattern: QuadPattern,
    pub limit: u32,
    /// The bucket of the subject it binds, by which it routes; `None` for an unbound subject.
    pub subject: Option<Bucket>,
    /// The credit its stream holds.
    pub credit: u32,
    /// The compute process's stream and link, once sent.
    pub route: Option<(RoleLink, u64)>,
    /// Which of its sends is current.
    pub attempt: u32,
    /// The compute processes that closed under it.
    pub tried: Vec<String>,
    /// How many of its frames reached the client.
    pub frames: u64,
    /// When its last frame arrived, or it was sent.
    pub last_frame: Instant,
}
