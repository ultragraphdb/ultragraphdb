/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The term dictionary's workload on the storage simulator: batches of writes create IRIs, blank nodes and literals
//! short and long, insert and delete quads naming them, and roll lists up; after every step and every recovery every
//! ID and literal hash a stored list names has its dictionary entry, no ID names two terms and no term has two IDs.
//! The block source, the metadata group's stand-in, survives every crash, and each store draws names from a seed of its
//! own, as a partition seeds its random source afresh at every start: two starts drawing from one seed draw one
//! sequence of names, and the later must draw past every name the earlier committed.

use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::Arc;

use oxrdf::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_core_terms::FLOAT32_VECTOR;
use ultragraphdb_error_store::StoreErrorEnum;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig, PostingErrorEnum};
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum, QuadStore, QuadWrite, WriteTerms};
use ultragraphdb_store_sim::{
    Event, FaultConfig, RunOutcome, Seed, SimContext, SimListCache, SimStore, Simulation, Stream,
    Violation,
};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_traits_store::{PersistMode, Scan, Store, StoreSnapshot};

use crate::utils::helpers::kind;

/// The rule a stored term without its dictionary entry breaks.
pub const DICTIONARY_COMPLETE: &str =
    "every ID and literal hash a stored list names has its dictionary entry";
/// The rule an ID naming a second term breaks.
pub const IDS_NEVER_REISSUED: &str = "an ID names one term for good";
/// The rule a term named by a second ID breaks.
pub const ONE_ID_PER_TERM: &str = "a term has one ID for good";
/// The rule broken by an error no fault explains.
pub const NO_UNEXPECTED_ERROR: &str =
    "a dictionary operation fails only for a fault the simulator injected";

type SimQuads = QuadStore<SimStore, SimListCache<CachedList>, TestBlockSource>;

pub struct DictionaryWorkload {
    quads: Option<Arc<SimQuads>>,
    /// The stores opened so far, which seeds the next one's random source.
    starts: u64,
    /// The metadata group's stand-in, which outlives every crash: blocks of 50 sequences, so runs cross blocks.
    source: TestBlockSource,
    next_iri: u64,
    /// The term each ID was first seen naming.
    seen: BTreeMap<SystemId, String>,
    /// The ID each term was first seen named by.
    named: BTreeMap<String, SystemId>,
    pub checked: usize,
}

impl Default for DictionaryWorkload {
    fn default() -> Self {
        Self {
            quads: None,
            starts: 0,
            source: TestBlockSource::new(NonZeroU64::MIN.saturating_add(49)),
            next_iri: 0,
            seen: BTreeMap::new(),
            named: BTreeMap::new(),
            checked: 0,
        }
    }
}

impl DictionaryWorkload {
    /// The faults of a run: every class but corruption, which a posting list cannot always detect.
    pub fn faults() -> FaultConfig {
        FaultConfig::mixed().with_corruption(0)
    }

    pub fn run(
        seed: Seed,
        faults: FaultConfig,
        steps: u64,
    ) -> Result<(RunOutcome, usize), Violation> {
        let mut w = Self::default();
        let outcome =
            Simulation::new(seed, faults).run(steps, 1, |ctx, event| w.step(ctx, event))?;
        Ok((outcome, w.checked))
    }

    fn quads(&mut self, ctx: &SimContext) -> Result<Arc<SimQuads>, QuadError> {
        if let Some(q) = &self.quads {
            return Ok(q.clone());
        }
        let cache = SimListCache::new(8, ctx.seed().stream(Stream::Cache));
        let lists = Lists::new(
            ctx.store().clone(),
            cache,
            ListsConfig::new(PersistMode::Buffer, 512),
        );
        self.starts += 1;
        let random = RandomSource::new(
            ctx.seed()
                .value()
                .wrapping_mul(1_000)
                .wrapping_add(self.starts),
        );
        let q = Arc::new(QuadStore::open(lists, self.source.clone(), random)?);
        self.quads = Some(q.clone());
        Ok(q)
    }

    pub fn step(&mut self, ctx: &mut SimContext, event: Event) -> Result<(), Violation> {
        if let Event::Recovered = event {
            self.quads = None;
        } else {
            let result = self.write(ctx);
            self.tolerate(ctx, result)?;
        }
        if ctx.store().needs_recovery() {
            return Ok(());
        }
        let result = self.check(ctx);
        match result {
            Ok(None) => Ok(()),
            Ok(Some((rule, detail))) => Err(ctx.violation(rule, detail)),
            Err(e) => self.tolerate(ctx, Err(e)),
        }
    }

