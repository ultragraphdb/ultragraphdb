/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod partition_rows_fill;
mod partition_rows_frames;
mod partition_rows_plan;
mod partition_rows_terms;

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{ErrorReply, Scan, ScanRange, TermTable};
use ultragraphdb_store_quads::term_key;

use self::partition_rows_frames::FrameTail;
use crate::types::access::{Access, RangeSpec};
use crate::types::link::Link;
use crate::types::names::Names;
use crate::types::page::Page;
use crate::types::read_at::ReadAt;
use crate::types::read_pins::ReadPins;
use crate::types::read_request::ReadRequest;
use crate::types::resume_at::ResumeAt;
use crate::types::scan_stream::{ScanFacts, ScanStream};
use crate::utils::keys::past_alone;

/// What every partition of one read shares of it.
#[derive(Debug)]
pub(crate) struct ReadShared {
    pub(crate) read: ReadRequest,
    /// The rows the read still wants, 0 for no limit: how far a Scan need go.
    pub(crate) remaining: Cell<u32>,
    pub(crate) pins: Rc<ReadPins>,
}

/// The graphs a partition's read goes through.
#[derive(Debug)]
enum Graphs {
    /// This graph alone, then none.
    One(Graph),
    /// Walked by Scans of limit 1 from this key of the keyspace.
    Walk { keyspace: Keyspace, from: Vec<u8> },
    /// None left.
    Done,
}

/// What the Scan under way reads.
#[derive(Debug)]
enum Reading {
    /// Rows, into the page.
    Rows,
    /// A predicates' list, into the keys read next.
    Predicates(Vec<SystemId>),
}

/// The Scan under way.
#[derive(Debug)]
struct OpenScan {
    stream: ScanStream,
    reading: Reading,
    /// The Scan's ranges not ended yet, the first the one under way.
    ranges: VecDeque<RangeSpec>,
    /// The range under way's last key and that key's last object, and whether a next frame continues that key's list.
    last: Option<FrameTail>,
    continues: bool,
    /// Whether a frame of the Scan has given a key past `from`.
    gave: bool,
    /// The key and object a Scan resumed in a list goes on from, which the read returned already.
    from: Option<FrameTail>,
}

/// What a read drops of the list or record it resumes in: the row it stopped after and those before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Skip {
    /// In the list of `key`, the object `encoding`: whole with no `statement`, else its statements at or below that ID.
    Object {
        key: Vec<u8>,
        encoding: Vec<u8>,
        statement: Option<SystemId>,
    },
    /// In a statement's record, the properties up to and including this one.
    Property {
        predicate: SystemId,
        value: EncodedTerm,
    },
}

/// What only some of a list's or record's objects are read for: a bound object beside a list's key, or a bound predicate and
/// object in a statement's record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RowFilter {
    pub(crate) object: Option<Vec<u8>>,
    pub(crate) property: Option<SystemId>,
}

/// A dictionary Scan under way, for the terms of one partition.
#[derive(Debug)]
pub(crate) struct Gate {
    stream: Option<ScanStream>,
    terms: Vec<(Vec<u8>, String)>,
}

/// One partition's part of a read: the Scans of the version its first chose, the page of rows the merge drains, and the names of
/// those rows, read before the merge sees them.
#[derive(Debug)]
pub(crate) struct PartitionRows {
    link: Rc<Link>,
    shared: Rc<ReadShared>,
    first: ReadAt,
    chosen: Option<(u64, u64)>,
    names: Names,
    page: Page,
    access: Option<Access>,
    filter: RowFilter,
    graphs: Graphs,
    graph: Graph,
    pending: VecDeque<RangeSpec>,
    open: Option<OpenScan>,
    left: Option<RangeSpec>,
    resume: Option<ResumeAt>,
    resume_listed: Option<ResumeAt>,
    skip: Option<Skip>,
    ended: bool,
}

/// The ranges of a Scan at most: a frame holds these with room to spare.
const RANGES_PER_SCAN: usize = 4_096;

impl PartitionRows {
    /// A read of the partition of `link`, whose first Scan reads as `first` says.
    pub(crate) fn new(link: Rc<Link>, shared: Rc<ReadShared>, first: ReadAt) -> Self {
        let names = Names::new(shared.read.tenant);
        Self {
            link,
            shared,
            first,
            chosen: None,
            names,
            page: Page::default(),
            access: None,
            filter: RowFilter::default(),
            graphs: Graphs::Done,
            graph: Graph::Default,
            pending: VecDeque::new(),
            open: None,
            left: None,
            resume: None,
            resume_listed: None,
            skip: None,
            ended: false,
        }
    }

    /// The partition.
    pub(crate) fn partition(&self) -> u32 {
        self.link.partition()
    }

    /// The version and the epoch the partition's first Scan chose, once one has answered.
    pub(crate) fn chosen(&self) -> Option<(u64, u64)> {
        self.chosen
    }

