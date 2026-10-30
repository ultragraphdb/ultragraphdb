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
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration,
    TypeDeclaration,
};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, Command, CommitOperation, DeclaredEntityType, DeclaredKind,
    DeclaredPredicate, DeclaredRelation, Describe, DescribeView, ErrorCode, Execute, ExecuteView,
    FrameView, Header, MAX_FRAME_SIZE, Operation, OperationView, PatternSlot, Prepare, PrepareView,
    QuadPattern, Read, ReadView, RequestTerm, RequestTermView, ResultBatch, ResultTerm, ResultView,
    StatementCache, StatementError, TermTable, Write, WriteChange, WriteView,
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
            .with_predicates(d.predicates().iter().map(declared_predicate).collect())
            .with_entity_types(
                d.entity_types()
                    .iter()
                    .map(|(t, preorder)| (declared_entity_type(t), *preorder))
                    .collect(),
            )
            .with_relations(d.relations().iter().map(declared_relation).collect())
    }
}

// The conversions the server session makes from the catalog's declarations (task 10.2), here for the scenarios.

fn declared_predicate(p: &PredicateDeclaration) -> DeclaredPredicate {
    let kind = match p.kind() {
        PredicateKind::Edge => DeclaredKind::Edge,
        PredicateKind::Property => DeclaredKind::Property,
        PredicateKind::Either => DeclaredKind::Either,
    };
    let mut declared = DeclaredPredicate::new(p.predicate().as_str())
        .with_kind(kind)
        .with_single(p.cardinality() == Cardinality::Single)
        .with_unique(p.unique())
        .with_tokenizers(p.tokenizers().to_vec())
        .with_count_index(p.count_index())
        .with_domains(p.domains().iter().map(|d| d.as_str().to_string()).collect());
    if let Some(datatype) = p.datatype() {
        declared = declared.with_datatype(datatype.as_str());
    }
    if let Some(spec) = p.vector_index() {
        declared = declared.with_vector_index(spec.factory(), spec.options().to_vec());
    }
    declared
}

fn declared_entity_type(t: &TypeDeclaration) -> DeclaredEntityType {
    DeclaredEntityType::new(
        t.iri().as_str(),
        t.parent().map(|p| p.as_str().to_string()),
        t.is_abstract(),
    )
}

fn declared_relation(r: &RelationDeclaration) -> DeclaredRelation {
    DeclaredRelation::new(
        r.iri().as_str(),
        r.roles()
            .iter()
            .map(|role| role.as_str().to_string())
            .collect(),
    )
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
    let mut terms = TermTable::new();
    let name = terms.iri("http://ex.org/name");
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Parameter(0),
        PatternSlot::Bound(name),
        PatternSlot::Unbound,
    );
    Prepare::new(0, 0, 0, terms, pattern, vec![ColumnType::Node], Vec::new())
        .frame(Header::new(CLUSTER, Command::Prepare).with_stream(3))
        .unwrap()
}

fn execute_frame(statement: u64) -> Vec<u8> {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Execute::new(statement, 5, 0, terms, vec![a], Vec::new())
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
            &PrepareView::from_frame(
                &FrameView::validate(&request, CLUSTER, MAX_FRAME_SIZE).unwrap(),
            )
            .unwrap(),
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
    let view =
        ExecuteView::from_frame(&FrameView::validate(&execute, CLUSTER, MAX_FRAME_SIZE).unwrap())
            .unwrap();
    w.statement = Some(cache.execute(&view, after).map(|_| ()));
    let again = cache
        .prepare(
            &PrepareView::from_frame(
                &FrameView::validate(&request, CLUSTER, MAX_FRAME_SIZE).unwrap(),
            )
            .unwrap(),
            after,
        )
        .expect("preparing again succeeds");
    let execute = execute_frame(again.statement());
    let view =
        ExecuteView::from_frame(&FrameView::validate(&execute, CLUSTER, MAX_FRAME_SIZE).unwrap())
            .unwrap();
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
    let asked =
        DescribeView::from_frame(&FrameView::validate(&request, CLUSTER, MAX_FRAME_SIZE).unwrap())
            .unwrap();
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
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let view = DescribeView::from_frame(&frame).unwrap();
    assert_eq!(
        view.property_keys().collect::<Vec<_>>(),
        vec!["http://ex.org/name"]
    );
    let answer = view.to_body().unwrap();
    assert_eq!(answer.predicates().len(), 1);
    let declared = &answer.predicates()[0];
    assert_eq!(declared.predicate(), ex("name").as_str());
    assert_eq!(declared.kind(), DeclaredKind::Property);
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
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE).unwrap();
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

