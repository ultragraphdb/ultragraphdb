/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire-protocol scenarios of bodies, prepared statements, describe, result batches and the conformance suite's
//! truncations.

use std::panic::{AssertUnwindSafe, catch_unwind};

use cucumber::{then, when};
use oxrdf::NamedNode;
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration, PredicateKind};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, Command, Describe, DescribeView, ErrorCode, Execute,
    ExecuteView, FrameView, Header, PatternSlot, Prepare, PrepareView, QuadPattern, ResultBatch,
    ResultTerm, ResultView, StatementCache, StatementError,
};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::{DataDictionary, QuadStore};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};
use ultragraphdb_traits_store::{PersistMode, Store, WriteBatch};

use crate::Steps;
use crate::utils::golden_bodies::{BODIES, CLUSTER, body_data, golden, reframe, view_any};

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// Quads over a simulated store, opened fresh, and namespace 0's declarations written into its store at their
/// versions; the data dictionary's answer for the namespace at a version is what a server builds the Describe answer
/// from.
struct Dictionary {
    store: SimStore,
    quads: QuadStore<SimStore, SimListCache<CachedList>>,
}

impl Dictionary {
    fn new() -> Self {
        let store = SimStore::new(Seed::new(9), FaultConfig::none(), SimClock::new());
        let cache = SimListCache::new(64, Seed::new(9).stream(Stream::Cache));
        let quads = QuadStore::open(Lists::new(store.clone(), cache, ListsConfig::default()))
            .expect("the quads open");
        Self { store, quads }
    }

    fn declare(&self, uid: u64, declaration: PredicateDeclaration, version: u64) {
        let mut batch = WriteBatch::new();
        batch.push(Catalog::entry(
            0,
            uid,
            &Declaration::Predicate(declaration),
            version,
        ));
        self.store
            .apply(batch, PersistMode::Sync)
            .expect("the declaration is written");
    }

    fn describe(&self, read_ts: u64) -> Describe {
        let d = DataDictionary::new(0)
            .describe(&self.quads, read_ts)
            .expect("the dictionary answers");
        Describe::new(0, read_ts)
            .with_labels(d.labels().iter().cloned().collect())
            .with_relationship_types(d.relationship_types().iter().cloned().collect())
            .with_property_keys(d.property_keys().iter().cloned().collect())
            .with_predicates(d.predicates().to_vec())
            .with_entity_types(d.entity_types().to_vec())
            .with_relations(d.relations().to_vec())
    }
}

fn name() -> PredicateDeclaration {
    PredicateDeclaration::new(ex("name"))
        .with_kind(PredicateKind::Property)
        .with_tokenizers(vec!["term".to_string()])
}

// Bodies are read in place.

#[when("a body's section offset points past its end")]
fn offset_past_the_end(w: &mut Steps) {
    // The golden Read: section 2, the pattern's term text, moved past the body's end.
    let bytes = golden("read").bytes();
    let mut data = body_data(&bytes).to_vec();
    let past = (data.len() + 16) as u32;
    data[16..20].copy_from_slice(&past.to_le_bytes());
    w.refusals = view_any(&reframe(&bytes, &data))
        .err()
        .into_iter()
        .collect();
    w.bytes = data;
}

#[then("the frame is refused with an error naming the section")]
fn section_named(w: &mut Steps) {
    let len = u32::from_le_bytes(w.bytes[20..24].try_into().unwrap());
    assert_eq!(
        w.refusals.iter().map(|e| e.0.clone()).collect::<Vec<_>>(),
        vec![ProtocolErrorEnum::SectionOutOfBounds {
            command: 7,
            section: 2,
            offset: (w.bytes.len() + 16) as u32,
            len,
            body: w.bytes.len(),
        }]
    );
    assert!(
        w.refusals[0].to_string().contains("section 2"),
        "{}",
        w.refusals[0]
    );
}

// Data requests, prepared statements and describe.

fn prepare_view_frame() -> Vec<u8> {
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Parameter(0),
        PatternSlot::Bound("<http://ex.org/name>".to_string()),
        PatternSlot::Unbound,
    );
    Prepare::new(0, 0, 0, pattern, vec![ColumnType::Node], Vec::new())
        .frame(Header::new(CLUSTER, Command::Prepare).with_stream(3))
        .unwrap()
}

fn execute_frame(statement: u64) -> Vec<u8> {
    Execute::new(
        statement,
        0,
        vec!["<http://ex.org/a>".to_string()],
        Vec::new(),
    )
    .frame(Header::new(CLUSTER, Command::Execute).with_stream(5))
    .unwrap()
}

