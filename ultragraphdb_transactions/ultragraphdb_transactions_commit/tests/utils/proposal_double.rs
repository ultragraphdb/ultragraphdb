/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A proposal built by hand: a read set and a write-set record encoded by `ultragraphdb_protocol_wire`'s own
//! `WriteSetRecord`, so a test names exactly what a transaction read and wrote without a store.

use ultragraphdb_protocol_wire::WriteSetRecord;
use ultragraphdb_transactions_commit::Proposal;
use ultragraphdb_transactions_txn::ReadSet;

/// What a transaction read, by kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reads {
    pub lists: Vec<Vec<u8>>,
    pub statements: Vec<(Vec<u8>, Vec<u8>)>,
    pub ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    pub absent: Vec<Vec<u8>>,
}

/// What a transaction wrote, as its record's conflict sections hold it. A range's end of `None` is written as an
/// empty end key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Writes {
    pub points: Vec<Vec<u8>>,
    pub statements: Vec<(Vec<u8>, Vec<u8>)>,
    pub ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}

impl Reads {
    pub fn list(key: &[u8]) -> Self {
        Self {
            lists: vec![key.to_vec()],
            ..Self::default()
        }
    }

    pub fn statement(key: &[u8], object: &[u8]) -> Self {
        Self {
            statements: vec![(key.to_vec(), object.to_vec())],
            ..Self::default()
        }
    }

    pub fn range(start: &[u8], end: Option<&[u8]>) -> Self {
        Self {
            ranges: vec![(start.to_vec(), end.map(<[u8]>::to_vec))],
            ..Self::default()
        }
    }

    pub fn absent(key: &[u8]) -> Self {
        Self {
            absent: vec![key.to_vec()],
            ..Self::default()
        }
    }
}

impl Writes {
    pub fn point(key: &[u8]) -> Self {
        Self {
            points: vec![key.to_vec()],
            ..Self::default()
        }
    }

    pub fn statement(key: &[u8], object: &[u8]) -> Self {
        Self {
            statements: vec![(key.to_vec(), object.to_vec())],
            ..Self::default()
        }
    }

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
    pub read_set: ReadSet,
    pub record: Vec<u8>,
}

impl Proposal for TestProposal {
    fn read_set(&self) -> &ReadSet {
        &self.read_set
    }

    fn record(&self) -> &[u8] {
        &self.record
    }
}

/// The sealed read set of a transaction of `namespace` at `read_version` that read `reads`.
pub fn read_set(namespace: u64, read_version: u64, reads: &Reads) -> ReadSet {
    let mut set = ReadSet::new();
    set.reset(namespace, read_version, usize::MAX);
    for key in &reads.lists {
        set.push_list(key).expect("within the limit");
    }
    for (key, object) in &reads.statements {
        set.push_statement(key, object).expect("within the limit");
    }
    for (start, end) in &reads.ranges {
        set.push_range(start, end.as_deref())
            .expect("within the limit");
    }
    for key in &reads.absent {
        set.push_absent(key).expect("within the limit");
    }
    set.seal();
    set
}

/// The record of a transaction of `namespace` at `read_version` that wrote `writes`, with `padding` bytes of commit
/// record to give it a size.
pub fn record(namespace: u64, read_version: u64, writes: &Writes, padding: usize) -> Vec<u8> {
    let mut record = WriteSetRecord::new(namespace, read_version, 0, 0);
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

/// A proposal of namespace 0 at `read_version` reading `reads` and writing `writes`.
pub fn proposal(read_version: u64, reads: &Reads, writes: &Writes) -> TestProposal {
    TestProposal {
        read_set: read_set(0, read_version, reads),
        record: record(0, read_version, writes, 0),
    }
}
