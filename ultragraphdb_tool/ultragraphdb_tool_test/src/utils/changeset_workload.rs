/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The changeset applier's workload on the storage simulator: a writer applies changesets to a single-valued indexed
//! property, a multi-valued property with two indexes and literals of several datatypes and languages, a counted edge,
//! and an edge whose statements take IDs and properties; an administrator rolls lists up and syncs. Faults fail
//! batches and reads, crash the process and cut its power.
//!
//! After every step and every recovery the indexes agree with the data: each value-index key holds the subjects whose
//! literals produce its token, each reverse list the subjects holding its object, each predicate list the predicates
//! its subject or object holds, each count key the nodes with its count, and each statement ID its record. Until the
//! first recovery, after which what a lost power kept is not the workload's to know, the statements held equal the
//! [`CanonicalDataset`]'s, the model every changeset is applied to as well.
//!
//! The model allocates its statement IDs from its own sequence, so statements are compared by their triples,
//! properties and whether they carry an ID; a statement deleted by its ID is named to each by its own ID for a
//! statement of the same triple and properties.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use oxrdf::vocab::xsd;
use std::num::NonZeroU64;

use oxrdf::{GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::{
    CanonicalDataset, Cardinality, Change, Changeset, Declaration, PredicateDeclaration,
    PredicateKind, RandomSource, StatementRef,
};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_core_tok::{get_tokenizer, get_tokenizer_for_lang};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_apply::{
    Applied, ChangesetApplier, ChangesetError, ChangesetErrorEnum, token_value,
};
use ultragraphdb_store_posting::{CachedList, ListsConfig, PostingError, PostingErrorEnum};
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum};
use ultragraphdb_store_sim::{
    Event, FaultConfig, RunOutcome, Seed, SimContext, SimListCache, SimStore, Simulation,
    SteppedExecutor, Stream, Violation,
};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_traits_store::{PersistMode, Scan, Store, StoreSnapshot};

/// The rule an index breaks when it disagrees with the statements it indexes.
pub const INDEXES_MATCH_DATA: &str = "every value-index, reverse, predicate-list and count key holds exactly what the statements produce";
/// The rule a statement ID breaks without its record, or a record without its statement.
pub const RECORDS_MATCH_POSTINGS: &str =
    "every statement ID a posting carries has its record, and every record its posting";
/// The rule the store breaks when it holds other statements than the model.
pub const STATISTICS_MATCH_DATA: &str = "the statistics of every graph and predicate count exactly the statements its forward lists hold";
/// The rule the store breaks when it holds other statements than the model.
pub const MODEL_AGREES: &str = "the store holds the statements the canonical model holds";
/// The rule broken by an error no fault explains.
pub const NO_UNEXPECTED_ERROR: &str =
    "a changeset fails only for a fault the simulator injected or a refusal the model makes too";

const NS: u64 = 0;
const SUBJECTS: u64 = 6;
const PHRASES: [&str; 5] = ["big apple", "red apple", "green pear", "big fig", "red"];

type SimChangesets =
    ChangesetApplier<SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// The workload's declarations: `name` a single-valued property with a `term` index, `tags` a property with `term`
/// and `exact` indexes, `friend` an edge with a count index; `knows` is left to its default.
fn declarations() -> Changeset {
    let name = PredicateDeclaration::new(ex("name"))
        .with_kind(PredicateKind::Property)
        .with_cardinality(Cardinality::Single)
        .with_tokenizers(vec!["term".into()]);
    let tags = PredicateDeclaration::new(ex("tags"))
        .with_kind(PredicateKind::Property)
        .with_tokenizers(vec!["term".into(), "exact".into()]);
    let friend = PredicateDeclaration::new(ex("friend"))
        .with_kind(PredicateKind::Edge)
        .with_count_index(true);
    [name, tags, friend]
        .into_iter()
        .fold(Changeset::new(NS), |c, d| {
            c.with(Change::Declare(Declaration::Predicate(d)))
        })
}

fn cardinality(predicate: &NamedNode) -> Cardinality {
    if *predicate == ex("name") {
        Cardinality::Single
    } else {
        Cardinality::Multi
    }
}

/// One statement, as both sides are compared: its quad, its properties sorted, and whether it carries an ID.
type Held = (String, Vec<String>, bool);