// Requests name terms by their text.

fn frame_with(body: &impl BodyBuilder) -> Vec<u8> {
    body.frame(Header::new(CLUSTER, Command::Abort).with_stream(21))
        .unwrap()
}

#[when("a Read binds its subject to the IRI `http://ex.org/a`")]
fn read_binds_iri(w: &mut Steps) {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    w.bytes = frame_with(&Read::new(0, 0, terms, pattern, Vec::new()));
}

#[then("the view reads an IRI entry whose text is `http://ex.org/a`, borrowed from the frame")]
fn iri_read(w: &mut Steps) {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let view = ReadView::from_frame(&frame).unwrap();
    let PatternSlot::Bound(index) = view.pattern().subject() else {
        panic!("the subject is bound");
    };
    let Some(RequestTermView::Iri(text)) = view.terms().get(index) else {
        panic!("the subject is an IRI");
    };
    assert_eq!(text, "http://ex.org/a");
    // In place: the text lies inside the frame's bytes.
    let range = w.bytes.as_ptr_range();
    assert!(range.contains(&text.as_ptr()));
}

#[when("a Write inserts a statement whose subject is the blank node labelled `b1`")]
fn write_blank(w: &mut Steps) {
    let mut terms = TermTable::new();
    let b1 = terms
        .intern(RequestTerm::BlankNode("b1".to_string()))
        .unwrap();
    let p = terms.iri("http://ex.org/p");
    let o = terms.iri("http://ex.org/o");
    let insert = WriteChange::Insert {
        graph: None,
        subject: b1,
        predicate: p,
        object: o,
    };
    w.bytes = frame_with(&Write::new(terms, vec![insert]));
}

#[then("the view reads a blank-node entry labelled `b1`")]
fn blank_read(w: &mut Steps) {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let view = WriteView::from_frame(&frame).unwrap();
    let Some(WriteChange::Insert { subject, .. }) = view.change(0) else {
        panic!("an insert");
    };
    assert_eq!(
        view.terms().get(subject),
        Some(RequestTermView::BlankNode("b1"))
    );
}

#[when("an Execute binds the literal `35` of datatype `http://www.w3.org/2001/XMLSchema#integer`")]
fn execute_typed(w: &mut Steps) {
    let mut terms = TermTable::new();
    let n = terms
        .intern(RequestTerm::Literal {
            lexical: "35".to_string(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".to_string()),
        })
        .unwrap();
    w.bytes = frame_with(&Execute::new(1, 5, 0, terms, vec![n], Vec::new()));
}

#[then("the view reads a literal entry of lexical form `35` and that datatype's IRI")]
fn typed_read(w: &mut Steps) {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let view = ExecuteView::from_frame(&frame).unwrap();
    assert_eq!(
        view.terms().get(view.parameters()[0]),
        Some(RequestTermView::Literal {
            lexical: "35",
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer")
        })
    );
}

#[when("an Operation's value is `chat` tagged `fr`")]
fn operation_tagged(w: &mut Steps) {
    let mut terms = TermTable::new();
    let p = terms.iri("http://ex.org/name");
    let chat = terms
        .intern(RequestTerm::LanguageLiteral {
            lexical: "chat".to_string(),
            language: "fr".to_string(),
            direction: None,
        })
        .unwrap();
    let operation = CommitOperation::InsertIfAbsent {
        graph: None,
        predicate: p,
        value: chat,
        properties: Vec::new(),
    };
    w.bytes = frame_with(&Operation::new(terms, operation));
}

#[then("the view reads a literal entry of lexical form `chat` and language tag `fr`")]
fn tagged_read(w: &mut Steps) {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let view = OperationView::from_frame(&frame).unwrap();
    let CommitOperation::InsertIfAbsent { value, .. } = view.operation() else {
        panic!("an insert-if-absent");
    };
    assert_eq!(
        view.terms().get(value),
        Some(RequestTermView::LanguageLiteral {
            lexical: "chat",
            language: "fr",
            direction: None
        })
    );
}

#[when(
    "a Write's object is the triple term of `http://ex.org/a`, `http://ex.org/knows` and `http://ex.org/b`"
)]
fn write_triple(w: &mut Steps) {
    let mut terms = TermTable::new();
    let (a, knows, b) = (
        terms.iri("http://ex.org/a"),
        terms.iri("http://ex.org/knows"),
        terms.iri("http://ex.org/b"),
    );
    let says = terms.iri("http://ex.org/says");
    let triple = terms
        .intern(RequestTerm::Triple {
            subject: a,
            predicate: knows,
            object: b,
        })
        .unwrap();
    let insert = WriteChange::Insert {
        graph: None,
        subject: a,
        predicate: says,
        object: triple,
    };
    w.bytes = frame_with(&Write::new(terms, vec![insert]));
}

