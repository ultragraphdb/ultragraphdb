/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The applier's workload on the storage simulator: a writer applies edges to an indexed scalar, an indexed list and
//! a uid predicate with reverse edges and counts; an administrator changes their indexes, the background rebuilds
//! running on the stepped executor, drops a predicate or the namespace's data, rolls lists up and syncs. Faults
//! fail batches, crash the process, cut its power, during drops too, and fail reads; after every step and every
//! recovery, every index the schema declares must agree with the data: Dgraph's indexes kept with their values.
//!
//! After a recovery, no key may lie under an index prefix the loaded schema does not declare: a rebuild a crash or a
//! power loss stopped leaves nothing of the indexes writes maintained for its new schema.
//!
//! The rules read the store through its trait alone, so the same workload runs on fjall.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::sync::Arc;

use ultragraphdb_core_catalog::{SchemaMode, parse as parse_schema};
use ultragraphdb_core_keys::{
    BYTE_COUNT, BYTE_COUNT_REV, BYTE_INDEX, attr_in_root_namespace, count_key, data_key, index_key,
    parse, predicate_prefix, reverse_key,
};
use ultragraphdb_core_tok::{build_tokens, get_tokenizer};
use ultragraphdb_core_types::Value;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_proto_pb::pb::{
    DirectedEdge, Mutations, SchemaUpdate, mutations::DropOp, posting,
};
use ultragraphdb_store_apply::{
    Applier, ApplyError, ApplyErrorEnum, prefixes_to_delete_tokens_for,
};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig, PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::{
    Event, FaultConfig, RunOutcome, Seed, SimContext, SimListCache, SimRng, Simulation,
    SteppedExecutor, Stream, Violation,
};
use ultragraphdb_traits_store::{PersistMode, Scan, Store, StoreSnapshot};

/// The rule an index breaks when it disagrees with the data it indexes.
pub const INDEX_MATCHES_DATA: &str =
    "every index the schema declares holds exactly the entries the data produces";
/// The rule a drop breaks when a key it dropped remains.
pub const DROP_REMOVES_ALL: &str = "a drop leaves no key under what it dropped";
/// The rule a recovery breaks when it leaves a key that writes maintained for a schema it did not load.
pub const NO_UNDECLARED_INDEX: &str =
    "after recovery no key exists under an index prefix the loaded schema does not declare";
/// The rule broken by an error no fault explains.
pub const NO_UNEXPECTED_ERROR: &str =
    "an apply operation fails only for a fault the simulator injected";

const UIDS: u64 = 6;
const PHRASES: [&str; 5] = ["big apple", "red apple", "green pear", "big fig", "red"];
const NAME_SCHEMAS: [&str; 4] = [
    "name: string .",
    "name: string @index(term) .",
    "name: string @index(exact) .",
    "name: string @index(term, exact) .",
];
const FRIEND_SCHEMAS: [&str; 4] = [
    "friend: [uid] .",
    "friend: [uid] @reverse .",
    "friend: [uid] @count .",
    "friend: [uid] @reverse @count .",
];
const TAGS_SCHEMA: &str = "tags: [string] @index(term) .";

/// The applier of a workload over `S`.
pub type WorkApplier<S> = Applier<S, SimListCache<CachedList>, SteppedExecutor>;

/// The applier over `store`, its rebuilds on `executor`.
pub fn applier_on<S>(
    store: S,
    executor: SteppedExecutor,
    seed: Seed,
) -> Result<Arc<WorkApplier<S>>, ApplyError>
where
    S: Store + Send + Sync + 'static,
{
    let cache = SimListCache::new(8, seed.stream(Stream::Cache));
    let lists = Lists::new(store, cache, ListsConfig::new(PersistMode::Buffer, 256));
    Ok(Arc::new(Applier::new(lists, executor, seed.value())?))
}

fn attr(name: &str) -> String {
    attr_in_root_namespace(name)
}

