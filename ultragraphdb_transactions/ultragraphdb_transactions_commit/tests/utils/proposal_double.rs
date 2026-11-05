/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A proposal built by hand: a read set and a write-set record encoded by `ultragraphdb_protocol_wire`'s own
//! `WriteSetRecord`, so a test names exactly what a transaction read and wrote without a store.

use ultragraphdb_core_ids::{Epoch, SessionToken, SystemId};
use ultragraphdb_protocol_wire::WriteSetRecord;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::Proposal;
use ultragraphdb_transactions_txn::ReadSet;

/// What a transaction read, by kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reads {
    /// The keys of the lists read whole.
    pub lists: Vec<Vec<u8>>,
    /// The statements tested: a list key and an object's encoding.
    pub statements: Vec<(Vec<u8>, Vec<u8>)>,
    /// The ranges scanned: a start, inclusive, and an end, exclusive, or `None` for the last key.
    pub ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    /// The terms found absent, each the sequence of its test ID ([`id`]).
    pub absent: Vec<u64>,
}

/// What a transaction wrote, as its record's conflict sections hold it. A range's end of `None` is written as an
/// empty end key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Writes {
    /// The conflict points: each a key written.
    pub points: Vec<Vec<u8>>,
    /// The conflict statements: a list key and the object it changed.
    pub statements: Vec<(Vec<u8>, Vec<u8>)>,
    /// The conflict ranges: the first key and the key past the last.
    pub ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}

impl Reads {
    /// The read of the list of `key`, whole.
    pub fn list(key: &[u8]) -> Self {
        Self {
            lists: vec![key.to_vec()],
            ..Self::default()
        }
    }

    /// The test of the statement of `object` in the list of `key`.
    pub fn statement(key: &[u8], object: &[u8]) -> Self {
        Self {
            statements: vec![(key.to_vec(), object.to_vec())],
            ..Self::default()
        }
    }

    /// The scan from `start`, inclusive, to `end`, exclusive, or to the last key.
    pub fn range(start: &[u8], end: Option<&[u8]>) -> Self {
        Self {
            ranges: vec![(start.to_vec(), end.map(<[u8]>::to_vec))],
            ..Self::default()
        }
    }

    /// The term of `id(sequence)`, found absent.
    pub fn absent(sequence: u64) -> Self {
        Self {
            absent: vec![sequence],
            ..Self::default()
        }
    }
}

impl Writes {
    /// A statement of the term of `id(sequence)` written: its term key, a conflict point.
    pub fn term(sequence: u64) -> Self {
        Self::point(&term_key(sequence))
    }

    /// The write of `key`.
    pub fn point(key: &[u8]) -> Self {
        Self {
            points: vec![key.to_vec()],
            ..Self::default()
        }
    }

    /// The change of `object` in the list of `key`.
    pub fn statement(key: &[u8], object: &[u8]) -> Self {
        Self {
            statements: vec![(key.to_vec(), object.to_vec())],
            ..Self::default()
        }
    }

    /// The write of the range from `start` to the key `end` past the last, or to the last key.
    pub fn range(start: &[u8], end: Option<&[u8]>) -> Self {
        Self {
            ranges: vec![(start.to_vec(), end.map(<[u8]>::to_vec))],
            ..Self::default()
        }
    }
}

/// A proposal: a sealed read set and a record's bytes.
#[derive(Debug, Clone)]
pub struct TestProposal {
    /// The sealed read set.
    pub read_set: ReadSet,
    /// The write-set record's bytes.
    pub record: Vec<u8>,
    /// The session token the transaction carries.
    pub token: Option<SessionToken>,
    /// The epoch the transaction may be stamped at, at the earliest.
    pub epoch_floor: Option<Epoch>,
    /// Whether it is a frame holding what it does: a one-frame write or a Drop.
    pub one_frame: bool,
    /// Whether it asks for commit-time operations.
    pub operations: bool,
}

impl TestProposal {
    /// The proposal carrying `token`.
    pub fn with_token(self, token: SessionToken) -> Self {
        Self {
            token: Some(token),
            ..self
        }
    }

    /// The proposal as a frame holding what it does, whose resend the session table answers.
    pub fn with_one_frame(self) -> Self {
        Self {
            one_frame: true,
            ..self
        }
    }

    /// The proposal asking for commit-time operations.
    pub fn asking_operations(self) -> Self {
        Self {
            operations: true,
            ..self
        }
    }

    /// The proposal stamped at `epoch` at the earliest.
    pub fn with_epoch_floor(self, epoch: Epoch) -> Self {
        Self {
            epoch_floor: Some(epoch),
            ..self
        }
    }
}

impl Proposal for TestProposal {
    fn read_set(&self) -> &ReadSet {
        &self.read_set
    }

    fn record(&self) -> &[u8] {
        &self.record
    }

    fn token(&self) -> Option<&SessionToken> {
        self.token.as_ref()
    }

    fn epoch_floor(&self) -> Option<Epoch> {
        self.epoch_floor
    }

    fn is_one_frame(&self) -> bool {
        self.one_frame
    }