#[then("the view reads a triple-term entry naming three earlier entries, those IRIs")]
fn triple_read(w: &mut Steps) {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let view = WriteView::from_frame(&frame).unwrap();
    let Some(WriteChange::Insert { object, .. }) = view.change(0) else {
        panic!("an insert");
    };
    let Some(RequestTermView::Triple {
        subject,
        predicate,
        object: o,
    }) = view.terms().get(object)
    else {
        panic!("a triple term");
    };
    assert!(subject < object && predicate < object && o < object);
    let text = |i| match view.terms().get(i) {
        Some(RequestTermView::Iri(t)) => t,
        other => panic!("an IRI, not {other:?}"),
    };
    assert_eq!(
        [text(subject), text(predicate), text(o)],
        ["http://ex.org/a", "http://ex.org/knows", "http://ex.org/b"]
    );
}

/// A term record: kind, direction, six zero bytes, then the four u32 fields.
fn record(kind: u8, fields: [u32; 4]) -> Vec<u8> {
    let mut r = vec![kind, 0, 0, 0, 0, 0, 0, 0];
    fields
        .iter()
        .for_each(|f| r.extend_from_slice(&f.to_le_bytes()));
    r
}

/// The golden Execute around a term table of `records` over the text `a`, binding entry 0.
fn execute_with(records: Vec<Vec<u8>>) -> Vec<u8> {
    let body = crate::utils::golden_bodies::join_sections(
        &[],
        &[
            records.concat(),
            b"a".to_vec(),
            0u32.to_le_bytes().to_vec(),
            Vec::new(),
        ],
    );
    reframe(&golden("execute").bytes(), &body)
}

#[when("a triple term's entry names an entry at or after its own index")]
fn forward_index(w: &mut Steps) {
    // Entry 1 names entry 2, which does not precede it.
    w.bytes = execute_with(vec![record(1, [0, 1, 1, 0]), record(5, [0, 0, 2, 0])]);
    w.refusals = view_any(&w.bytes).err().into_iter().collect();
}

#[then("the frame is refused with an error naming the entry and the index")]
fn forward_refused(w: &mut Steps) {
    assert_eq!(
        w.refusals.iter().map(|e| e.0.clone()).collect::<Vec<_>>(),
        vec![ProtocolErrorEnum::IndexOutOfRange {
            command: 11,
            section: 0,
            at: 1,
            index: 2,
            limit: 1
        }]
    );
    assert!(
        w.refusals[0].to_string().contains("entry 1"),
        "{}",
        w.refusals[0]
    );
}

#[when("a request's term table holds a triple term nested 33 deep")]
fn too_deep(w: &mut Steps) {
    let mut records = vec![record(1, [0, 1, 1, 0])];
    for k in 0..33u32 {
        records.push(record(5, [k, 0, 0, 0]));
    }
    w.bytes = execute_with(records);
    w.refusals = view_any(&w.bytes).err().into_iter().collect();
}