/// Whether `error` is one an injected fault explains, or a refusal the workload provokes on purpose.
pub fn expected(error: &ApplyError) -> bool {
    match &error.0 {
        ApplyErrorEnum::Posting(PostingError(PostingErrorEnum::Store(StoreError(
            StoreErrorEnum::WriteFailed(_)
            | StoreErrorEnum::Poisoned
            | StoreErrorEnum::ReadFailed { .. }
            | StoreErrorEnum::TsTooOld,
        ))))
        | ApplyErrorEnum::IndexingInProgress => true,
        // A schema change refused while a predicate holds data: the workload changes them at random. A delete of a
        // uid from a predicate a drop left without a schema, which Dgraph refuses alike.
        ApplyErrorEnum::ListToScalar { .. }
        | ApplyErrorEnum::ScalarUid { .. }
        | ApplyErrorEnum::ScalarIsUid { .. } => true,
        ApplyErrorEnum::Mutations { causes } => causes.iter().all(expected),
        ApplyErrorEnum::Schema(ultragraphdb_core_catalog::SchemaError(
            ultragraphdb_core_catalog::SchemaErrorEnum::Store(StoreError(
                StoreErrorEnum::ReadFailed { .. } | StoreErrorEnum::TsTooOld,
            )),
        )) => true,
        _ => false,
    }
}

/// One step of the writer: one to three edges in one proposal.
pub fn write<S>(a: &Arc<WorkApplier<S>>, rng: &mut SimRng) -> Result<(), ApplyError>
where
    S: Store + Send + Sync + 'static,
{
    let mut edges = Vec::new();
    for _ in 0..rng.between(1, 3) {
        let uid = rng.between(1, UIDS);
        let phrase = PHRASES[rng.index(PHRASES.len())];
        let edge = |name: &str| DirectedEdge {
            entity: uid,
            attr: attr(name),
            value: phrase.as_bytes().to_vec(),
            ..DirectedEdge::default()
        };
        let del = |mut e: DirectedEdge| {
            e.op = 1;
            e
        };
        let star = |name: &str| DirectedEdge {
            value: b"_STAR_ALL".to_vec(),
            ..del(edge(name))
        };
        let uid_edge = DirectedEdge {
            entity: uid,
            attr: attr("friend"),
            value_id: rng.between(1, UIDS),
            value_type: posting::ValType::Uid as i32,
            ..DirectedEdge::default()
        };
        edges.push(match rng.below(10) {
            0..2 => edge("name"),
            2 => del(edge("name")),
            3 => edge("tags"),
            4 => del(edge("tags")),
            5 => star(if rng.chance(500) { "name" } else { "tags" }),
            6..8 => uid_edge,
            8 => del(uid_edge),
            _ => star("friend"),
        });
    }
    let described: Vec<String> = edges
        .iter()
        .map(|e| {
            format!(
                "{}{} {} {}",
                if e.op == 1 { "-" } else { "+" },
                e.attr,
                e.entity,
                if e.value_id != 0 {
                    e.value_id.to_string()
                } else {
                    String::from_utf8_lossy(&e.value).into_owned()
                }
            )
        })
        .collect();
    let r = a.apply_mutations(&Mutations {
        start_ts: a.lists().latest_ts(),
        edges,
        ..Mutations::default()
    });
    tracing::debug!(target: "workload", edges = ?described, result = ?r, "write");
    r.map(|_| ())
}

/// The schema changes the workload starts with.
pub fn initial_schema() -> String {
    [NAME_SCHEMAS[1], FRIEND_SCHEMAS[3], TAGS_SCHEMA].join("\n")
}

/// Applies schema text.
pub fn alter<S>(a: &Arc<WorkApplier<S>>, text: &str) -> Result<(), ApplyError>
where
    S: Store + Send + Sync + 'static,
{
    let updates = parse_schema(text)
        .expect("the schema parses")
        .into_parts()
        .0;
    let r = a.alter(updates);
    tracing::debug!(target: "workload", text, result = ?r, "alter");
    r.map(|_| ())
}