/// The changeset applier's workload on the simulator: random changesets and administration over a fault-injecting
/// simulated store, each step checked against the canonical model and the store's own invariants.
#[derive(Default)]
pub struct ChangesetWorkload {
    applier: Option<Arc<SimChangesets>>,
    /// The metadata group's stand-in, which outlives every crash: no sequence is leased twice.
    blocks: Option<TestBlockSource>,
    /// The appliers opened so far, which seeds the next one's random source afresh, as a partition seeds it at start.
    starts: u64,
    /// The model, until the first recovery.
    model: Option<CanonicalDataset>,
    /// A changeset the store and the model did not both apply, or both refuse.
    disagreement: Option<String>,
    /// How many changesets the store applied.
    pub writes: usize,
    /// How many changesets the store and the model both refused.
    pub refusals: usize,
    /// How many steps were checked.
    pub checked: usize,
    /// How many checks compared the store with the model.
    pub compared: usize,
}

impl ChangesetWorkload {
    /// The faults of a run: every class but corruption, which a posting list cannot always detect.
    pub fn faults() -> FaultConfig {
        FaultConfig::mixed().with_corruption(0)
    }

    /// Runs the workload for `steps` steps of `seed` under `faults`: the simulation's outcome and the workload's
    /// counts.
    ///
    /// # Errors
    ///
    /// The [`Violation`] of the first rule a step breaks.
    pub fn run(
        seed: Seed,
        faults: FaultConfig,
        steps: u64,
    ) -> Result<(RunOutcome, ChangesetWorkload), Violation> {
        let mut w = Self {
            model: Some(CanonicalDataset::new()),
            ..Self::default()
        };
        let outcome =
            Simulation::new(seed, faults).run(steps, 2, |ctx, event| w.step(ctx, event))?;
        Ok((outcome, w))
    }

    fn applier(&mut self, ctx: &SimContext) -> Result<Arc<SimChangesets>, ChangesetError> {
        if let Some(a) = &self.applier {
            return Ok(a.clone());
        }
        let cache = SimListCache::new(8, ctx.seed().stream(Stream::Cache));
        let lists = ultragraphdb_store_posting::Lists::new(
            ctx.store().clone(),
            cache,
            ListsConfig::new(PersistMode::Buffer, 256),
        );
        let blocks = self
            .blocks
            .get_or_insert_with(|| TestBlockSource::new(NonZeroU64::MIN.saturating_add(99)))
            .clone();
        self.starts += 1;
        let random = RandomSource::new(
            ctx.seed()
                .value()
                .wrapping_mul(1_000)
                .wrapping_add(self.starts),
        );
        let a = Arc::new(ChangesetApplier::new(
            lists,
            blocks,
            random,
            ctx.executor().clone(),
            ctx.seed().value(),
        )?);
        let declared = match a.quads().id_of(NS, ex("friend").as_str())? {
            Some(friend) => a.catalog(NS)?.predicate(friend).is_some(),
            None => false,
        };
        if !declared {
            let _declared: Applied = a.apply(&declarations())?;
        }
        self.applier = Some(a.clone());
        Ok(a)
    }

    /// Performs one step on `event`: a write or administration, then the checks.
    ///
    /// # Errors
    ///
    /// The [`Violation`] of the rule the step breaks.
    pub fn step(&mut self, ctx: &mut SimContext, event: Event) -> Result<(), Violation> {
        if event == Event::Recovered {
            self.applier = None;
            self.model = None;
        } else {
            let result = match event {
                Event::Step(0) => self.write(ctx),
                Event::Step(_) | Event::Recovered => self.administer(ctx),
            };
            self.tolerate(ctx, result)?;
            if let Some(detail) = self.disagreement.take() {
                return Err(ctx.violation(MODEL_AGREES, detail));
            }
        }
        if ctx.store().needs_recovery() {
            self.model = None;
            return Ok(());
        }
        // The check only reads, so a read an injected fault failed is retried.
        let mut result = self.check(ctx);
        for _ in 0..8 {
            match &result {
                Err(e) if injected(e) && !ctx.store().needs_recovery() => result = self.check(ctx),
                _ => break,
            }
        }
        match result {
            Ok(None) => {
                self.checked += 1;
                Ok(())
            }
            Ok(Some((rule, detail))) => Err(ctx.violation(rule, detail)),
            Err(e) => self.tolerate(ctx, Err(e)),
        }
    }