    fn has_operations(&self) -> bool {
        self.operations
    }
}

/// The sealed read set of a transaction of `tenant` at `read_version` that read `reads`.
pub fn read_set(tenant: u64, read_version: u64, reads: &Reads) -> TestResult<ReadSet> {
    let mut set = ReadSet::new();
    set.reset(tenant, read_version, usize::MAX);
    for key in &reads.lists {
        set.push_list(key)?;
    }
    for (key, object) in &reads.statements {
        set.push_statement(key, object)?;
    }
    for (start, end) in &reads.ranges {
        set.push_range(start, end.as_deref())?;
    }
    for sequence in &reads.absent {
        set.push_absent(id(*sequence)?)?;
    }
    set.seal();
    Ok(set)
}

/// The system ID of type 1, sequence `sequence` (at least 1) and bucket 5, a term the tests name by its sequence:
/// `(1 << 112) | (sequence << 48) | (5 << 24)`.
pub fn id(sequence: u64) -> TestResult<SystemId> {
    SystemId::from_u128((1 << 112) | (u128::from(sequence) << 48) | (5 << 24)).context("the ID")
}

/// The term key of `id(sequence)`, written from its layout: the tag 0xFE, then the ID's 16 big-endian bytes.
pub fn term_key(sequence: u64) -> Vec<u8> {
    let mut key = vec![0xFE, 0x00, 0x01];
    key.extend_from_slice(&sequence.to_be_bytes());
    key.extend_from_slice(&[0, 0, 5, 0, 0, 0]);
    key
}

/// The record of a transaction of `tenant` at `read_version` that wrote `writes`, with `padding` bytes of commit
/// record to give it a size.
pub fn record(tenant: u64, read_version: u64, writes: &Writes, padding: usize) -> Vec<u8> {
    record_in(tenant, read_version, 0, 0, writes, padding)
}

/// The same, committed as request `request` of session `session`.
pub fn record_in(
    tenant: u64,
    read_version: u64,
    session: u128,
    request: u64,
    writes: &Writes,
    padding: usize,
) -> Vec<u8> {
    record_with(tenant, read_version, session, request, 0, writes, padding)
}

/// The same, executed against catalog version `catalog`.
pub fn record_with(
    tenant: u64,
    read_version: u64,
    session: u128,
    request: u64,
    catalog: u64,
    writes: &Writes,
    padding: usize,
) -> Vec<u8> {
    let mut record = WriteSetRecord::transaction(tenant);
    record.set_read_version(read_version);
    record.set_session(session.to_be_bytes(), request);
    record.set_catalog(catalog);
    for key in &writes.points {
        record.push_conflict_point(key.clone());
    }
    for (key, object) in &writes.statements {
        record.push_conflict_statement(key.clone(), object.clone());
    }
    for (start, end) in &writes.ranges {
        record.push_conflict_range(start.clone(), end.clone().unwrap_or_default());
    }
    record.set_commit_record(vec![7; padding]);
    record.encode()
}

/// A proposal of tenant 0 at `read_version` reading `reads` and writing `writes`.
pub fn proposal(read_version: u64, reads: &Reads, writes: &Writes) -> TestResult<TestProposal> {
    Ok(TestProposal {
        read_set: read_set(0, read_version, reads)?,
        record: record(0, read_version, writes, 0),
        token: None,
        epoch_floor: None,
        one_frame: false,
        operations: false,
    })
}

/// A proposal of tenant 0 at `read_version`, request `request` of session `session`, reading `reads` and writing
/// `writes`.
pub fn proposal_in(
    session: u128,
    request: u64,
    read_version: u64,
    reads: &Reads,
    writes: &Writes,
) -> TestResult<TestProposal> {
    Ok(TestProposal {
        read_set: read_set(0, read_version, reads)?,
        record: record_in(0, read_version, session, request, writes, 0),
        token: None,
        epoch_floor: None,
        one_frame: false,
        operations: false,
    })
}

/// A proposal of tenant 0 at `read_version` executed against catalog version `catalog`, reading `reads` and writing
/// `writes`.
pub fn proposal_at_catalog(
    catalog: u64,
    read_version: u64,
    reads: &Reads,
    writes: &Writes,
) -> TestResult<TestProposal> {
    Ok(TestProposal {
        read_set: read_set(0, read_version, reads)?,
        record: record_with(0, read_version, 0, 0, catalog, writes, 0),
        token: None,
        epoch_floor: None,
        one_frame: false,
        operations: false,
    })
}

/// A registration record a home accepted (kind 2) in tenant 0: `id(entity)` from bucket group `group`, at generation
/// `generation`.
pub fn registration(entity: u64, group: u32, generation: u64) -> TestResult<Vec<u8>> {
    use ultragraphdb_core_ids::BucketGroup;
    use ultragraphdb_protocol_wire::RecordRegistration;
    let generation = std::num::NonZeroU64::new(generation).context("a generation of 1 or more")?;
    let registration = RecordRegistration::new(
        id(entity)?,
        BucketGroup::new(group).context("the group")?,
        generation,
    );
    Ok(WriteSetRecord::registration(0, registration).encode())
}
