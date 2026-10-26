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
use oxrdf::{GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::{
    CanonicalDataset, Cardinality, Change, Changeset, Declaration, PredicateDeclaration,
    PredicateKind, StatementId, StatementRef,
};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_core_tok::{get_tokenizer, get_tokenizer_for_lang};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_apply::{ChangesetApplier, ChangesetError, ChangesetErrorEnum, token_value};
use ultragraphdb_store_posting::{CachedList, ListsConfig, PostingError, PostingErrorEnum};
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum};
use ultragraphdb_store_sim::{
    Event, FaultConfig, RunOutcome, Seed, SimContext, SimListCache, SimStore, Simulation, Stream,
    Violation,
};
use ultragraphdb_traits_store::{PersistMode, Scan, Store, StoreSnapshot};

/// The rule an index breaks when it disagrees with the statements it indexes.
pub const INDEXES_MATCH_DATA: &str = "every value-index, reverse, predicate-list and count key holds exactly what the statements produce";
/// The rule a statement ID breaks without its record, or a record without its statement.
pub const RECORDS_MATCH_POSTINGS: &str =
    "every statement ID a posting carries has its record, and every record its posting";
/// The rule the store breaks when it holds other statements than the model.
pub const MODEL_AGREES: &str = "the store holds the statements the canonical model holds";
/// The rule broken by an error no fault explains.
pub const NO_UNEXPECTED_ERROR: &str =
    "a changeset fails only for a fault the simulator injected or a refusal the model makes too";

const NS: u64 = 0;
const SUBJECTS: u64 = 6;
const PHRASES: [&str; 5] = ["big apple", "red apple", "green pear", "big fig", "red"];

type SimChangesets = ChangesetApplier<SimStore, SimListCache<CachedList>>;

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

#[derive(Default)]
pub struct ChangesetWorkload {
    applier: Option<Arc<SimChangesets>>,
    /// The model, until the first recovery.
    model: Option<CanonicalDataset>,
    /// A changeset the store and the model did not both apply, or both refuse.
    disagreement: Option<String>,
    pub writes: usize,
    pub refusals: usize,
    pub checked: usize,
    pub compared: usize,
}