/// One step of the administrator. Returns the prefixes a drop it made removed.
pub fn administer<S>(a: &Arc<WorkApplier<S>>, rng: &mut SimRng) -> Result<Vec<Vec<u8>>, ApplyError>
where
    S: Store + Send + Sync + 'static,
{
    let drop_of = |name: &str| {
        let p = predicate_prefix(&attr(name)).expect("a prefix");
        let mut twin = p.clone();
        twin[0] = 0x04;
        vec![p, twin]
    };
    match rng.below(20) {
        0..5 => alter(a, NAME_SCHEMAS[rng.index(NAME_SCHEMAS.len())]).map(|_| Vec::new()),
        5..9 => alter(a, FRIEND_SCHEMAS[rng.index(FRIEND_SCHEMAS.len())]).map(|_| Vec::new()),
        9 => {
            let name = if rng.chance(500) { "name" } else { "friend" };
            a.drop_predicate(&attr(name))?;
            Ok(drop_of(name))
        }
        10 => {
            a.apply_mutations(&Mutations {
                start_ts: a.lists().latest_ts(),
                drop_op: DropOp::Data as i32,
                drop_value: "0".into(),
                ..Mutations::default()
            })?;
            Ok(vec![vec![0x00, 0, 0, 0, 0, 0, 0, 0, 0]])
        }
        11..13 => {
            let key = data_key(
                &attr(if rng.chance(500) { "friend" } else { "tags" }),
                rng.between(1, UIDS),
            )
            .expect("a key");
            match a.lists().rollup_key(&key) {
                Ok(_) | Err(PostingError(PostingErrorEnum::HighPriorityOp)) => Ok(Vec::new()),
                Err(e) => Err(e.into()),
            }
        }
        13 => a.lists().sync().map(|_| Vec::new()).map_err(Into::into),
        // Defines the predicates a drop removed.
        _ => {
            let missing: Vec<&str> = [
                ("name", NAME_SCHEMAS[0]),
                ("friend", FRIEND_SCHEMAS[0]),
                ("tags", TAGS_SCHEMA),
            ]
            .into_iter()
            .filter(|(n, _)| a.schema().get(SchemaMode::Query, &attr(n)).is_none())
            .map(|(_, s)| s)
            .collect();
            if missing.is_empty() {
                Ok(Vec::new())
            } else {
                alter(a, &missing.join("\n")).map(|_| Vec::new())
            }
        }
    }
}

/// The postings of `key` at `read_ts`, each as its uid and its untagged value.
fn postings<S>(
    a: &WorkApplier<S>,
    key: &[u8],
    read_ts: u64,
) -> Result<Vec<(u64, Vec<u8>)>, ApplyError>
where
    S: Store + Send + Sync + 'static,
{
    let list = a.lists().read_uncached(key, read_ts)?;
    let mut out = Vec::new();
    list.iterate(read_ts, 0, |p| {
        out.push((p.uid, p.value.clone()));
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })?;
    Ok(out)
}

/// The uids of every live key under `prefix` at `read_ts`.
fn under<S>(
    a: &WorkApplier<S>,
    prefix: &[u8],
    read_ts: u64,
) -> Result<BTreeMap<Vec<u8>, BTreeSet<u64>>, ApplyError>
where
    S: Store + Send + Sync + 'static,
{
    let snapshot = a.lists().store().snapshot(read_ts)?;
    let mut out = BTreeMap::new();
    let keys: Vec<Vec<u8>> = snapshot
        .scan(&Scan::prefix(prefix.to_vec()))
        .map(|i| i.map(|i| i.key().to_vec()))
        .collect::<Result<_, _>>()?;
    for key in keys {
        if parse(&key)?.has_start_uid() {
            continue;
        }
        let uids: BTreeSet<u64> = postings(a, &key, read_ts)?
            .into_iter()
            .map(|(u, _)| u)
            .collect();
        if !uids.is_empty() {
            out.insert(key, uids);
        }
    }
    Ok(out)
}

