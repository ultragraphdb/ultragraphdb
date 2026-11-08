/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use ultragraphdb_core_keys::{QuadKey, key_prefix};
use ultragraphdb_protocol_wire::{EntriesEnd, ErrorCode, ErrorReply, ScanRange};

use super::graphs::Graphs;
use super::open_scan::OpenScan;
use super::partition_rows_frames::{predicates_of, take_rows};
use super::partition_rows_plan::listed_after;
use super::reading::Reading;
use super::skip::Skip;
use super::{PartitionRows, RANGES_PER_SCAN};
use crate::types::range_spec::RangeSpec;
use crate::types::scan_stream::ScanStream;
use crate::utils::keys::{past_alone, prefix_end};
use crate::utils::refusals::unavailable;

impl PartitionRows {
    /// Reads on until the page holds a row or the partition has no more: the next frame of the Scan under way, the next Scan of
    /// the ranges to come, the next graph's. Past it the page's next row has its names.
    /// A future dropped at a frame drops the Scan under way, taken out to be read, so a partition whose fill was
    /// dropped is read no further: `merge` ends the read with the error that dropped it.
    pub(crate) async fn fill(&mut self) -> Result<(), ErrorReply> {
        loop {
            if self.page.has_row() {
                return self.name_ahead().await;
            }
            if self.ended {
                return Ok(());
            }
            self.page.clear();
            if self.open.is_some() {
                self.next_frame().await?;
                continue;
            }
            if let Some(left) = self.left.take() {
                self.pending.push_front(left);
            }
            if !self.pending.is_empty() {
                self.open_pending()?;
                continue;
            }
            if !self.next_graph().await? {
                self.ended = true;
            }
        }
    }

    /// Whether a row of this partition may be left past the merge's last, by what it has read alone: reading on to tell would record
    /// a span the read's rows never reached, against a read-write transaction's commit (`PROTOCOL.md` §9.1).
    pub(crate) fn may_hold_more(&self) -> bool {
        self.page.has_row()
            || self.left.is_some()
            || self.open.is_some()
            || !self.pending.is_empty()
            || !matches!(self.graphs, Graphs::Done)
    }

    /// The next graph's ranges to come; false with no graph left.
    /// A future dropped in a walk cancels the walk's Scan as its stream drops.
    async fn next_graph(&mut self) -> Result<bool, ErrorReply> {
        let Some(access) = self.access.clone() else {
            return Ok(false);
        };
        let graph = match std::mem::replace(&mut self.graphs, Graphs::Done) {
            Graphs::Done => return Ok(false),
            Graphs::One(graph) => graph,
            Graphs::Walk { keyspace, from } => {
                let Some((graph, next)) = self.walk(keyspace, &from).await? else {
                    return Ok(false);
                };
                self.graphs = Graphs::Walk {
                    keyspace,
                    from: next,
                };
                graph
            }
        };
        self.graph = graph;
        let range = self.range_in(&access, graph);
        if access.lists_predicates() {
            self.open_listing(&range)?;
        } else {
            self.pending.push_back(range);
        }
        Ok(true)
    }

    /// Opens the Scan of a predicates' list.
    fn open_listing(&mut self, range: &RangeSpec) -> Result<(), ErrorReply> {
        let mut scan = self.scan(0);
        scan.push_range(
            ScanRange::new(self.partition(), &range.first, &range.past).with_after(&range.after),
        );
        self.open = Some(OpenScan {
            stream: ScanStream::open(&self.link, &scan)?,
            reading: Reading::Predicates(Vec::new()),
            ranges: VecDeque::from([range.clone()]),
            last: None,
            continues: false,
            gave: false,
            from: None,
        });
        Ok(())
    }

    /// A Scan of limit 1 from `from` finding the next graph by its first key, cancelled past its first frame: the graph, and the
    /// key just past it, where the walk goes on.
    /// A dropped future cancels the Scan as its stream drops.
    async fn walk(
        &mut self,
        keyspace: ultragraphdb_core_keys::Keyspace,
        from: &[u8],
    ) -> Result<Option<(ultragraphdb_core_keys::Graph, Vec<u8>)>, ErrorReply> {
        let mut scan = self.scan(1);
        scan.push_range(ScanRange::new(self.partition(), from, &[]));
        let mut stream = ScanStream::open(&self.link, &scan)?;
        // Each turn takes one frame of the Scan, awaited within the stream's stall bound; the frame the partition marks
        // last ends it at the latest (Ultra rule 2).
        loop {
            let (facts, key) = stream
                .next(|view| {
                    Ok(view
                        .entries()
                        .iter()
                        .next()
                        .map(|entry| entry.key().to_vec()))
                })
                .await?;
            self.note_version(facts);
            if let Some(key) = key {
                let graph = QuadKey::parse(&key)
                    .ok()
                    .and_then(|(parsed, _)| parsed.graph())
                    .ok_or_else(|| {
                        unavailable(format!(
                            "partition {} holds a key naming no graph",
                            self.partition()
                        ))
                    })?;
                let tenant = self.shared.read.tenant;
                let next = prefix_end(&key_prefix(keyspace, false, tenant, Some(graph), None));
                return Ok(Some((graph, next)));
            }
            if facts.last {
                return Ok(None);
            }
        }
    }

