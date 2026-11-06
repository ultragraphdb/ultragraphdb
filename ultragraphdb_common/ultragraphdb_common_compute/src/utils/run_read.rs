/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use compio::runtime::spawn;
use futures::channel::mpsc::UnboundedSender;
use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_core_ids::{Epoch, SystemId};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Closed, ClosedView, Command, ErrorReply, ErrorReplyView, FLAG_LAST,
    Header, ReadLevel, RequestTerm, ResumeKeyView,
};
use ultragraphdb_store_quads::{ReferrerMember, ReferrerSet, iri_bucket};

use crate::types::compute_core::ComputeCore;
use crate::types::frontier::Frontier;
use crate::types::link::Link;
use crate::types::partition_rows::{PartitionRows, terms_of};
use crate::types::pushed::Pushed;
use crate::types::read_at::ReadAt;
use crate::types::read_pins::ReadPins;
use crate::types::read_request::ReadRequest;
use crate::types::read_route::ReadRoute;
use crate::types::read_shared::ReadShared;
use crate::types::result_sink::ResultSink;
use crate::types::resume_at::ResumeAt;
use crate::types::resume_terms::ResumeTerms;
use crate::utils::literals::{datatype_named, encoding_of};
use crate::utils::refusals::{resume_refused, unavailable};

/// Runs one read on this core to its end: its stream's last frame is its last batch or its refusal.
pub(crate) async fn run_read(
    core: Rc<ComputeCore>,
    read: ReadRequest,
    sink: ResultSink,
    pushes: UnboundedSender<Pushed>,
) {
    let (cluster, stream) = (core.cluster_id(), read.stream);
    if let Err(refusal) = answer(&core, read, sink).await {
        let header = Header::new(cluster, Command::Error)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        match refusal.frame(header) {
            // A session gone holds no stream to answer on.
            Ok(frame) => drop(pushes.unbounded_send(Pushed::Frame(stream, frame))),
            Err(rule) => tracing::warn!(%rule, "a read's refusal was not built"),
        }
    }
    // A session gone holds no read to end.
    drop(pushes.unbounded_send(Pushed::Ended(stream)));
}

/// The read's answer: its partitions' Scans, then its batches, once each has closed the read's epoch.
async fn answer(
    core: &Rc<ComputeCore>,
    read: ReadRequest,
    mut sink: ResultSink,
) -> Result<(), ErrorReply> {
    let resume = resume_terms(&read)?;
    let map = core.placement().await?;
    let route = ReadRoute::of(&read, &map)?;
    let limit = read.limit;
    let shared = Rc::new(ReadShared {
        read,
        remaining: Cell::new(limit),
        pins: Rc::default(),
    });
    // Dropped with the read, which cancels every Scan it holds.
    let _pins = (shared.read.read_only).then(|| {
        spawn(ReadPins::hold(
            Rc::clone(&shared.pins),
            core.times().pin_after(),
            core.times().pin_every(),
        ))
    });
    let Opened { mut parts, reached } = opened_parts(core, &shared, &map, &route).await?;
    if let Some(terms) = &resume {
        let at = resume_at(core, &shared, &mut parts, terms).await?;
        parts.iter_mut().for_each(|part| part.resume_at(at.clone()));
    }
    let first = parts.first().and_then(PartitionRows::chosen);
    let epoch = answered_epoch(&shared.read, first.map(|(_, epoch)| epoch));
    sink.read_at(
        epoch,
        if reached == 1 {
            parts.first().map_or(0, PartitionRows::partition)
        } else {
            0
        },
    );
    merge(&shared, parts, sink).await
}

/// The epoch a batch names: the snapshot's at level 0, the read's at level 2, 0 for every other read.
fn answered_epoch(read: &ReadRequest, chosen: Option<u64>) -> u64 {
    if !read.read_only || read.named {
        return 0;
    }
    match read.level {
        ReadLevel::Snapshot => read.epoch,
        ReadLevel::Latest => 0,
        ReadLevel::MonotonicFast => chosen.unwrap_or(read.epoch),
    }
}

/// The resume key's last row, checked in the protocol's order; `None` for a read not resumed.
fn resume_terms(read: &ReadRequest) -> Result<Option<ResumeTerms>, ErrorReply> {
    if read.resume.is_empty() {
        return Ok(None);
    }
    let key = ResumeKeyView::read(&read.resume).map_err(|rule| resume_refused(&rule))?;
    if key.pattern() != read.pattern_hash {
        return Err(resume_refused(&"it continues another pattern"));
    }
    if key.scope() != read.scope() {
        return Err(resume_refused(&"it belongs to another transaction"));
    }
    ResumeTerms::of(&key)
        .map(Some)
        .ok_or_else(|| resume_refused(&"its last row holds a term no row names"))
}