/// What an index the rule checks found wrong, or `None`.
pub fn check<S>(a: &WorkApplier<S>) -> Result<Option<String>, ApplyError>
where
    S: Store + Send + Sync + 'static,
{
    if a.rebuilding() {
        return Ok(None);
    }
    let read_ts = a.lists().latest_ts();
    a.lists().gate(read_ts)?;
    // The string predicates: each tokenizer's keys hold the subjects whose values produce them.
    for name in ["name", "tags"] {
        let Some(schema) = a.schema().get(SchemaMode::Query, &attr(name)) else {
            continue;
        };
        for t in &schema.tokenizer {
            let tokenizer = get_tokenizer(t).expect("a tokenizer");
            let mut expected: BTreeMap<Vec<u8>, BTreeSet<u64>> = BTreeMap::new();
            for uid in 1..=UIDS {
                for (_, value) in postings(a, &data_key(&attr(name), uid)?, read_ts)? {
                    let text = String::from_utf8_lossy(&value).into_owned();
                    for token in build_tokens(&Value::str(text), &tokenizer)? {
                        expected
                            .entry(index_key(&attr(name), &token)?)
                            .or_default()
                            .insert(uid);
                    }
                }
            }
            let mut prefix = predicate_prefix(&attr(name))?;
            prefix.push(BYTE_INDEX);
            prefix.push(tokenizer.identifier());
            let actual = under(a, &prefix, read_ts)?;
            if actual != expected {
                return Ok(Some(format!(
                    "{name} {t}: index {actual:?}, data {expected:?}"
                )));
            }
        }
    }
    // The uid predicate: its reverse edges and its counts.
    if let Some(schema) = a.schema().get(SchemaMode::Query, &attr("friend")) {
        let mut targets: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
        let mut counts: BTreeMap<Vec<u8>, BTreeSet<u64>> = BTreeMap::new();
        for uid in 1..=UIDS {
            let held: Vec<u64> = postings(a, &data_key(&attr("friend"), uid)?, read_ts)?
                .into_iter()
                .map(|(u, _)| u)
                .collect();
            if !held.is_empty() {
                counts
                    .entry(count_key(&attr("friend"), held.len() as u32, false)?)
                    .or_default()
                    .insert(uid);
            }
            for target in held {
                targets.entry(target).or_default().insert(uid);
            }
        }
        if schema.directive == 2 {
            let expected: BTreeMap<Vec<u8>, BTreeSet<u64>> = targets
                .iter()
                .map(|(t, s)| Ok((reverse_key(&attr("friend"), *t)?, s.clone())))
                .collect::<Result<_, ApplyError>>()?;
            let mut prefix = predicate_prefix(&attr("friend"))?;
            prefix.push(ultragraphdb_core_keys::BYTE_REVERSE);
            let actual = under(a, &prefix, read_ts)?;
            if actual != expected {
                return Ok(Some(format!(
                    "friend reverse: {actual:?}, data {expected:?}"
                )));
            }
        }
        if schema.count {
            let mut prefix = predicate_prefix(&attr("friend"))?;
            prefix.push(BYTE_COUNT);
            let actual = under(a, &prefix, read_ts)?;
            if actual != counts {
                return Ok(Some(format!("friend counts: {actual:?}, data {counts:?}")));
            }
            if schema.directive == 2 {
                let mut expected: BTreeMap<Vec<u8>, BTreeSet<u64>> = BTreeMap::new();
                for (t, sources) in &targets {
                    expected
                        .entry(count_key(&attr("friend"), sources.len() as u32, true)?)
                        .or_default()
                        .insert(*t);
                }
                let mut prefix = predicate_prefix(&attr("friend"))?;
                prefix.push(BYTE_COUNT_REV);
                let actual = under(a, &prefix, read_ts)?;
                if actual != expected {
                    return Ok(Some(format!(
                        "friend reverse counts: {actual:?}, data {expected:?}"
                    )));
                }
            }
        }
    }
    Ok(None)
}

/// The index prefixes the workload's schemas use for `name`, each with its split twin, that `schema` does not
/// declare.
fn undeclared_prefixes(
    name: &str,
    schema: Option<&SchemaUpdate>,
) -> Result<Vec<Vec<u8>>, ApplyError> {
    let a = attr(name);
    let mut prefixes = Vec::new();
    if name == "friend" {
        let kind = |byte: u8| -> Result<Vec<Vec<u8>>, ApplyError> {
            let mut p = predicate_prefix(&a)?;
            p.push(byte);
            let mut twin = p.clone();
            twin[0] = 0x04;
            Ok(vec![p, twin])
        };
        let reverse = schema.is_some_and(|s| s.directive == 2);
        let count = schema.is_some_and(|s| s.count);
        for (byte, declared) in [
            (ultragraphdb_core_keys::BYTE_REVERSE, reverse),
            (BYTE_COUNT, count),
            (BYTE_COUNT_REV, count && reverse),
        ] {
            if !declared {
                prefixes.extend(kind(byte)?);
            }
        }
        return Ok(prefixes);
    }
    for t in ["term", "exact"] {
        if schema.is_some_and(|s| s.tokenizer.iter().any(|d| d == t)) {
            continue;
        }
        prefixes.extend(prefixes_to_delete_tokens_for(&a, t, false)?);
        if t == "exact" {
            prefixes.extend(prefixes_to_delete_tokens_for(&a, t, true)?);
        }
    }
    Ok(prefixes)
}

/// The first key under an index prefix the loaded schema does not declare, live at the newest timestamp: what a
/// rebuild stopped by a crash or a power loss must not leave.
pub fn undeclared_index_key<S>(a: &WorkApplier<S>) -> Result<Option<String>, ApplyError>
where
    S: Store + Send + Sync + 'static,
{
    let read_ts = a.lists().latest_ts();
    a.lists().gate(read_ts)?;
    let snapshot = a.lists().store().snapshot(read_ts)?;
    for name in ["name", "tags", "friend"] {
        let schema = a.schema().get(SchemaMode::Query, &attr(name));
        for prefix in undeclared_prefixes(name, schema.as_ref())? {
            if let Some(item) = snapshot.scan(&Scan::prefix(prefix)).next() {
                let key = item?.key().to_vec();
                return Ok(Some(format!(
                    "{name} under {:?}: key {key:?}",
                    schema.map(|s| (s.tokenizer, s.directive, s.count))
                )));
            }
        }
    }
    Ok(None)
}

