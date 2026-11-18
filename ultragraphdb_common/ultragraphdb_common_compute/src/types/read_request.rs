/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_wire::{
    ComputeReadView, ErrorReply, MAX_FRAME_FLOOR_U32, ReadLevel, RequestTerm,
};
use xxhash_rust::xxh3::xxh3_64;

use crate::types::graph_slot::GraphSlot;
use crate::types::read_pattern::ReadPattern;
use crate::utils::clock::now;

/// A read as the executor runs it, for as long as it runs: what its request asks ([`ReadPattern`]) and what it takes of its
/// transaction when it starts, built by [`ReadRequest::new`] and its setters, or from a ComputeRead, its frame given back.
#[derive(Debug)]
pub struct ReadRequest {
    pub(crate) stream: u64,
    pub(crate) partition: u32,
    pub(crate) read_only: bool,
    pub(crate) level: ReadLevel,
    pub(crate) named: bool,
    pub(crate) tenant: u64,
    pub(crate) read_version: u64,
    pub(crate) epoch: u64,
    pub(crate) principal: u64,
    pub(crate) transaction: [u8; 16],
    pub(crate) limit: u32,
    pub(crate) deadline_ms: u32,
    pub(crate) max_frame: u32,
    pub(crate) graph: GraphSlot,
    pub(crate) subject: Option<String>,
    pub(crate) predicate: Option<String>,
    pub(crate) object: Option<RequestTerm>,
    pub(crate) resume: Vec<u8>,
    pub(crate) pattern_hash: u64,
    pub(crate) arrived: Instant,
}

impl ReadRequest {
    /// The read `pattern` asks in a transaction of `tenant`, read-only or read-write, at `level`; every other field 0 until
    /// set, as the transaction's state at the read's start says (`PROTOCOL.md` §11.12, *Reads*).
    #[must_use]
    pub fn new(pattern: ReadPattern, tenant: u64, read_only: bool, level: ReadLevel) -> Self {
        let ReadPattern {
            graph,
            subject,
            predicate,
            object,
            limit,
            resume,
            pattern_hash,
        } = pattern;
        Self {
            stream: 0,
            partition: 0,
            read_only,
            level,
            named: false,
            tenant,
            read_version: 0,
            epoch: 0,
            principal: 0,
            transaction: [0; 16],
            limit,
            deadline_ms: 0,
            max_frame: MAX_FRAME_FLOOR_U32,
            graph,
            subject,
            predicate,
            object,
            resume,
            pattern_hash,
            arrived: now(),
        }
    }

    /// The transaction's partition, once bound, or the partition a level-1 transaction's first read bound it to.
    #[must_use]
    pub fn with_partition(mut self, partition: u32) -> Self {
        self.partition = partition;
        self
    }

    /// A read-write transaction's identity, as its Bind named it, which the read's resume key's `scope` hashes.
    #[must_use]
    pub fn with_transaction(mut self, transaction: [u8; 16]) -> Self {
        self.transaction = transaction;
        self
    }

    /// The version the read reads at: the Bind's answer, or the snapshot's version bound.
    #[must_use]
    pub fn with_read_version(mut self, read_version: u64) -> Self {
        self.read_version = read_version;
        self.named = false;
        self
    }

    /// The version the client named, which the read reads at without waiting.
    #[must_use]
    pub fn with_named_version(mut self, read_version: u64) -> Self {
        self.read_version = read_version;
        self.named = true;
        self
    }

    /// The epoch the read waits for: the snapshot's, the token's, or the read's floor.
    #[must_use]
    pub fn with_epoch(mut self, epoch: u64) -> Self {
        self.epoch = epoch;
        self
    }

    /// The client's principal.
    #[must_use]
    pub fn with_principal(mut self, principal: u64) -> Self {
        self.principal = principal;
        self
    }

    /// The milliseconds left until the transaction's deadline, counted from now.
    #[must_use]
    pub fn with_deadline_ms(mut self, deadline_ms: u32) -> Self {
        self.deadline_ms = deadline_ms;
        self.arrived = now();
        self
    }

    /// The `max_frame` of the client's Hello, 524,288 to 1,048,576: every Result frame of the read is at most this.
    #[must_use]
    pub fn with_max_frame(mut self, max_frame: u32) -> Self {
        self.max_frame = max_frame;
        self
    }

    /// The read `view` asks on `stream`.
    pub(crate) fn of(view: &ComputeReadView<'_>, stream: u64) -> Result<Self, ErrorReply> {
        let pattern = ReadPattern::of_parts(
            (view.terms(), view.pattern()),
            view.limit(),
            view.resume(),
            view.pattern_hash(),
        )?;
        let read = Self::new(pattern, view.tenant(), view.read_only(), view.level())
            .with_partition(view.partition())
            .with_transaction(view.transaction())
            .with_epoch(view.epoch())
            .with_principal(view.principal())
            .with_deadline_ms(view.deadline_ms())
            .with_max_frame(view.max_frame());
        let read = if view.named() {
            read.with_named_version(view.read_version())
        } else {
            read.with_read_version(view.read_version())
        };
        Ok(Self { stream, ..read })
    }

    /// Whether the read is a read-write transaction's.
    pub(crate) fn is_read_write(&self) -> bool {
        !self.read_only
    }

    /// The pattern's unbound slots and the reifier column.
    pub(crate) fn columns(&self) -> u32 {
        let unbound = [
            self.graph == GraphSlot::Unbound,
            self.subject.is_none(),
            self.predicate.is_none(),
            self.object.is_none(),
        ];
        unbound
            .iter()
            .fold(1, |columns, open| columns + u32::from(*open))
    }

    /// What binds the read's resume key to its transaction (`PROTOCOL.md` §11.8).
    pub(crate) fn scope(&self) -> u64 {
        if self.read_only {
            self.read_version
        } else {
            xxh3_64(&self.transaction)
        }
    }

    /// The milliseconds left until the transaction's deadline.
    pub(crate) fn deadline_left(&self) -> u32 {
        let spent =
            u32::try_from(now().duration_since(self.arrived).as_millis()).unwrap_or(u32::MAX);
        self.deadline_ms.saturating_sub(spent)
    }
}