    /// An error is the workload's own when no fault explains it.
    fn tolerate(
        &self,
        ctx: &mut SimContext,
        result: Result<(), QuadError>,
    ) -> Result<(), Violation> {
        let Err(error) = result else {
            return Ok(());
        };
        let fault = |e: &StoreErrorEnum| {
            matches!(
                e,
                StoreErrorEnum::WriteFailed(_)
                    | StoreErrorEnum::Poisoned
                    | StoreErrorEnum::ReadFailed { .. }
                    | StoreErrorEnum::TsTooOld
            )
        };
        let injected = match &error.0 {
            QuadErrorEnum::Store(e) => fault(&e.0),
            QuadErrorEnum::Posting(p) => matches!(&p.0, PostingErrorEnum::Store(e) if fault(&e.0)),
            QuadErrorEnum::Term(_)
            | QuadErrorEnum::Key(_)
            | QuadErrorEnum::Mapping(_)
            | QuadErrorEnum::Catalog(_)
            | QuadErrorEnum::Id(_)
            | QuadErrorEnum::Corrupted { .. }
            | QuadErrorEnum::NoSequence { .. }
            | QuadErrorEnum::PendingFull { .. }
            | QuadErrorEnum::NoOwnedBucket { .. }
            | QuadErrorEnum::SkolemDraws { .. }
            | QuadErrorEnum::NotOwned { .. }
            | QuadErrorEnum::UnknownSkolem { .. } => false,
        };
        if injected || ctx.store().needs_recovery() {
            Ok(())
        } else {
            Err(ctx.violation(NO_UNEXPECTED_ERROR, error.to_string()))
        }
    }

    fn term(&mut self, ctx: &mut SimContext) -> Term {
        match ctx.rng().below(10) {
            0..3 => {
                NamedNode::new_unchecked(format!("http://ex.org/s{}", ctx.rng().below(6))).into()
            }
            3 => {
                self.next_iri += 1;
                NamedNode::new_unchecked(format!(
                    "http://ex.org/new{}-{}",
                    ctx.seed(),
                    self.next_iri
                ))
                .into()
            }
            4 => BlankNode::new_unchecked(format!("b{}", ctx.rng().below(3))).into(),
            5 => Literal::new_simple_literal(format!("v{}", ctx.rng().below(4))).into(),
            6 => Literal::new_simple_literal(format!(
                "a long literal, hashed {}",
                ctx.rng().below(4)
            ))
            .into(),
            // `en-GB` as the checked constructor lowercases it.
            7 => Literal::new_language_tagged_literal_unchecked(
                format!("label {}", ctx.rng().below(9999)),
                "en-gb",
            )
            .into(),
            8 => Literal::new_typed_literal(
                format!("{}", ctx.rng().below(9)),
                NamedNode::new_unchecked("http://ex.org/dt"),
            )
            .into(),
            _ => Literal::new_typed_literal(
                format!("[{}, 1]", ctx.rng().below(9)),
                NamedNode::new_unchecked(FLOAT32_VECTOR),
            )
            .into(),
        }
    }

    fn write(&mut self, ctx: &mut SimContext) -> Result<(), QuadError> {
        let quads = self.quads(ctx)?;
        if ctx.rng().chance(100) {
            let key = self.some_key(&quads, ctx)?;
            if let Some(key) = key {
                let _rollup_key: bool = quads.lists().rollup_key(&key)?;
            }
            return Ok(());
        }
        if ctx.rng().chance(80) {
            return Ok(quads.lists().sync()?);
        }
        let mut batch = quads.lists().batch();
        let mut terms: Vec<WriteTerms> = Vec::new();
        for _ in 0..ctx.rng().between(1, 3) {
            let mut write = QuadWrite::new(&quads, 0, batch.request(quads.lists().applied_ts()));
            for _ in 0..ctx.rng().between(1, 4) {
                let subject: NamedOrBlankNode = match self.term(ctx) {
                    Term::NamedNode(n) => n.into(),
                    Term::BlankNode(b) => b.into(),
                    Term::Literal(_) | Term::Triple(_) => {
                        NamedNode::new_unchecked("http://ex.org/s0").into()
                    }
                };
                let quad = Quad::new(
                    subject,
                    NamedNode::new_unchecked(format!("http://ex.org/p{}", ctx.rng().below(3))),
                    self.term(ctx),
                    if ctx.rng().chance(200) {
                        GraphName::NamedNode(NamedNode::new_unchecked("http://ex.org/g"))
                    } else {
                        GraphName::DefaultGraph
                    },
                );
                let (t, q) = write.terms();
                let graph = t.graph(q, quad.graph_name.as_ref())?;
                let subject_id = t.node(q, quad.subject.as_ref())?;
                let predicate = t.predicate(q, quad.predicate.as_ref())?;
                let object = t.term(q, quad.object.as_ref())?;
                if ctx.rng().chance(250) {
                    let _deleted: Option<Vec<SystemId>> =
                        write.delete(graph, subject_id, predicate, &object, &kind(&quad.object))?;
                } else {
                    write.insert(
                        graph,
                        subject_id,
                        predicate,
                        &object,
                        &[],
                        &kind(&quad.object),
                    );
                }
            }
            let (request, t) = write.finish();
            batch.push(request);
            terms.push(t);
        }
        let _versions: Vec<u64> = quads.commit(batch, &terms)?;
        Ok(())
    }