/// Whether a key remains under `prefixes`, any version.
fn remains<S>(a: &WorkApplier<S>, prefixes: &[Vec<u8>]) -> Result<bool, ApplyError>
where
    S: Store + Send + Sync + 'static,
{
    let snapshot = a.lists().store().snapshot(u64::MAX)?;
    for p in prefixes {
        if snapshot
            .scan(&Scan::prefix(p.clone()).all_versions())
            .next()
            .transpose()?
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The workload on the simulator.
#[derive(Default)]
pub struct ApplyWorkload {
    applier: Option<Arc<WorkApplier<ultragraphdb_store_sim::SimStore>>>,
    pub checks: usize,
    pub writes: usize,
    pub changes: usize,
    pub drops: usize,
    /// The recoveries after which the undeclared-index rule was checked.
    pub recoveries: usize,
}

impl ApplyWorkload {
    pub fn faults() -> FaultConfig {
        FaultConfig::mixed().with_corruption(0)
    }

    /// Runs `steps` steps from `seed`: the outcome, how many index checks found agreement, writes and changes made.
    pub fn run(
        seed: Seed,
        faults: FaultConfig,
        steps: u64,
    ) -> Result<(RunOutcome, ApplyWorkload), Violation> {
        let mut workload = Self::default();
        let outcome =
            Simulation::new(seed, faults).run(steps, 2, |ctx, event| workload.step(ctx, event))?;
        Ok((outcome, workload))
    }

    fn applier(
        &mut self,
        ctx: &SimContext,
    ) -> Result<Arc<WorkApplier<ultragraphdb_store_sim::SimStore>>, ApplyError> {
        if let Some(a) = &self.applier {
            return Ok(Arc::clone(a));
        }
        let a = applier_on(ctx.store().clone(), ctx.executor().clone(), ctx.seed())?;
        if a.schema().get(SchemaMode::Query, &attr("name")).is_none() {
            alter(&a, &initial_schema())?;
        }
        self.applier = Some(Arc::clone(&a));
        Ok(a)
    }

    pub fn step(&mut self, ctx: &mut SimContext, event: Event) -> Result<(), Violation> {
        if event == Event::Recovered {
            self.applier = None;
        }
        let a = match self.applier(ctx) {
            Ok(a) => a,
            Err(e) if expected(&e) => return Ok(()),
            Err(e) => return Err(ctx.violation(NO_UNEXPECTED_ERROR, e.to_string())),
        };
        if event == Event::Recovered {
            match undeclared_index_key(&a) {
                Ok(None) => {}
                Ok(Some(detail)) => return Err(ctx.violation(NO_UNDECLARED_INDEX, detail)),
                Err(e) if expected(&e) => {}
                Err(e) => return Err(ctx.violation(NO_UNEXPECTED_ERROR, e.to_string())),
            }
            self.recoveries += 1;
        }
        let result = match event {
            Event::Recovered => Ok(Vec::new()),
            Event::Step(0) => {
                let r = write(&a, ctx.rng());
                self.writes += usize::from(r.is_ok());
                r.map(|_| Vec::new())
            }
            Event::Step(_) => {
                let r = administer(&a, ctx.rng());
                self.changes += usize::from(r.is_ok());
                r
            }
        };
        match result {
            Ok(dropped) if !dropped.is_empty() => {
                self.drops += 1;
                match remains(&a, &dropped) {
                    Ok(true) => return Err(ctx.violation(DROP_REMOVES_ALL, format!("{dropped:?}"))),
                    Ok(false) => {}
                    Err(e) if expected(&e) => {}
                    Err(e) => return Err(ctx.violation(NO_UNEXPECTED_ERROR, e.to_string())),
                }
            }
            Ok(_) => {}
            Err(e) if expected(&e) => {}
            Err(e) => return Err(ctx.violation(NO_UNEXPECTED_ERROR, e.to_string())),
        }
        if ctx.store().needs_recovery() {
            return Ok(());
        }
        match check(&a) {
            Ok(None) => {
                self.checks += 1;
                Ok(())
            }
            Ok(Some(detail)) => Err(ctx.violation(INDEX_MATCHES_DATA, detail)),
            Err(e) if expected(&e) => Ok(()),
            Err(e) => Err(ctx.violation(NO_UNEXPECTED_ERROR, e.to_string())),
        }
    }
}