    /// An error is the workload's own when no fault explains it.
    fn tolerate(
        &mut self,
        ctx: &mut SimContext,
        result: Result<(), ChangesetError>,
    ) -> Result<(), Violation> {
        let Err(error) = result else {
            return Ok(());
        };
        if injected(&error) || ctx.store().needs_recovery() {
            Ok(())
        } else {
            Err(ctx.violation(NO_UNEXPECTED_ERROR, error.to_string()))
        }
    }

    fn subject(ctx: &mut SimContext) -> NamedOrBlankNode {
        ex(&format!("s{}", ctx.rng().below(SUBJECTS))).into()
    }

    fn literal(ctx: &mut SimContext) -> Term {
        let phrase = PHRASES[ctx.rng().index(PHRASES.len())];
        match ctx.rng().below(6) {
            0 => Literal::new_language_tagged_literal_unchecked(phrase, "en").into(),
            1 => Literal::new_language_tagged_literal_unchecked(phrase, "de").into(),
            2 => {
                Literal::new_typed_literal(format!("0{}", ctx.rng().below(3)), xsd::INTEGER).into()
            }
            _ => Literal::new_simple_literal(phrase).into(),
        }
    }

    /// One change for the store and the same change for the model: they differ only in the statement ID a delete of
    /// one statement names.
    fn change(
        &mut self,
        ctx: &mut SimContext,
        a: &SimChangesets,
    ) -> Result<Option<(Change, Change)>, ChangesetError> {
        let graph = GraphName::DefaultGraph;
        let subject = Self::subject(ctx);
        let same = |c: Change| Some((c.clone(), c));
        Ok(match ctx.rng().below(14) {
            0..2 => same(insert_triple(subject, "name", Self::literal(ctx))),
            2 => same(delete_triple(subject, "name", Self::literal(ctx))),
            3..5 => same(insert_triple(subject, "tags", Self::literal(ctx))),
            5 => same(delete_triple(subject, "tags", Self::literal(ctx))),
            6 => {
                let predicate = ex(["name", "tags", "friend", "knows"][ctx.rng().index(4)]);
                same(Change::DeletePattern {
                    graph,
                    subject,
                    predicate,
                })
            }
            7..9 => same(insert_triple(subject, "friend", Self::subject(ctx).into())),
            9 => same(delete_triple(subject, "friend", Self::subject(ctx).into())),
            10 | 11 => same(Self::knows_edge(ctx, subject)),
            12 => same(Change::SetProperty {
                statement: StatementRef::Triple {
                    graph,
                    subject,
                    predicate: ex("knows"),
                    object: Self::subject(ctx).into(),
                },
                predicate: ex("weight"),
                value: Literal::new_typed_literal(ctx.rng().below(3).to_string(), xsd::INTEGER)
                    .into(),
            }),
            _ => self.delete_statement(ctx, a)?,
        })
    }

    /// An insert of a `knows` edge from `subject`, with a `since` property or none.
    fn knows_edge(ctx: &mut SimContext, subject: NamedOrBlankNode) -> Change {
        let properties = match ctx.rng().below(3) {
            0 => Vec::new(),
            year => vec![(
                ex("since"),
                Literal::new_typed_literal(format!("20{year}0"), xsd::INTEGER).into(),
            )],
        };
        Change::InsertEdge {
            graph: GraphName::DefaultGraph,
            subject,
            predicate: ex("knows"),
            object: Self::subject(ctx).into(),
            properties,
        }
    }

    /// A delete of one statement with an ID, the store's and the model's of the same triple and properties.
    fn delete_statement(
        &mut self,
        ctx: &mut SimContext,
        a: &SimChangesets,
    ) -> Result<Option<(Change, Change)>, ChangesetError> {
        let Some(model) = &self.model else {
            return Ok(None);
        };
        let candidates: Vec<_> = model
            .statements()
            .filter(|s| s.reifier().is_some())
            .collect();
        if candidates.is_empty() {
            return Ok(None);
        }
        let chosen = &candidates[ctx.rng().index(candidates.len())];
        let wanted = held_of(
            chosen.graph(),
            chosen.subject(),
            chosen.predicate(),
            chosen.object(),
            chosen.properties(),
            true,
        );
        let stored = stored_statements(a)?;
        let Some((_, Some(id))) = stored.iter().find(|(h, id)| *h == wanted && id.is_some()) else {
            return Ok(None);
        };
        let Some(model_reifier) = chosen.reifier() else {
            return Ok(None);
        };
        let delete = |reifier: NamedNode| Change::DeleteStatement {
            graph: GraphName::DefaultGraph,
            reifier,
        };
        let stored_reifier = NamedNode::new_unchecked(a.quads().iri_of(NS, *id)?.0);
        Ok(Some((
            delete(stored_reifier),
            delete(model_reifier.clone()),
        )))
    }

