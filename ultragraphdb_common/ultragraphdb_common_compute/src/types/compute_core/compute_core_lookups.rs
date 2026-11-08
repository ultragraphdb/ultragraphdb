/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use ultragraphdb_core_ids::{Epoch, SystemId, Tenant};
use ultragraphdb_core_keys::{CountDirection, Graph, QuadKey};
use ultragraphdb_protocol_wire::{ErrorReply, ObjectRecords, Scan, ScanAt, ScanRange};
use ultragraphdb_store_quads::{id_key, term_key};

use super::ComputeCore;
use crate::types::link::Link;
use crate::types::lookup_at::LookupAt;
use crate::types::read_fault::ReadFault;
use crate::types::read_pins::ReadPins;
use crate::types::scan_stream::ScanStream;
use crate::utils::keys::{hex, past_alone};
use crate::utils::refusals::unavailable;

/// The one-key ranges a lookup's Scan of names holds at most.
const NAMES_PER_SCAN: usize = 4_096;

/// A lookup's one partition, its version chosen by the first Scan.
#[derive(Debug)]
struct LookupPartition {
    link: Rc<Link>,
    version: u64,
}

impl ComputeCore {
    /// The partitions holding `tenant`'s run, asked at `at`: each past a dictionary Scan at the epoch, so no list Scan waits on
    /// another's close, its version held by `pins`. `iri`'s ID per partition, `None` where that partition holds no statement of it.
    /// A dropped future cancels each Scan it opened as its stream drops; the versions it added stay held by `pins`
    /// until they drop.
    async fn lookup_partitions(
        self: &Rc<Self>,
        tenant: u64,
        (at, pins): (LookupAt, &ReadPins),
        iri: &str,
    ) -> Result<Vec<(LookupPartition, Option<SystemId>)>, ErrorReply> {
        let map = self.placement().await?;
        let epoch = Epoch::new(at.epoch()).unwrap_or(Epoch::ZERO);
        let bound = ultragraphdb_core_ids::Version::bound_of(epoch);
        let key = term_key(tenant, iri);
        let mut open = Vec::new();
        for (holder, _) in map.holders_of_run(Tenant::new(tenant), epoch) {
            let link = self.link(holder.get()).await?;
            let mut scan = Scan::new(
                ScanAt::ReadVersion,
                bound,
                at.epoch(),
                0,
                [0; 16],
                0,
                at.deadline_ms(),
            );
            let past = past_alone(&key);
            scan.push_range(ScanRange::new(holder.get(), &key, &past));
            let stream = ScanStream::open(&link, &scan)?;
            open.push((link, stream));
        }
        let mut parts = Vec::with_capacity(open.len());
        for (link, mut stream) in open {
            let (facts, id) = stream
                .next(|view| {
                    view.entries()
                        .iter()
                        .next()
                        .and_then(|entry| entry.value())
                        .map(SystemId::decode)
                        .transpose()
                        .map_err(|error| {
                            ReadFault::Stored(format!(
                                "the dictionary entry {} holds no ID: {error}",
                                hex(&key)
                            ))
                        })
                })
                .await?;
            pins.add(&link, tenant, 0, facts.version);
            parts.push((
                LookupPartition {
                    link,
                    version: facts.version,
                },
                id,
            ));
        }
        Ok(parts)
    }

    /// The lists of range `first` to `past` of `part`, handed to `take` each with its key: every frame's, in order.
    /// A dropped future cancels its Scan as its stream drops.
    async fn lookup_lists(
        part: &LookupPartition,
        first: &[u8],
        past: &[u8],
        mut take: impl FnMut(&[u8], &[u8]) -> Result<(), ReadFault>,
    ) -> Result<(), ErrorReply> {
        let mut scan = Scan::new(ScanAt::ReadVersion, part.version, 0, 0, [0; 16], 0, 0);
        scan.push_range(ScanRange::new(part.link.partition(), first, past));
        let mut stream = ScanStream::open(&part.link, &scan)?;
        // Each turn takes one frame of the Scan, awaited within the stream's stall bound; the frame the partition marks
        // last ends it at the latest (Ultra rule 2).
        loop {
            let (facts, ()) = stream
                .next(|view| {
                    view.entries()
                        .iter()
                        .try_for_each(|entry| take(entry.key(), entry.value().unwrap_or_default()))
                })
                .await?;
            if facts.last {
                return Ok(());
            }
        }
    }

    /// What the dictionary of `part` names `ids`, each once, one Scan of one-key ranges a batch. An ID it answers no text for
    /// refuses the lookup with code 17, naming its key.
    /// A dropped future cancels its Scan as its stream drops.
    async fn lookup_names(
        part: &LookupPartition,
        tenant: u64,
        ids: &[SystemId],
    ) -> Result<Vec<(SystemId, String)>, ErrorReply> {
        let mut named = Vec::with_capacity(ids.len());
        for batch in ids.chunks(NAMES_PER_SCAN) {
            let keys: Vec<Vec<u8>> = batch.iter().map(|id| id_key(tenant, *id)).collect();
            let mut scan = Scan::new(ScanAt::ReadVersion, part.version, 0, 0, [0; 16], 0, 0);
            for key in &keys {
                let past = past_alone(key);
                scan.push_range(ScanRange::new(part.link.partition(), key, &past));
            }
            let mut stream = ScanStream::open(&part.link, &scan)?;
            let mut texts: HashMap<Vec<u8>, String> = HashMap::with_capacity(keys.len());
            // Each turn takes one frame of the Scan, awaited within the stream's stall bound; the frame the partition
            // marks last ends it at the latest (Ultra rule 2).
            loop {
                let (facts, ()) = stream
                    .next(|view| {
                        for entry in view.entries().iter() {
                            let text = std::str::from_utf8(entry.value().unwrap_or_default())
                                .map_err(|error| {
                                    ReadFault::Stored(format!(
                                        "the dictionary entry {} holds no text: {error}",
                                        hex(entry.key())
                                    ))
                                })?;
                            let _none = texts.insert(entry.key().to_vec(), text.to_string());
                        }
                        Ok(())
                    })
                    .await?;
                if facts.last {
                    break;
                }
            }
            for (id, key) in batch.iter().zip(&keys) {
                let text = texts.remove(key).ok_or_else(|| {
                    unavailable(format!(
                        "a list of partition {} names the dictionary entry {}, which the partition lacks",
                        part.link.partition(),
                        hex(key)
                    ))
                })?;
                named.push((*id, text));
            }
        }
        Ok(named)
    }