    fn some_key(
        &self,
        quads: &SimQuads,
        ctx: &mut SimContext,
    ) -> Result<Option<Vec<u8>>, QuadError> {
        let snapshot = quads.store().snapshot(quads.lists().applied_ts())?;
        let keyspace = if ctx.rng().chance(500) {
            Keyspace::Forward
        } else {
            Keyspace::Reverse
        };
        let keys: Vec<Vec<u8>> = snapshot
            .scan(&Scan::prefix(key_prefix(keyspace, false, 0, None, None)))
            .filter_map(|i| i.ok().map(|i| i.key().to_vec()))
            .collect();
        Ok((!keys.is_empty()).then(|| keys[ctx.rng().index(keys.len())].clone()))
    }

    /// Reads every forward and reverse list and every term each names through the dictionary.
    fn check(&mut self, ctx: &mut SimContext) -> Result<Option<(&'static str, String)>, QuadError> {
        let quads = self.quads(ctx)?;
        let ts = quads.lists().applied_ts();
        let snapshot = quads.store().snapshot(ts)?;
        let mut keys: Vec<Vec<u8>> = Vec::new();
        for keyspace in [Keyspace::Forward, Keyspace::Reverse] {
            for item in snapshot.scan(&Scan::prefix(key_prefix(keyspace, false, 0, None, None))) {
                keys.push(item?.key().to_vec());
            }
        }
        for key in keys {
            let (parsed, _) = QuadKey::parse(&key)?;
            let mut ids: Vec<SystemId> = parsed.predicate().into_iter().collect();
            if let Some(Graph::Named(graph)) = parsed.graph() {
                ids.push(graph);
            }
            match &parsed {
                QuadKey::Forward { subject, .. } => ids.push(*subject),
                QuadKey::Reverse { object, .. } => {
                    if let Err(e) = quads.term_of(0, object) {
                        return dictionary_miss(e);
                    }
                }
                QuadKey::Catalog { .. }
                | QuadKey::SubjectPredicates { .. }
                | QuadKey::ObjectPredicates { .. }
                | QuadKey::ValueIndex { .. }
                | QuadKey::CountIndex { .. }
                | QuadKey::VectorIndex { .. }
                | QuadKey::Statement { .. }
                | QuadKey::Statistics { .. }
                | QuadKey::Mark { .. }
                | QuadKey::Referrers { .. }
                | QuadKey::Dictionary { .. } => {}
            }
            for id in ids {
                match quads.iri_of(0, id) {
                    Ok((term, _)) => {
                        if let Some(broken) = self.record(id, term) {
                            return Ok(Some(broken));
                        }
                    }
                    Err(e) => return dictionary_miss(e),
                }
            }
            match quads.list_terms(0, &key, ts) {
                Ok(_) => self.checked += 1,
                Err(e) => return dictionary_miss(e),
            }
        }
        Ok(None)
    }

    /// Records that `id` names `term`: the rule broken when either was seen with another.
    fn record(&mut self, id: SystemId, term: String) -> Option<(&'static str, String)> {
        if let Some(previous) = self.seen.get(&id)
            && *previous != term
        {
            return Some((
                IDS_NEVER_REISSUED,
                format!("{id:?} named {previous}, now {term}"),
            ));
        }
        if let Some(previous) = self.named.get(&term)
            && *previous != id
        {
            return Some((
                ONE_ID_PER_TERM,
                format!("{term} was {previous:?}, now {id:?}"),
            ));
        }
        let _seen: Option<String> = self.seen.insert(id, term.clone());
        let _named: Option<SystemId> = self.named.insert(term, id);
        None
    }
}

/// A missing entry is the rule broken; any other error is returned for the fault check.
fn dictionary_miss(e: QuadError) -> Result<Option<(&'static str, String)>, QuadError> {
    match e.0 {
        QuadErrorEnum::Corrupted { .. } => Ok(Some((DICTIONARY_COMPLETE, e.to_string()))),
        QuadErrorEnum::Store(_)
        | QuadErrorEnum::Posting(_)
        | QuadErrorEnum::Term(_)
        | QuadErrorEnum::Key(_)
        | QuadErrorEnum::Mapping(_)
        | QuadErrorEnum::Catalog(_)
        | QuadErrorEnum::Id(_)
        | QuadErrorEnum::NoSequence { .. }
        | QuadErrorEnum::PendingFull { .. }
        | QuadErrorEnum::NoOwnedBucket { .. }
        | QuadErrorEnum::SkolemDraws { .. }
        | QuadErrorEnum::NotOwned { .. }
        | QuadErrorEnum::UnknownSkolem { .. } => Err(e),
    }
}