/// The partitions a read goes on reading, and how many it has reached.
struct Opened {
    parts: Vec<PartitionRows>,
    reached: usize,
}

/// The read's partitions, each past its dictionary Scan: a partition's terms resolved and its epoch closed.
async fn opened_parts(
    core: &Rc<ComputeCore>,
    shared: &Rc<ReadShared>,
    map: &PlacementMap,
    route: &ReadRoute,
) -> Result<Opened, ErrorReply> {
    match route {
        ReadRoute::Partitions(partitions) => {
            let first = first_at(core, shared, partitions).await?;
            let parts = gated_parts(core, shared, partitions, first, Vec::new(), None).await?;
            Ok(Opened {
                reached: parts.len(),
                parts,
            })
        }
        ReadRoute::Reverse { home } => reverse_parts(core, shared, map, *home).await,
    }
}

/// How a read of `partitions` first reads each: past a Closed of each for a monotonic-fast read reaching several, every Closed
/// sent before any answer is awaited, so the read waits on the slowest partition rather than on all of them in turn.
async fn first_at(
    core: &Rc<ComputeCore>,
    shared: &Rc<ReadShared>,
    partitions: &[u32],
) -> Result<ReadAt, ErrorReply> {
    let read = &shared.read;
    if !(read.read_only && read.level == ReadLevel::MonotonicFast && partitions.len() > 1) {
        return Ok(ReadAt::of(read));
    }
    let closed = futures::future::try_join_all(
        partitions
            .iter()
            .map(|partition| closed_epoch(core, *partition)),
    )
    .await?;
    let lowest = closed.into_iter().min().unwrap_or(u64::MAX);
    Ok(ReadAt::at_epoch(lowest.max(read.epoch)))
}

/// A stream of a link asked one question, forgotten however its wait ends, a read cancelled meanwhile included.
struct Asked {
    link: Rc<Link>,
    stream: u64,
}

impl Drop for Asked {
    fn drop(&mut self) {
        self.link.forget(self.stream);
    }
}

/// The latest epoch `partition` has closed. A partition silent past [`Link::answer_wait`] is hung, and its connection closes.
async fn closed_epoch(core: &Rc<ComputeCore>, partition: u32) -> Result<u64, ErrorReply> {
    use futures::StreamExt;

    let link = core.link(partition).await?;
    let (stream, mut frames) = link.open_stream()?;
    let asked = Asked {
        link: Rc::clone(&link),
        stream,
    };
    link.send(stream, &Closed::request(&[partition]))?;
    let Ok(next) = compio::time::timeout(link.answer_wait(), frames.next()).await else {
        let why = format!(
            "partition {partition}'s answer to a Closed did not come within {:?}",
            link.answer_wait()
        );
        link.close(&why);
        return Err(unavailable(why));
    };
    drop(asked);
    let frame = next.ok_or_else(|| link.closed_under_read())?;
    let view = frame.view().map_err(|rule| {
        link.close(&rule);
        link.closed_under_read()
    })?;
    if view.command() == Command::Error {
        let error =
            ErrorReplyView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
        return Err(ErrorReply::new(error.code(), error.message()).with_detail(error.detail()));
    }
    let closed = ClosedView::from_frame(&view).map_err(|rule| {
        link.close(&rule);
        link.closed_under_read()
    })?;
    Ok(closed.partition(0).map_or(0, |held| held.epoch()))
}

/// Sends every partition's dictionary Scan, then waits for each, so none's list Scan waits on another's close. `held` are
/// partitions already past theirs.
async fn gated_parts(
    core: &Rc<ComputeCore>,
    shared: &Rc<ReadShared>,
    partitions: &[u32],
    first: ReadAt,
    held: Vec<PartitionRows>,
    home_object: Option<SystemId>,
) -> Result<Vec<PartitionRows>, ErrorReply> {
    let object_from_home = home_object.is_some();
    let terms = terms_of(&shared.read, object_from_home);
    let mut open = Vec::with_capacity(partitions.len());
    for partition in partitions {
        let link = core.link(*partition).await?;
        let part = PartitionRows::new(link, Rc::clone(shared), first);
        let gate = part.open_gate(terms.clone())?;
        open.push((part, gate));
    }
    let mut parts = held;
    for (mut part, gate) in open {
        let ids = part.finish_gate(gate).await?;
        part.plan(&ids, home_object)?;
        parts.push(part);
    }
    parts.sort_by_key(PartitionRows::partition);
    Ok(parts)
}