    /// The row the merge sees next.
    pub(crate) fn page(&self) -> &Page {
        &self.page
    }

    /// Moves past the row the merge took.
    pub(crate) fn advance(&mut self) {
        self.page.advance();
    }

    /// A Scan at most `limit` entries per range, as a partition's first reads or, once one has answered, at the version it
    /// chose, its ranges to come.
    fn scan(&self, limit: u32) -> Scan {
        let read = &self.shared.read;
        let at = self
            .chosen
            .map_or(self.first, |(version, _)| ReadAt::chosen(version));
        let deadline = if at.epoch == 0 {
            0
        } else {
            read.deadline_left()
        };
        Scan::new(
            at.at,
            at.read_version,
            at.epoch,
            read.principal,
            read.transaction,
            limit,
            deadline,
        )
    }

    /// Takes what a frame of the partition's said of its version: the first chooses it, and the read then holds it.
    fn note_version(&mut self, facts: ScanFacts) {
        self.shared.pins.took_frame();
        if self.chosen.is_none() {
            self.chosen = Some((facts.version, facts.epoch));
            let read = &self.shared.read;
            if read.read_only {
                self.shared
                    .pins
                    .add(&self.link, read.tenant, read.principal, facts.version);
            }
        }
    }

    /// Sends the dictionary Scan resolving `terms`, the IRIs the partition's lists are keyed by: one one-key range each, or any one
    /// such range for a read with no term to resolve, whose answer still shows the partition has closed the read's epoch.
    pub(crate) fn open_gate(&self, terms: Vec<String>) -> Result<Gate, ErrorReply> {
        let read = &self.shared.read;
        if terms.is_empty() && read.is_read_write() {
            return Ok(Gate {
                stream: None,
                terms: Vec::new(),
            });
        }
        let mut table = TermTable::new();
        let mut unique: Vec<(Vec<u8>, String)> = Vec::with_capacity(terms.len());
        for term in terms {
            if read.is_read_write() {
                let _index = table.iri(&term);
            }
            let key = term_key(read.tenant, &term);
            if !unique.iter().any(|(held, _)| *held == key) {
                unique.push((key, term));
            }
        }
        let any = term_key(read.tenant, "");
        let keys: Vec<&[u8]> = if unique.is_empty() {
            vec![any.as_slice()]
        } else {
            unique.iter().map(|(key, _)| key.as_slice()).collect()
        };
        let mut scan = self.scan(0).with_terms(table);
        for key in keys {
            let past = past_alone(key);
            scan.push_range(ScanRange::new(self.partition(), key, &past));
        }
        Ok(Gate {
            stream: Some(ScanStream::open(&self.link, &scan)?),
            terms: unique,
        })
    }

    /// Waits for the answer to `gate`: the IDs of its terms this partition holds. Past it the partition has closed the read's
    /// epoch.
    pub(crate) async fn finish_gate(
        &mut self,
        gate: Gate,
    ) -> Result<HashMap<String, SystemId>, ErrorReply> {
        let Gate { stream, terms } = gate;
        let mut ids = HashMap::with_capacity(terms.len());
        let Some(mut stream) = stream else {
            return Ok(ids);
        };
        let mut found: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        loop {
            let (facts, ()) = stream
                .next(|view| {
                    found.extend(view.entries().iter().map(|entry| {
                        (
                            entry.key().to_vec(),
                            entry.value().unwrap_or_default().to_vec(),
                        )
                    }));
                    Ok(())
                })
                .await?;
            self.note_version(facts);
            if facts.last {
                break;
            }
        }
        for (key, value) in found {
            let Some((_, term)) = terms.iter().find(|(held, _)| *held == key) else {
                continue;
            };
            let id = SystemId::decode(&value).map_err(|error| {
                crate::utils::refusals::unavailable(format!(
                    "partition {} holds no ID under the term {term}: {error}",
                    self.partition()
                ))
            })?;
            let _none = ids.insert(term.clone(), id);
        }
        Ok(ids)
    }

    /// The values `keys`, one-key ranges, hold at the partition's version.
    pub(crate) async fn values(
        &mut self,
        keys: &[Vec<u8>],
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, ErrorReply> {
        let mut found = Vec::new();
        for chunk in keys.chunks(RANGES_PER_SCAN) {
            let mut scan = self.scan(0);
            for key in chunk {
                let past = past_alone(key);
                scan.push_range(ScanRange::new(self.partition(), key, &past));
            }
            let mut stream = ScanStream::open(&self.link, &scan)?;
            loop {
                let (facts, ()) = stream
                    .next(|view| {
                        found.extend(view.entries().iter().map(|entry| {
                            (
                                entry.key().to_vec(),
                                entry.value().unwrap_or_default().to_vec(),
                            )
                        }));
                        Ok(())
                    })
                    .await?;
                self.note_version(facts);
                if facts.last {
                    break;
                }
            }
        }
        Ok(found)
    }
}

pub(crate) use partition_rows_plan::terms_of;