    /// The subjects the value index lists under `token` of `tokenizer` for `predicate` in the default graph of `tenant` at `at`,
    /// by their IRIs, merged over every partition of the tenant's run in key order (quad-reads, "A value-index lookup asks every
    /// partition"). The query core's own read; no client request names it.
    ///
    /// A dropped future cancels every Scan it holds as its stream drops, and its pins let go of the versions they held.
    ///
    /// # Errors
    ///
    /// What a partition refuses, relayed, and code 17 for a connection closing under the lookup.
    pub async fn value_index(
        self: &Rc<Self>,
        tenant: u64,
        at: LookupAt,
        predicate: &str,
        (tokenizer, token): (u8, &[u8]),
    ) -> Result<Vec<String>, ErrorReply> {
        let pins = ReadPins::at_once(self.times());
        let mut named: BTreeMap<SystemId, String> = BTreeMap::new();
        for (part, id) in self
            .lookup_partitions(tenant, (at, &pins), predicate)
            .await?
        {
            let Some(id) = id else {
                continue;
            };
            let key = QuadKey::ValueIndex {
                tenant,
                graph: Graph::Default,
                predicate: id,
                tokenizer,
                token: token.to_vec(),
            }
            .to_bytes();
            let past = past_alone(&key);
            let mut subjects = Vec::new();
            Self::lookup_lists(&part, &key, &past, |_, value| {
                subjects.extend(subjects_of(value)?);
                Ok(())
            })
            .await?;
            subjects.sort_unstable();
            subjects.dedup();
            named.extend(Self::lookup_names(&part, tenant, &subjects).await?);
        }
        Ok(named.into_values().collect())
    }

    /// The objects whose reverse count of `predicate` in the default graph of `tenant` at `at` is exactly `count`, summed over every
    /// partition of the tenant's run, since each lists an object under its own subjects' count alone (index-maintenance, "A reverse
    /// count sums the referrers' partitions"), by their IRIs, in key order. The query core's own read; no client request names it.
    ///
    /// A dropped future cancels every Scan it holds as its stream drops, and its pins let go of the versions they held.
    ///
    /// # Errors
    ///
    /// As [`ComputeCore::value_index`].
    pub async fn reverse_counted(
        self: &Rc<Self>,
        tenant: u64,
        at: LookupAt,
        predicate: &str,
        count: u64,
    ) -> Result<Vec<String>, ErrorReply> {
        let pins = ReadPins::at_once(self.times());
        let mut sums: BTreeMap<SystemId, (u64, usize)> = BTreeMap::new();
        let parts = self
            .lookup_partitions(tenant, (at, &pins), predicate)
            .await?;
        for (at_part, (part, id)) in parts.iter().enumerate() {
            let Some(id) = *id else {
                continue;
            };
            let key_of = |listed| {
                QuadKey::CountIndex {
                    tenant,
                    graph: Graph::Default,
                    predicate: id,
                    direction: CountDirection::Reverse,
                    count: listed,
                }
                .to_bytes()
            };
            let (first, past) = (key_of(0), key_of(u32::MAX));
            let mut listed: Vec<(u64, SystemId)> = Vec::new();
            Self::lookup_lists(part, &first, &past, |key, value| {
                let (parsed, _) =
                    QuadKey::parse(key).map_err(|error| ReadFault::Stored(error.to_string()))?;
                if let QuadKey::CountIndex { count: held, .. } = parsed {
                    for object in subjects_of(value)? {
                        listed.push((u64::from(held), object));
                    }
                }
                Ok(())
            })
            .await?;
            for (held, object) in listed {
                let sum = sums.entry(object).or_insert((0, at_part));
                sum.0 = sum.0.saturating_add(held);
            }
        }
        let mut wanted: BTreeMap<usize, Vec<SystemId>> = BTreeMap::new();
        for (object, (sum, at_part)) in sums {
            if sum == count {
                wanted.entry(at_part).or_default().push(object);
            }
        }
        let mut named: BTreeMap<SystemId, String> = BTreeMap::new();
        for (at_part, objects) in wanted {
            if let Some((part, _)) = parts.get(at_part) {
                named.extend(Self::lookup_names(part, tenant, &objects).await?);
            }
        }
        Ok(named.into_values().collect())
    }
}

/// The system IDs a list's object records hold.
fn subjects_of(value: &[u8]) -> Result<Vec<SystemId>, ReadFault> {
    let records =
        ObjectRecords::read(value).map_err(|error| ReadFault::Frame(error.to_string()))?;
    records
        .iter()
        .map(|record| {
            ultragraphdb_core_terms::EncodedTerm::from_bytes(record.encoding())
                .ok()
                .and_then(|term| term.as_id())
                .ok_or_else(|| {
                    ReadFault::Stored(
                        "an index list holds an object naming no system ID".to_string(),
                    )
                })
        })
        .collect()
}