/// A reverse read's partitions: the object's home, for its referrer set at the read's epoch, then the partitions the set names,
/// each past its own dictionary Scan at that epoch (`PROTOCOL.md` §11.12).
async fn reverse_parts(
    core: &Rc<ComputeCore>,
    shared: &Rc<ReadShared>,
    map: &PlacementMap,
    home: u32,
) -> Result<Opened, ErrorReply> {
    let read = &shared.read;
    let link = core.link(home).await?;
    let mut at_home = PartitionRows::new(Rc::clone(&link), Rc::clone(shared), ReadAt::of(read));
    let terms = terms_of(read, false);
    let gate = at_home.open_gate(terms)?;
    let ids = at_home.finish_gate(gate).await?;
    let object = match &read.object {
        Some(RequestTerm::Iri(iri)) => ids.get(iri).copied(),
        _ => None,
    };
    let Some(object) = object else {
        at_home.plan(&ids, None)?;
        return Ok(Opened {
            parts: vec![at_home],
            reached: 1,
        });
    };
    let key = QuadKey::Referrers {
        tenant: read.tenant,
        entity: object,
    }
    .to_bytes();
    // At a named version, one within an epoch the home may not have closed, the member of an edge at or below that version may
    // stand above it in the home's own order: the set read at the latest version, and held to the version's epoch, holds it.
    let held_set = if read.named {
        PartitionRows::new(link, Rc::clone(shared), ReadAt::latest())
            .values(std::slice::from_ref(&key))
            .await?
    } else {
        at_home.values(std::slice::from_ref(&key)).await?
    };
    let set = match held_set.first() {
        Some((_, value)) => ReferrerSet::decode(&key, value).map_err(|error| {
            unavailable(format!("the referrer set at partition {home}: {error}"))
        })?,
        None => ReferrerSet::new(),
    };
    let epoch = at_home.chosen().map_or(read.epoch, |(_, epoch)| epoch);
    let at = Epoch::new(epoch).unwrap_or(Epoch::ZERO);
    let members: Vec<u32> = map
        .holders_of_groups(set.at(at).map(ReferrerMember::group), at)
        .into_iter()
        .map(|(holder, _)| holder.get())
        .collect();
    let mut reached = members.clone();
    if !reached.contains(&home) {
        reached.push(home);
        reached.sort_unstable();
    }
    ReadRoute::check_reached(read, &reached)?;
    let others: Vec<u32> = members.iter().copied().filter(|p| *p != home).collect();
    let first = member_at(read, epoch);
    let mut held = Vec::new();
    if members.contains(&home) {
        at_home.plan(&ids, Some(object))?;
        held.push(at_home);
    } else if others.is_empty() {
        at_home.plan(&ids, Some(object))?;
        at_home.end();
        held.push(at_home);
    }
    let parts = gated_parts(core, shared, &others, first, held, Some(object)).await?;
    Ok(Opened {
        parts,
        reached: reached.len(),
    })
}

/// How a reverse read first reads a member of its home's referrer set, at the epoch the home answered.
fn member_at(read: &ReadRequest, epoch: u64) -> ReadAt {
    if read.read_only && read.level == ReadLevel::MonotonicFast {
        ReadAt::at_epoch(epoch)
    } else {
        ReadAt::of(read)
    }
}

/// The IDs of the resume key's terms: read at each term's home, since one ID must key several partitions, or, in a read-write
/// transaction, at its partition, which takes them from their homes itself.
async fn resume_at(
    core: &Rc<ComputeCore>,
    shared: &Rc<ReadShared>,
    parts: &mut [PartitionRows],
    terms: &ResumeTerms,
) -> Result<ResumeAt, ErrorReply> {
    let read = &shared.read;
    let mut named: Vec<&str> = vec![terms.subject.as_str(), terms.predicate.as_str()];
    named.extend(terms.graph.as_deref());
    named.extend(terms.reifier.as_deref());
    match &terms.object {
        RequestTerm::Iri(iri) => named.push(iri),
        other @ (RequestTerm::BlankNode(_)
        | RequestTerm::Literal { .. }
        | RequestTerm::LanguageLiteral { .. }
        | RequestTerm::Triple { .. }) => named.extend(datatype_named(other)),
    }
    let ids = ids_at_homes(core, shared, parts, &named).await?;
    let id_of = |iri: &str| {
        ids.get(iri).copied().ok_or_else(|| {
            resume_refused(&format!(
                "the term {iri} of tenant {} has no entry at its home",
                read.tenant
            ))
        })
    };
    let object = match &terms.object {
        RequestTerm::Iri(iri) => ultragraphdb_core_terms::EncodedTerm::id(id_of(iri)?),
        other @ (RequestTerm::BlankNode(_)
        | RequestTerm::Literal { .. }
        | RequestTerm::LanguageLiteral { .. }
        | RequestTerm::Triple { .. }) => {
            let datatype = match datatype_named(other) {
                Some(iri) => Some(id_of(iri)?),
                None => None,
            };
            encoding_of(other, datatype)
                .map_err(|error| resume_refused(&error))?
                .ok_or_else(|| resume_refused(&"its object is no term"))?
        }
    };
    Ok(ResumeAt {
        graph: match &terms.graph {
            Some(iri) => Graph::Named(id_of(iri)?),
            None => Graph::Default,
        },
        subject: id_of(&terms.subject)?,
        predicate: id_of(&terms.predicate)?,
        object,
        statement: match &terms.reifier {
            Some(iri) => Some(id_of(iri)?),
            None => None,
        },
    })
}