    fn write(&mut self, ctx: &mut SimContext) -> Result<(), ChangesetError> {
        let a = self.applier(ctx)?;
        let (mut stored, mut modelled) = (Changeset::new(NS), Changeset::new(NS));
        for _ in 0..ctx.rng().between(1, 3) {
            if let Some((s, m)) = self.change(ctx, &a)? {
                stored.push(s);
                modelled.push(m);
            }
        }
        let result = a.apply(&stored);
        tracing::debug!(target: "workload", changes = ?stored.changes(), result = ?result, "changeset");
        let refused = match result {
            Ok(_) => {
                self.writes += 1;
                false
            }
            Err(e) if matches!(e.0, ChangesetErrorEnum::Mapping(_)) => {
                self.refusals += 1;
                true
            }
            Err(e) => return Err(e),
        };
        if let Some(model) = &mut self.model {
            let modelled_refused = model.apply(&modelled, cardinality).is_err();
            if modelled_refused != refused {
                self.disagreement = Some(format!(
                    "{stored:?}: the store refused it {refused}, the model {modelled_refused}"
                ));
            }
        }
        Ok(())
    }

    fn administer(&mut self, ctx: &mut SimContext) -> Result<(), ChangesetError> {
        let a = self.applier(ctx)?;
        if ctx.rng().chance(300) {
            return Ok(a.quads().lists().sync()?);
        }
        let keyspace = [
            Keyspace::Forward,
            Keyspace::Reverse,
            Keyspace::ValueIndex,
            Keyspace::CountIndex,
        ][ctx.rng().index(4)];
        let keys = live_keys(&a, keyspace)?;
        if keys.is_empty() {
            return Ok(());
        }
        let key = &keys[ctx.rng().index(keys.len())];
        match a.quads().lists().rollup_key(key) {
            Ok(_) | Err(PostingError(PostingErrorEnum::HighPriorityOp)) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn check(
        &mut self,
        ctx: &mut SimContext,
    ) -> Result<Option<(&'static str, String)>, ChangesetError> {
        let a = self.applier(ctx)?;
        let ts = a.quads().lists().applied_ts();
        a.quads().lists().gate(ts)?;
        let forward = lists_of(&a, Keyspace::Forward)?;
        if let Some(detail) = indexes_disagree(&a, &forward)? {
            return Ok(Some((INDEXES_MATCH_DATA, detail)));
        }
        if let Some(detail) = statistics_disagree(&a, &forward, ts)? {
            return Ok(Some((STATISTICS_MATCH_DATA, detail)));
        }
        let stored = statements_of(&a, &forward);
        let stored = match stored {
            Ok(s) => s,
            Err(e) if matches!(&e.0, ChangesetErrorEnum::Quad(q) if matches!(q.0, QuadErrorEnum::Corrupted { .. })) =>
            {
                return Ok(Some((RECORDS_MATCH_POSTINGS, e.to_string())));
            }
            Err(e) => return Err(e),
        };
        if let Some(detail) = orphan_record(&a, &stored)? {
            return Ok(Some((RECORDS_MATCH_POSTINGS, detail)));
        }
        if let Some(model) = &self.model {
            let mut held: Vec<Held> = stored.into_iter().map(|(h, _)| h).collect();
            let mut expected: Vec<Held> = model
                .statements()
                .map(|s| {
                    held_of(
                        s.graph(),
                        s.subject(),
                        s.predicate(),
                        s.object(),
                        s.properties(),
                        s.reifier().is_some(),
                    )
                })
                .collect();
            held.sort();
            expected.sort();
            if held != expected {
                return Ok(Some((
                    MODEL_AGREES,
                    format!("store {held:?}, model {expected:?}"),
                )));
            }
            self.compared += 1;
        }
        Ok(None)
    }
}

/// The first graph and predicate whose statistics count other than the statements its forward lists hold, every
/// predicate whose statistics key the store holds included.
fn statistics_disagree(
    a: &SimChangesets,
    forward: &Lists,
    ts: u64,
) -> Result<Option<String>, ChangesetError> {
    let mut held: BTreeMap<(Graph, SystemId), i64> = BTreeMap::new();
    for (key, objects) in forward {
        if let QuadKey::Forward {
            graph, predicate, ..
        } = key
        {
            // A list's length fits in `i64`; were it not to, the count saturates rather than wraps.
            *held.entry((*graph, *predicate)).or_default() +=
                i64::try_from(objects.len()).unwrap_or(i64::MAX);
        }
    }
    for key in live_keys(a, Keyspace::Statistics)? {
        if let Ok((
            QuadKey::Statistics {
                graph, predicate, ..
            },
            _,
        )) = QuadKey::parse(&key)
        {
            let _statements: &mut i64 = held.entry((graph, predicate)).or_default();
        }
    }
    for ((graph, predicate), statements) in held {
        let counted = a.quads().statistics(NS, graph, predicate, ts)?.statements();
        if counted != statements {
            return Ok(Some(format!(
                "graph {graph:?} predicate {predicate:?}: {statements} statements held, {counted} counted"
            )));
        }
    }
    Ok(None)
}

/// Whether `error` is one an injected fault explains.
fn injected(error: &ChangesetError) -> bool {
    let store = |e: &StoreError| {
        matches!(
            e.0,
            StoreErrorEnum::WriteFailed(_)
                | StoreErrorEnum::Poisoned
                | StoreErrorEnum::ReadFailed { .. }
                | StoreErrorEnum::TsTooOld
        )
    };
    let posting = |e: &PostingError| matches!(&e.0, PostingErrorEnum::Store(s) if store(s));
    let quad = |e: &QuadError| match &e.0 {
        QuadErrorEnum::Store(s) => store(s),
        QuadErrorEnum::Posting(p) => posting(p),
        QuadErrorEnum::Catalog(c) => {
            matches!(&c.0, ultragraphdb_core_catalog::CatalogErrorEnum::Store(s) if store(s))
        }
        QuadErrorEnum::Term(_)
        | QuadErrorEnum::Key(_)
        | QuadErrorEnum::Mapping(_)
        | QuadErrorEnum::Id(_)
        | QuadErrorEnum::Corrupted { .. }
        | QuadErrorEnum::NoSequence { .. }
        | QuadErrorEnum::PendingFull { .. }
        | QuadErrorEnum::NoOwnedBucket { .. }
        | QuadErrorEnum::SkolemDraws { .. }
        | QuadErrorEnum::UnknownSkolem { .. } => false,
    };
    match &error.0 {
        ChangesetErrorEnum::Store(s) => store(s),
        ChangesetErrorEnum::Posting(p) => posting(p),
        ChangesetErrorEnum::Quad(q) => quad(q),
        ChangesetErrorEnum::Catalog(c) => {
            matches!(&c.0, ultragraphdb_core_catalog::CatalogErrorEnum::Store(s) if store(s))
        }
        ChangesetErrorEnum::Mapping(_)
        | ChangesetErrorEnum::Tok(_)
        | ChangesetErrorEnum::Hnsw(_)
        | ChangesetErrorEnum::Invariant(_)
        | ChangesetErrorEnum::Literal { .. }
        | ChangesetErrorEnum::Unique { .. }
        | ChangesetErrorEnum::Contradicted { .. }
        | ChangesetErrorEnum::Rebuilding { .. }
        | ChangesetErrorEnum::VectorOption { .. }
        | ChangesetErrorEnum::Dimension { .. }
        | ChangesetErrorEnum::NoDimension { .. }
        | ChangesetErrorEnum::NoVectorIndex { .. }
        | ChangesetErrorEnum::OperationFailed { .. }
        | ChangesetErrorEnum::NotUnique { .. } => false,
    }
}

fn held_of(
    graph: &GraphName,
    subject: &NamedOrBlankNode,
    predicate: &NamedNode,
    object: &Term,
    properties: &[(NamedNode, Term)],
    with_id: bool,
) -> Held {
    let mut properties: Vec<String> = properties.iter().map(|(p, v)| format!("{p} {v}")).collect();
    properties.sort();
    (
        format!("{subject} {predicate} {object} {graph}"),
        properties,
        with_id,
    )
}

/// The keys of `keyspace` in the namespace's default graph live at the newest timestamp, split parts left out.
fn live_keys(a: &SimChangesets, keyspace: Keyspace) -> Result<Vec<Vec<u8>>, ChangesetError> {
    let snapshot = a.quads().store().snapshot(a.quads().lists().applied_ts())?;
    let keys = snapshot
        .scan(&Scan::prefix(key_prefix(
            keyspace,
            false,
            NS,
            Some(DEFAULT_GRAPH),
            None,
        )))
        .map(|i| i.map(|i| i.key().to_vec()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(keys)
}

/// Lists by key, each with its objects and their statement IDs.
type Lists = Vec<(QuadKey, Vec<(EncodedTerm, Vec<SystemId>)>)>;

/// The objects of every non-empty list of `keyspace`, with its key, in key order.
fn lists_of(a: &SimChangesets, keyspace: Keyspace) -> Result<Lists, ChangesetError> {
    let ts = a.quads().lists().applied_ts();
    let mut out = Vec::new();
    for key in live_keys(a, keyspace)? {
        let objects = a.quads().objects(&key, ts)?;
        if !objects.is_empty() {
            out.push((QuadKey::parse(&key).map_err(QuadError::from)?.0, objects));
        }
    }
    Ok(out)
}

/// Every statement the store holds, with its statement ID when it carries one.
fn stored_statements(a: &SimChangesets) -> Result<Vec<(Held, Option<SystemId>)>, ChangesetError> {
    statements_of(a, &lists_of(a, Keyspace::Forward)?)
}

/// The statements `forward`, the store's forward lists, hold.
fn statements_of(
    a: &SimChangesets,
    forward: &Lists,
) -> Result<Vec<(Held, Option<SystemId>)>, ChangesetError> {
    let ts = a.quads().lists().applied_ts();
    let q = a.quads();
    let mut out = Vec::new();
    for (key, objects) in forward.iter().cloned() {
        let QuadKey::Forward {
            graph,
            predicate,
            subject,
            ..
        } = key
        else {
            continue;
        };
        let graph_name = match graph {
            Graph::Default => GraphName::DefaultGraph,
            Graph::Named(g) => GraphName::NamedNode(NamedNode::new_unchecked(q.iri_of(NS, g)?.0)),
        };
        let subject_term: NamedOrBlankNode =
            NamedNode::new_unchecked(q.iri_of(NS, subject)?.0).into();
        let predicate_iri = NamedNode::new_unchecked(q.iri_of(NS, predicate)?.0);
        for (object, ids) in objects {
            let object_term = q.term_of(NS, &object)?;
            let held = |properties: &[(NamedNode, Term)], with_id: bool| {
                held_of(
                    &graph_name,
                    &subject_term,
                    &predicate_iri,
                    &object_term,
                    properties,
                    with_id,
                )
            };
            if ids.is_empty() {
                out.push((held(&[], false), None));
            }
            for id in ids {
                let record = q.statement(NS, graph, id, ts)?.ok_or_else(|| {
                    QuadError::corrupted(&key.to_bytes(), format!("statement {id:?} has no record"))
                })?;
                if (record.subject(), record.predicate(), record.object())
                    != (subject, predicate, &object)
                {
                    return Err(QuadError::corrupted(
                        &key.to_bytes(),
                        format!("record {id:?} names another statement"),
                    )
                    .into());
                }
                let properties = record
                    .properties()
                    .iter()
                    .map(|(p, v)| {
                        Ok((
                            NamedNode::new_unchecked(q.iri_of(NS, *p)?.0),
                            q.term_of(NS, v)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, QuadError>>()?;
                out.push((held(&properties, true), Some(id)));
            }
        }
    }
    Ok(out)
}

/// A statement record no posting carries the ID of.
fn orphan_record(
    a: &SimChangesets,
    stored: &[(Held, Option<SystemId>)],
) -> Result<Option<String>, ChangesetError> {
    let ids: BTreeSet<SystemId> = stored.iter().filter_map(|(_, id)| *id).collect();
    for key in live_keys(a, Keyspace::Statements)? {
        if let (QuadKey::Statement { statement, .. }, _) =
            QuadKey::parse(&key).map_err(QuadError::from)?
            && !ids.contains(&statement)
            && a.quads()
                .statement(NS, DEFAULT_GRAPH, statement, a.quads().lists().applied_ts())?
                .is_some()
        {
            return Ok(Some(format!("record {statement:?} has no posting")));
        }
    }
    Ok(None)
}

/// The indexes the statements of the default graph produce, by the list each names.
#[derive(Default)]
struct DataIndexes {
    /// Each declared tokenizer's token of each literal, to its subjects.
    tokens: BTreeMap<(SystemId, u8, Vec<u8>), BTreeSet<SystemId>>,
    reverse: BTreeMap<(SystemId, EncodedTerm), BTreeSet<SystemId>>,
    subject_predicates: BTreeMap<SystemId, BTreeSet<SystemId>>,
    object_predicates: BTreeMap<EncodedTerm, BTreeSet<SystemId>>,
    forward_counts: BTreeMap<(SystemId, u32), BTreeSet<SystemId>>,
}

/// The indexes `forward`, the store's forward lists, produce in the default graph.
fn data_indexes(a: &SimChangesets, forward: &Lists) -> Result<DataIndexes, ChangesetError> {
    let q = a.quads();
    let catalog = a.catalog(NS)?;
    let mut data = DataIndexes::default();
    for (key, objects) in forward {
        let QuadKey::Forward {
            predicate,
            subject,
            graph,
            ..
        } = key
        else {
            continue;
        };
        if *graph != DEFAULT_GRAPH {
            continue;
        }
        let declaration = catalog.predicate(*predicate);
        let _new_predicate: bool = data
            .subject_predicates
            .entry(*subject)
            .or_default()
            .insert(*predicate);
        if declaration.is_some_and(PredicateDeclaration::count_index) {
            let count = u32::try_from(objects.len()).unwrap_or(u32::MAX);
            let _new_counted: bool = data
                .forward_counts
                .entry((*predicate, count))
                .or_default()
                .insert(*subject);
        }
        for (object, _) in objects {
            let _new_subject: bool = data
                .reverse
                .entry((*predicate, object.clone()))
                .or_default()
                .insert(*subject);
            let _new_object_predicate: bool = data
                .object_predicates
                .entry(object.clone())
                .or_default()
                .insert(*predicate);
            let (Some(declaration), Term::Literal(literal)) = (declaration, q.term_of(NS, object)?)
            else {
                continue;
            };
            for name in declaration.tokenizers() {
                let (Some(t), Some(value)) =
                    (get_tokenizer(name), token_value(literal.as_ref(), name))
                else {
                    continue;
                };
                let t = get_tokenizer_for_lang(&t, literal.language().unwrap_or_default());
                for token in t.tokens(&value)? {
                    let _new_token_subject: bool = data
                        .tokens
                        .entry((*predicate, t.identifier(), token))
                        .or_default()
                        .insert(*subject);
                }
            }
        }
    }
    Ok(data)
}

/// The IDs among `objects`.
fn object_uids(objects: &[(EncodedTerm, Vec<SystemId>)]) -> BTreeSet<SystemId> {
    objects.iter().filter_map(|(o, _)| o.as_id()).collect()
}

/// What an index holds that the statements do not produce, or lacks.
fn indexes_disagree(a: &SimChangesets, forward: &Lists) -> Result<Option<String>, ChangesetError> {
    let data = data_indexes(a, forward)?;
    let actual_tokens: BTreeMap<(SystemId, u8, Vec<u8>), BTreeSet<SystemId>> =
        lists_of(a, Keyspace::ValueIndex)?
            .into_iter()
            .filter_map(|(k, o)| {
                if let QuadKey::ValueIndex {
                    predicate,
                    tokenizer,
                    token,
                    ..
                } = k
                {
                    Some(((predicate, tokenizer, token), object_uids(&o)))
                } else {
                    None
                }
            })
            .collect();
    if actual_tokens != data.tokens {
        return Ok(Some(format!(
            "value index {actual_tokens:?}, data {:?}",
            data.tokens
        )));
    }
    let actual_reverse: BTreeMap<(SystemId, EncodedTerm), BTreeSet<SystemId>> =
        lists_of(a, Keyspace::Reverse)?
            .into_iter()
            .filter_map(|(k, o)| {
                if let QuadKey::Reverse {
                    predicate, object, ..
                } = k
                {
                    Some(((predicate, object), object_uids(&o)))
                } else {
                    None
                }
            })
            .collect();
    if actual_reverse != data.reverse {
        return Ok(Some(format!(
            "reverse {actual_reverse:?}, data {:?}",
            data.reverse
        )));
    }
    if let Some(detail) = predicate_lists_disagree(a, &data)? {
        return Ok(Some(detail));
    }
    counts_disagree(a, &data)
}

/// What the subject and object predicate lists hold that `data` does not, or lack.
fn predicate_lists_disagree(
    a: &SimChangesets,
    data: &DataIndexes,
) -> Result<Option<String>, ChangesetError> {
    let actual_subject_predicates: BTreeMap<SystemId, BTreeSet<SystemId>> =
        lists_of(a, Keyspace::SubjectPredicates)?
            .into_iter()
            .filter_map(|(k, o)| {
                if let QuadKey::SubjectPredicates { subject, .. } = k {
                    Some((subject, object_uids(&o)))
                } else {
                    None
                }
            })
            .collect();
    if actual_subject_predicates != data.subject_predicates {
        return Ok(Some(format!(
            "subject predicates {actual_subject_predicates:?}, data {:?}",
            data.subject_predicates
        )));
    }
    let actual_object_predicates: BTreeMap<EncodedTerm, BTreeSet<SystemId>> =
        lists_of(a, Keyspace::ObjectPredicates)?
            .into_iter()
            .filter_map(|(k, o)| {
                if let QuadKey::ObjectPredicates { object, .. } = k {
                    Some((object, object_uids(&o)))
                } else {
                    None
                }
            })
            .collect();
    if actual_object_predicates != data.object_predicates {
        return Ok(Some(format!(
            "object predicates {actual_object_predicates:?}, data {:?}",
            data.object_predicates
        )));
    }
    Ok(None)
}

/// What the count index holds that `data` does not produce, or lacks: forward counts of the predicates declaring a
/// count index, and reverse counts of `friend`.
fn counts_disagree(
    a: &SimChangesets,
    data: &DataIndexes,
) -> Result<Option<String>, ChangesetError> {
    let mut reverse_counts: BTreeMap<(SystemId, u32), BTreeSet<EncodedTerm>> = BTreeMap::new();
    if let Some(friend) = a.quads().id_of(NS, ex("friend").as_str())? {
        for ((predicate, object), subjects) in &data.reverse {
            if *predicate == friend {
                let count = u32::try_from(subjects.len()).unwrap_or(u32::MAX);
                let _new_object: bool = reverse_counts
                    .entry((friend, count))
                    .or_default()
                    .insert(object.clone());
            }
        }
    }
    let mut actual_forward_counts: BTreeMap<(SystemId, u32), BTreeSet<SystemId>> = BTreeMap::new();
    let mut actual_reverse_counts: BTreeMap<(SystemId, u32), BTreeSet<EncodedTerm>> =
        BTreeMap::new();
    for (k, o) in lists_of(a, Keyspace::CountIndex)? {
        if let QuadKey::CountIndex {
            predicate,
            direction,
            count,
            ..
        } = k
        {
            match direction {
                CountDirection::Forward => {
                    let _previous: Option<BTreeSet<SystemId>> =
                        actual_forward_counts.insert((predicate, count), object_uids(&o));
                }
                CountDirection::Reverse => {
                    let _previous: Option<BTreeSet<EncodedTerm>> = actual_reverse_counts
                        .insert((predicate, count), o.into_iter().map(|(t, _)| t).collect());
                }
            }
        }
    }
    let forward_counts = &data.forward_counts;
    if actual_forward_counts != *forward_counts || actual_reverse_counts != reverse_counts {
        return Ok(Some(format!(
            "counts {actual_forward_counts:?} {actual_reverse_counts:?}, data {forward_counts:?} {reverse_counts:?}"
        )));
    }
    Ok(None)
}

/// An insert of the triple of `subject`, `predicate` and `object` in the default graph.
fn insert_triple(subject: NamedOrBlankNode, predicate: &str, object: Term) -> Change {
    Change::Insert {
        graph: GraphName::DefaultGraph,
        subject,
        predicate: ex(predicate),
        object,
    }
}

/// A delete of the triple of `subject`, `predicate` and `object` in the default graph.
fn delete_triple(subject: NamedOrBlankNode, predicate: &str, object: Term) -> Change {
    Change::Delete {
        graph: GraphName::DefaultGraph,
        subject,
        predicate: ex(predicate),
        object,
    }
}