#[then("the frame is refused with an error naming the entry and its depth")]
fn depth_refused(w: &mut Steps) {
    assert_eq!(
        w.refusals.iter().map(|e| e.0.clone()).collect::<Vec<_>>(),
        vec![ProtocolErrorEnum::TermTooDeep {
            command: 11,
            section: 0,
            at: 33,
            depth: 33,
            limit: 32
        }]
    );
}

#[when("a statement's subject parameter is bound to a literal")]
fn literal_at_a_node_slot(w: &mut Steps) {
    let mut cache = StatementCache::new(16);
    let request = prepare_view_frame();
    let prepared = cache
        .prepare(
            &PrepareView::from_frame(
                &FrameView::validate(&request, CLUSTER, MAX_FRAME_SIZE).unwrap(),
            )
            .unwrap(),
            1,
        )
        .expect("the statement prepares");
    let mut terms = TermTable::new();
    let literal = terms
        .intern(RequestTerm::Literal {
            lexical: "35".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        })
        .unwrap();
    let execute = Execute::new(prepared.statement(), 5, 0, terms, vec![literal], Vec::new())
        .frame(Header::new(CLUSTER, Command::Execute).with_stream(7))
        .unwrap();
    let view =
        ExecuteView::from_frame(&FrameView::validate(&execute, CLUSTER, MAX_FRAME_SIZE).unwrap())
            .unwrap();
    w.statement = Some(cache.execute(&view, 1).map(|_| ()));
    w.statements = vec![prepared.statement()];
}

#[then("the execution is refused with code 11, `detail` the statement")]
fn refused_not_a_node(w: &mut Steps) {
    let refusal = w.statement.clone().unwrap().unwrap_err();
    let reply = refusal.reply();
    assert_eq!(reply.code(), ErrorCode::Parameters);
    assert_eq!(reply.code().number(), 11);
    assert_eq!(reply.detail(), w.statements[0]);
}

#[when(
    "a literal `\"0.1234567890123456789\"^^xsd:decimal`, of 19 fraction digits, is written and read back"
)]
fn decimal_past_its_payload(w: &mut Steps) {
    let lexical = "0.1234567890123456789";
    let literal = oxrdf::Literal::new_typed_literal(lexical, oxrdf::vocab::xsd::DECIMAL);
    let encoded =
        EncodedTerm::literal::<ultragraphdb_core_terms::TermError>(literal.as_ref(), |_| {
            panic!("a known datatype takes no UID")
        })
        .expect("the literal encodes");
    let text = encoded.strings().next().map(|(_, text)| text.to_string());
    let term = ResultTerm::new(encoded.term().clone());
    let term = match &text {
        Some(text) => term.with_text(text.clone()),
        None => term,
    };
    let mut batch = ResultBatch::new(1);
    batch.push_row(&[Some(&term)]).unwrap();
    let bytes = batch
        .frame(Header::new(CLUSTER, Command::Result).with_stream(3))
        .unwrap();
    let view =
        ResultView::from_frame(&FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE).unwrap())
            .unwrap();
    let read = view.term(0).unwrap();
    w.terms = vec![(read.encoding().to_vec(), read.text().map(str::to_string))];
}

#[then(
    "its term carries tag `0x31`, a known datatype's code and the hash of its lexical form, and the batch's text holds the lexical form unchanged"
)]
fn kept_by_its_lexical_form(w: &mut Steps) {
    let lexical = "0.1234567890123456789";
    let (encoding, text) = &w.terms[0];
    // Tag 0x31, the code of xsd:decimal (3), then BLAKE2b-128 of the lexical form: 18 bytes.
    assert_eq!(encoding.len(), 18);
    assert_eq!(encoding[0], 0x31);
    assert_eq!(encoding[1], 3);
    assert_eq!(
        &encoding[2..],
        ultragraphdb_core_terms::Hash128::of(lexical.as_bytes()).as_bytes()
    );
    assert_eq!(text.as_deref(), Some(lexical));
}