impl ChangesetWorkload {
    /// The faults of a run: every class but corruption, which a posting list cannot always detect.
    pub fn faults() -> FaultConfig {
        FaultConfig::mixed().with_corruption(0)
    }

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
        let a = Arc::new(ChangesetApplier::new(lists)?);
        let declared = match a.quads().uid_of(NS, ex("friend").as_str())? {
            Some(friend) => a.catalog(NS)?.predicate(friend).is_some(),
            None => false,
        };
        if !declared {
            a.apply(&declarations())?;
        }
        self.applier = Some(a.clone());
        Ok(a)
    }

    pub fn step(&mut self, ctx: &mut SimContext, event: Event) -> Result<(), Violation> {
        if event == Event::Recovered {
            self.applier = None;
            self.model = None;
        } else {
            let result = match event {
                Event::Step(0) => self.write(ctx),
                _ => self.administer(ctx),
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
            0..2 => same(Change::Insert {
                graph,
                subject,
                predicate: ex("name"),
                object: Self::literal(ctx),
            }),
            2 => same(Change::Delete {
                graph,
                subject,
                predicate: ex("name"),
                object: Self::literal(ctx),
            }),
            3..5 => same(Change::Insert {
                graph,
                subject,
                predicate: ex("tags"),
                object: Self::literal(ctx),
            }),
            5 => same(Change::Delete {
                graph,
                subject,
                predicate: ex("tags"),
                object: Self::literal(ctx),
            }),
            6 => {
                let predicate = ex(["name", "tags", "friend", "knows"][ctx.rng().index(4)]);
                same(Change::DeletePattern {
                    graph,
                    subject,
                    predicate,
                })
            }
            7..9 => same(Change::Insert {
                graph,
                subject,
                predicate: ex("friend"),
                object: Self::subject(ctx).into(),
            }),
            9 => same(Change::Delete {
                graph,
                subject,
                predicate: ex("friend"),
                object: Self::subject(ctx).into(),
            }),
            10 | 11 => {
                let properties = match ctx.rng().below(3) {
                    0 => Vec::new(),
                    year => vec![(
                        ex("since"),
                        Literal::new_typed_literal(format!("20{year}0"), xsd::INTEGER).into(),
                    )],
                };
                same(Change::InsertEdge {
                    graph,
                    subject,
                    predicate: ex("knows"),
                    object: Self::subject(ctx).into(),
                    properties,
                })
            }
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

    /// A delete of one statement with an ID, the store's and the model's of the same triple and properties.
    fn delete_statement(
        &mut self,
        ctx: &mut SimContext,
        a: &SimChangesets,
    ) -> Result<Option<(Change, Change)>, ChangesetError> {
        let Some(model) = &self.model else {
            return Ok(None);
        };
        let candidates: Vec<_> = model.statements().filter(|s| s.id().is_some()).collect();
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
        let delete = |id: u64| Change::DeleteStatement {
            graph: GraphName::DefaultGraph,
            id: StatementId::new(id),
        };
        let model_id = chosen.id().map(|i| i.value()).unwrap_or_default();
        Ok(Some((delete(*id), delete(model_id))))
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
        let ts = a.quads().lists().latest_ts();
        a.quads().lists().gate(ts)?;
        let forward = lists_of(&a, Keyspace::Forward)?;
        if let Some(detail) = indexes_disagree(&a, &forward)? {
            return Ok(Some((INDEXES_MATCH_DATA, detail)));
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
                        s.id().is_some(),
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
        _ => false,
    };
    match &error.0 {
        ChangesetErrorEnum::Store(s) => store(s),
        ChangesetErrorEnum::Posting(p) => posting(p),
        ChangesetErrorEnum::Quad(q) => quad(q),
        ChangesetErrorEnum::Catalog(c) => {
            matches!(&c.0, ultragraphdb_core_catalog::CatalogErrorEnum::Store(s) if store(s))
        }
        _ => false,
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
    let snapshot = a.quads().store().snapshot(a.quads().lists().latest_ts())?;
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
type Lists = Vec<(QuadKey, Vec<(EncodedTerm, Vec<u64>)>)>;

/// The objects of every non-empty list of `keyspace`, with its key, in key order.
fn lists_of(a: &SimChangesets, keyspace: Keyspace) -> Result<Lists, ChangesetError> {
    let ts = a.quads().lists().latest_ts();
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
fn stored_statements(a: &SimChangesets) -> Result<Vec<(Held, Option<u64>)>, ChangesetError> {
    statements_of(a, &lists_of(a, Keyspace::Forward)?)
}

/// The statements `forward`, the store's forward lists, hold.
fn statements_of(
    a: &SimChangesets,
    forward: &Lists,
) -> Result<Vec<(Held, Option<u64>)>, ChangesetError> {
    let ts = a.quads().lists().latest_ts();
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
            DEFAULT_GRAPH => GraphName::DefaultGraph,
            g => GraphName::NamedNode(NamedNode::new_unchecked(q.iri_of(NS, g)?.0)),
        };
        let subject_term: NamedOrBlankNode =
            NamedNode::new_unchecked(q.iri_of(NS, subject)?.0).into();
        let predicate_iri = NamedNode::new_unchecked(q.iri_of(NS, predicate)?.0);
        for (object, ids) in objects {
            let object_term = q.term_of(NS, &object)?;
            if ids.is_empty() {
                out.push((
                    held_of(
                        &graph_name,
                        &subject_term,
                        &predicate_iri,
                        &object_term,
                        &[],
                        false,
                    ),
                    None,
                ));
            }
            for id in ids {
                let record = q.statement(NS, graph, id, ts)?.ok_or_else(|| {
                    QuadError::Corrupted(&key.to_bytes(), format!("statement {id} has no record"))
                })?;
                if (record.subject(), record.predicate(), record.object())
                    != (subject, predicate, &object)
                {
                    return Err(QuadError::Corrupted(
                        &key.to_bytes(),
                        format!("record {id} names another statement"),
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
                let held = held_of(
                    &graph_name,
                    &subject_term,
                    &predicate_iri,
                    &object_term,
                    &properties,
                    true,
                );
                out.push((held, Some(id)));
            }
        }
    }
    Ok(out)
}

/// A statement record no posting carries the ID of.
fn orphan_record(
    a: &SimChangesets,
    stored: &[(Held, Option<u64>)],
) -> Result<Option<String>, ChangesetError> {
    let ids: BTreeSet<u64> = stored.iter().filter_map(|(_, id)| *id).collect();
    for key in live_keys(a, Keyspace::Statements)? {
        if let (QuadKey::Statement { statement, .. }, _) =
            QuadKey::parse(&key).map_err(QuadError::from)?
            && !ids.contains(&statement)
            && a.quads()
                .statement(NS, DEFAULT_GRAPH, statement, a.quads().lists().latest_ts())?
                .is_some()
        {
            return Ok(Some(format!("record {statement} has no posting")));
        }
    }
    Ok(None)
}

/// What an index holds that the statements do not produce, or lacks.
fn indexes_disagree(a: &SimChangesets, forward: &Lists) -> Result<Option<String>, ChangesetError> {
    let q = a.quads();
    let uid = |name: &str| q.uid_of(NS, ex(name).as_str());
    let catalog = a.catalog(NS)?;
    // Value indexes: each declared tokenizer's token of each literal, to its subjects.
    let mut tokens: BTreeMap<(u64, u8, Vec<u8>), BTreeSet<u64>> = BTreeMap::new();
    let mut reverse: BTreeMap<(u64, EncodedTerm), BTreeSet<u64>> = BTreeMap::new();
    let mut subject_predicates: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    let mut object_predicates: BTreeMap<EncodedTerm, BTreeSet<u64>> = BTreeMap::new();
    let mut forward_counts: BTreeMap<(u64, u32), BTreeSet<u64>> = BTreeMap::new();
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
        subject_predicates
            .entry(*subject)
            .or_default()
            .insert(*predicate);
        if declaration.is_some_and(PredicateDeclaration::count_index) {
            let count = u32::try_from(objects.len()).unwrap_or(u32::MAX);
            forward_counts
                .entry((*predicate, count))
                .or_default()
                .insert(*subject);
        }
        for (object, _) in objects {
            reverse
                .entry((*predicate, object.clone()))
                .or_default()
                .insert(*subject);
            object_predicates
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
                    tokens
                        .entry((*predicate, t.identifier(), token))
                        .or_default()
                        .insert(*subject);
                }
            }
        }
    }
    let uids = |objects: &[(EncodedTerm, Vec<u64>)]| -> BTreeSet<u64> {
        objects.iter().filter_map(|(o, _)| o.as_uid()).collect()
    };
    let actual_tokens: BTreeMap<(u64, u8, Vec<u8>), BTreeSet<u64>> =
        lists_of(a, Keyspace::ValueIndex)?
            .into_iter()
            .filter_map(|(k, o)| match k {
                QuadKey::ValueIndex {
                    predicate,
                    tokenizer,
                    token,
                    ..
                } => Some(((predicate, tokenizer, token), uids(&o))),
                _ => None,
            })
            .collect();
    if actual_tokens != tokens {
        return Ok(Some(format!(
            "value index {actual_tokens:?}, data {tokens:?}"
        )));
    }
    let actual_reverse: BTreeMap<(u64, EncodedTerm), BTreeSet<u64>> =
        lists_of(a, Keyspace::Reverse)?
            .into_iter()
            .filter_map(|(k, o)| match k {
                QuadKey::Reverse {
                    predicate, object, ..
                } => Some(((predicate, object), uids(&o))),
                _ => None,
            })
            .collect();
    if actual_reverse != reverse {
        return Ok(Some(format!(
            "reverse {actual_reverse:?}, data {reverse:?}"
        )));
    }
    let actual_subject_predicates: BTreeMap<u64, BTreeSet<u64>> =
        lists_of(a, Keyspace::SubjectPredicates)?
            .into_iter()
            .filter_map(|(k, o)| match k {
                QuadKey::SubjectPredicates { subject, .. } => Some((subject, uids(&o))),
                _ => None,
            })
            .collect();
    if actual_subject_predicates != subject_predicates {
        return Ok(Some(format!(
            "subject predicates {actual_subject_predicates:?}, data {subject_predicates:?}"
        )));
    }
    let actual_object_predicates: BTreeMap<EncodedTerm, BTreeSet<u64>> =
        lists_of(a, Keyspace::ObjectPredicates)?
            .into_iter()
            .filter_map(|(k, o)| match k {
                QuadKey::ObjectPredicates { object, .. } => Some((object, uids(&o))),
                _ => None,
            })
            .collect();
    if actual_object_predicates != object_predicates {
        return Ok(Some(format!(
            "object predicates {actual_object_predicates:?}, data {object_predicates:?}"
        )));
    }
    let mut reverse_counts: BTreeMap<(u64, u32), BTreeSet<EncodedTerm>> = BTreeMap::new();
    if let Some(friend) = uid("friend")? {
        for ((predicate, object), subjects) in &reverse {
            if *predicate == friend {
                let count = u32::try_from(subjects.len()).unwrap_or(u32::MAX);
                reverse_counts
                    .entry((friend, count))
                    .or_default()
                    .insert(object.clone());
            }
        }
    }
    let mut actual_forward_counts: BTreeMap<(u64, u32), BTreeSet<u64>> = BTreeMap::new();
    let mut actual_reverse_counts: BTreeMap<(u64, u32), BTreeSet<EncodedTerm>> = BTreeMap::new();
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
                    actual_forward_counts.insert((predicate, count), uids(&o));
                }
                CountDirection::Reverse => {
                    actual_reverse_counts
                        .insert((predicate, count), o.into_iter().map(|(t, _)| t).collect());
                }
            }
        }
    }
    if actual_forward_counts != forward_counts || actual_reverse_counts != reverse_counts {
        return Ok(Some(format!(
            "counts {actual_forward_counts:?} {actual_reverse_counts:?}, data {forward_counts:?} {reverse_counts:?}"
        )));
    }
    Ok(None)
}