/// The IDs `named` have at their homes: each home asked in one Scan of one-key ranges.
async fn ids_at_homes(
    core: &Rc<ComputeCore>,
    shared: &Rc<ReadShared>,
    parts: &mut [PartitionRows],
    named: &[&str],
) -> Result<HashMap<String, SystemId>, ErrorReply> {
    let read = &shared.read;
    let map = core.placement().await?;
    let mut homes: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for iri in named {
        let home = if read.is_read_write() {
            read.partition
        } else {
            iri_bucket(read.tenant, iri).map_or(read.partition, |bucket| {
                map.partition_of(bucket, Epoch::ZERO).get()
            })
        };
        homes.entry(home).or_default().push((*iri).to_string());
    }
    let mut ids = HashMap::new();
    for (home, terms) in homes {
        if let Some(part) = parts.iter_mut().find(|part| part.partition() == home) {
            let gate = part.open_gate(terms)?;
            ids.extend(part.finish_gate(gate).await?);
        } else {
            let link = core.link(home).await?;
            let mut part = PartitionRows::new(link, Rc::clone(shared), ReadAt::of(read));
            let gate = part.open_gate(terms)?;
            ids.extend(part.finish_gate(gate).await?);
        }
    }
    Ok(ids)
}

/// Merges the partitions' rows in key order into batches, up to the read's limit, and ends the stream: with a resume key when
/// the limit stopped it with a row left.
async fn merge(
    shared: &Rc<ReadShared>,
    mut parts: Vec<PartitionRows>,
    mut sink: ResultSink,
) -> Result<(), ErrorReply> {
    let read = &shared.read;
    let order = |rows: &[PartitionRows], a: usize, b: usize| {
        let row = |at: usize| rows.get(at).and_then(|part| part.page().peek());
        match (row(a), row(b)) {
            (Some(a), Some(b)) => a.order(&b),
            _ => Ordering::Equal,
        }
    };
    // Every partition's first page is read at once, so the read waits on the slowest rather than on each in turn.
    let _filled = futures::future::try_join_all(parts.iter_mut().map(PartitionRows::fill)).await?;
    let mut frontier = Frontier::with_capacity(parts.len());
    for at in 0..parts.len() {
        if parts.get(at).is_some_and(|part| part.page().has_row()) {
            let held = &parts;
            frontier.push(at, |a, b| order(held, a, b));
        }
    }
    let mut sent = 0u32;
    let mut resume = Vec::new();
    loop {
        let popped = {
            let held = &parts;
            frontier.pop(|a, b| order(held, a, b))
        };
        let Some(at) = popped else {
            break;
        };
        let Some(part) = parts.get_mut(at) else {
            break;
        };
        let cells = part.cells()?;
        sink.push_row(&cells.columns(read)).await?;
        sent = sent.saturating_add(1);
        if read.limit != 0 {
            shared.remaining.set(read.limit.saturating_sub(sent).max(1));
        }
        let stops = read.limit != 0 && sent == read.limit;
        let key = if stops {
            Some(part.resume_key_of_next()?)
        } else {
            None
        };
        part.advance();
        if let Some(key) = key {
            let mut more = !frontier.streams().is_empty();
            more = more || part.has_more().await?;
            if more {
                resume = key;
            }
            break;
        }
        part.fill().await?;
        if part.page().has_row() {
            let held = &parts;
            frontier.push(at, |a, b| order(held, a, b));
        }
    }
    sink.finish(resume).await
}