#[when(
    "a statement is prepared, the catalog of its namespace changes, and the statement is executed"
)]
fn prepared_then_changed(w: &mut Steps) {
    let dictionary = Dictionary::new();
    dictionary.declare(7, name(), 10);
    let before = dictionary.describe(10).schema_id();
    let mut cache = StatementCache::new(16);
    let request = prepare_view_frame();
    let prepared = cache
        .prepare(
            &PrepareView::from_frame(&FrameView::validate(&request, CLUSTER).unwrap()).unwrap(),
            before,
        )
        .expect("the statement prepares");
    // The catalog changes: `age` is declared.
    dictionary.declare(
        8,
        PredicateDeclaration::new(ex("age")).with_kind(PredicateKind::Property),
        20,
    );
    let after = dictionary.describe(20).schema_id();
    assert_ne!(before, after, "a declaration changes the schema id");
    let execute = execute_frame(prepared.statement());
    let view = ExecuteView::from_frame(&FrameView::validate(&execute, CLUSTER).unwrap()).unwrap();
    w.statement = Some(cache.execute(&view, after).map(|_| ()));
    let again = cache
        .prepare(
            &PrepareView::from_frame(&FrameView::validate(&request, CLUSTER).unwrap()).unwrap(),
            after,
        )
        .expect("preparing again succeeds");
    let execute = execute_frame(again.statement());
    let view = ExecuteView::from_frame(&FrameView::validate(&execute, CLUSTER).unwrap()).unwrap();
    w.statement_again = Some(cache.execute(&view, after).map(|_| ()));
    w.schemas = vec![before, after];
    w.statements = vec![prepared.statement(), again.statement()];
}

#[then("the execution is refused as stale, and preparing it again succeeds")]
fn refused_stale(w: &mut Steps) {
    let refusal = w.statement.clone().unwrap().unwrap_err();
    assert_eq!(
        refusal,
        StatementError::Stale(w.statements[0], w.schemas[0], w.schemas[1])
    );
    assert_eq!(refusal.code(), ErrorCode::StaleStatement);
    assert_eq!(refusal.reply().code(), ErrorCode::StaleStatement);
    assert_ne!(w.statements[0], w.statements[1]);
    assert_eq!(w.statement_again, Some(Ok(())));
}

#[when(
    "`name` is declared a property with a `term` index and a client sends Describe for the namespace"
)]
fn describe_name(w: &mut Steps) {
    let dictionary = Dictionary::new();
    dictionary.declare(7, name(), 10);
    let request = Describe::request(0)
        .frame(Header::new(CLUSTER, Command::Describe).with_stream(9))
        .unwrap();
    let asked = DescribeView::from_frame(&FrameView::validate(&request, CLUSTER).unwrap()).unwrap();
    // The server answers the namespace asked, from the dictionary.
    let answer = dictionary.describe(10);
    assert_eq!(asked.namespace(), answer.namespace());
    w.bytes = answer
        .frame(Header::new(CLUSTER, Command::Describe).with_stream(9))
        .unwrap();
}

#[then(
    "the answer lists `name` as a property with its `term` index, as the data dictionary reports it"
)]
fn name_listed(w: &mut Steps) {
    let frame = FrameView::validate(&w.bytes, CLUSTER).unwrap();
    let view = DescribeView::from_frame(&frame).unwrap();
    assert_eq!(
        view.property_keys().collect::<Vec<_>>(),
        vec!["http://ex.org/name"]
    );
    let answer = view.to_body().unwrap();
    assert_eq!(answer.predicates().len(), 1);
    let declared = &answer.predicates()[0];
    assert_eq!(declared.predicate(), &ex("name"));
    assert_eq!(declared.kind(), PredicateKind::Property);
    assert_eq!(declared.tokenizers(), &["term".to_string()]);
    assert_eq!(answer.read_version(), 10);
}

// Result batches are columnar and dictionary-encoded.

#[when("a batch of 1,000 rows names the same IRI in every row")]
fn thousand_rows(w: &mut Steps) {
    let iri = ResultTerm::new(EncodedTerm::uid(42)).with_text("http://ex.org/same");
    let mut batch = ResultBatch::new(2);
    for row in 0..1_000u64 {
        let other = ResultTerm::new(EncodedTerm::uid(1_000 + row))
            .with_text(format!("http://ex.org/n{row}"));
        batch.push_row(&[Some(&iri), Some(&other)]).unwrap();
    }
    w.bytes = batch
        .frame(Header::new(CLUSTER, Command::Result).with_stream(1))
        .unwrap();
}

#[then("its term table holds that IRI once")]
fn iri_once(w: &mut Steps) {
    let frame = FrameView::validate(&w.bytes, CLUSTER).unwrap();
    let view = ResultView::from_frame(&frame).unwrap();
    assert_eq!(view.rows(), 1_000);
    // 1,000 distinct objects and the one IRI.
    assert_eq!(view.term_count(), 1_001);
    let same: Vec<u32> = (0..view.term_count() as u32)
        .filter(|i| view.term(*i).unwrap().term().unwrap().as_uid() == Some(42))
        .collect();
    assert_eq!(same, vec![0]);
    assert!((0..1_000).all(|row| view.cell(0, row) == Some(0)));
}

// The conformance suite.

#[when("each golden frame is truncated at every length below its size and sent")]
fn truncate_all(w: &mut Steps) {
    w.truncations = 0;
    w.refusals.clear();
    for body in BODIES {
        let bytes = body.bytes();
        for len in 0..bytes.len() {
            let outcome = catch_unwind(AssertUnwindSafe(|| view_any(&bytes[..len])));
            match outcome {
                Ok(Err(refusal)) => w.refusals.push(refusal),
                Ok(Ok(())) => panic!("{} truncated at {len} was accepted", body.name),
                Err(_) => panic!("{} truncated at {len} panicked", body.name),
            }
            w.truncations += 1;
        }
    }
}

#[then("each truncation yields a `ProtocolError` and none panics")]
fn every_truncation_refused(w: &mut Steps) {
    let total: usize = BODIES.iter().map(|b| b.size as usize).sum();
    assert_eq!(w.truncations, total);
    assert_eq!(w.refusals.len(), total);
}