    /// Opens the Scan of the ranges next to come: as many one-key ranges as the rows still wanted can fill, or one whole range at
    /// most those rows' keys.
    fn open_pending(&mut self) -> Result<(), ErrorReply> {
        let remaining = self.shared.remaining.get();
        let alone = self.pending.front().is_some_and(RangeSpec::is_alone);
        let wanted = if remaining == 0 {
            RANGES_PER_SCAN
        } else {
            (remaining as usize).min(RANGES_PER_SCAN) // a `u32`, lossless
        };
        let count = if alone { wanted } else { 1 };
        let limit = if alone { 0 } else { remaining };
        let from = self
            .pending
            .front()
            .filter(|range| !range.after.is_empty())
            .map(|range| (range.first.clone(), range.after.clone()));
        let mut scan = self.scan(limit);
        let mut ranges = VecDeque::with_capacity(count);
        for _ in 0..count {
            if !self
                .pending
                .front()
                .is_some_and(|range| range.is_alone() == alone)
            {
                break;
            }
            let Some(range) = self.pending.pop_front() else {
                break;
            };
            scan.push_range(
                ScanRange::new(self.partition(), &range.first, &range.past)
                    .with_after(&range.after),
            );
            ranges.push_back(range);
        }
        self.open = Some(OpenScan {
            stream: ScanStream::open(&self.link, &scan)?,
            reading: Reading::Rows,
            ranges,
            last: None,
            continues: false,
            gave: false,
            from,
        });
        Ok(())
    }

    /// Takes the next frame of the Scan under way. A Scan stalled behind the merge, which grants a partition credit only as it drains
    /// that partition, goes on by a new Scan of the version from where it stopped, so a partition waiting its turn fails no read.
    /// A future dropped at the frame drops the Scan, taken out of `open`, and cancels it.
    async fn next_frame(&mut self) -> Result<(), ErrorReply> {
        let Some(mut open) = self.open.take() else {
            return Ok(());
        };
        let taken = match &mut open.reading {
            Reading::Rows => {
                let (page, filter) = (&mut self.page, &self.filter);
                let skip = self.skip.as_ref();
                open.stream
                    .next(|view| take_rows(view, page, filter, skip))
                    .await
            }
            Reading::Predicates(found) => {
                open.stream
                    .next(|view| predicates_of(view, found).map(|()| None))
                    .await
            }
        };
        let (facts, last) = match taken {
            Ok(taken) => taken,
            Err(refusal) if stalled_behind_merge(&refusal, &open) => {
                self.go_on_after_stall(open);
                return Ok(());
            }
            Err(refusal) => return Err(refusal),
        };
        self.note_version(facts);
        if let Some(last) = last {
            // A frame giving only where a resumed Scan went on from is no progress: a partition stalling there again refuses.
            open.gave = open.gave || open.from.as_ref() != Some(&last);
            open.last = Some(last);
            open.continues = facts.continues;
        }
        if facts.end != EntriesEnd::More {
            let ended = open.ranges.pop_front();
            if facts.end == EntriesEnd::Limit
                && let (Some(range), Some((key, _))) = (ended, &open.last)
            {
                self.left = Some(RangeSpec {
                    first: past_alone(key),
                    past: range.past,
                    after: Vec::new(),
                });
            }
            open.last = None;
            open.continues = false;
        }
        if facts.last {
            if let Reading::Predicates(found) = &open.reading {
                self.expand(found);
            }
        } else {
            self.open = Some(open);
        }
        Ok(())
    }

    /// Puts what `open` had yet to answer back among the ranges to come: the range under way from its last key's next object, which a
    /// list's `after` holds and the read drops, or from the key past it.
    fn go_on_after_stall(&mut self, open: OpenScan) {
        let OpenScan {
            mut ranges,
            last,
            continues,
            ..
        } = open;
        if let (Some(range), Some((key, object))) = (ranges.front_mut(), last) {
            if continues {
                self.skip = Some(Skip::Object {
                    key: key.clone(),
                    encoding: object.clone(),
                    statement: None,
                });
                range.first = key;
                range.after = object;
            } else {
                range.first = past_alone(&key);
                range.after = Vec::new();
            }
        }
        // A range read to its end before the stall holds nothing more to ask for.
        if ranges
            .front()
            .is_some_and(|range| !range.past.is_empty() && range.first >= range.past)
        {
            drop(ranges.pop_front());
        }
        while let Some(range) = ranges.pop_back() {
            self.pending.push_front(range);
        }
    }

    /// The keys `predicates` name in the graph under way, the first resumed where the read stopped when it stopped there.
    fn expand(&mut self, predicates: &[ultragraphdb_core_ids::SystemId]) {
        let Some(access) = self.access.clone() else {
            return;
        };
        let tenant = self.shared.read.tenant;
        for predicate in predicates {
            let Some(key) = access.listed_key(tenant, self.graph, *predicate) else {
                continue;
            };
            let resumed = self.resume_listed.take_if(|at| at.predicate == *predicate);
            let after = match resumed {
                Some(at) => {
                    self.skip_object(&key, &access, &at);
                    listed_after(&access, &at)
                }
                None => Vec::new(),
            };
            self.pending.push_back(RangeSpec::alone(key, after));
        }
        self.resume_listed = None;
    }
}

/// Whether `refusal` is a partition giving up on a list Scan that gave keys and then waited for credit. Such a Scan waits on no epoch,
/// so code 24 can only say that; one that gave no key past where it went on from refused the read for what it is, so a partition
/// stalling again and again ends the read rather than resuming it without end.
fn stalled_behind_merge(refusal: &ErrorReply, open: &OpenScan) -> bool {
    refusal.code() == ErrorCode::TimedOut && open.gave && matches!(open.reading, Reading::Rows)
}
