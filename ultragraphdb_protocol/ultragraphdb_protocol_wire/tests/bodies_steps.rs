/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire-protocol scenarios of bodies, prepared statements, describe, result batches and the conformance suite's
//! truncations.

use std::num::NonZeroU64;
use std::panic::{AssertUnwindSafe, catch_unwind};

use cucumber::{then, when};
use oxrdf::NamedNode;
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, RandomSource,
    RelationDeclaration, TypeDeclaration,
};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, Command, CommitOperation, ConditionalWrite,
    ConditionalWriteView, DeclaredEntityType, DeclaredKind, DeclaredPredicate, DeclaredRelation,
    Describe, DescribeView, DirectWrite, DirectWriteView, Entries, EntriesEnd, EntriesSequence,
    EntriesView, ErrorCode, Execute, ExecuteView, FLAG_LAST, FrameView, Header, MAX_FRAME_SIZE,
    ObjectRecords, ObjectRecordsBuilder, Operation, OperationView, PatternSlot, Prepare,
    PrepareView, QuadPattern, Read, ReadView, RequestTerm, RequestTermView, ResultBatch,
    ResultTerm, ResultView, ScanView, StatementCache, StatementError, TermTable, Write,
    WriteChange, WriteView,
};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::{DataDictionary, QuadStore};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_traits_store::{PersistMode, Store, WriteBatch};

use crate::Steps;
use crate::utils::golden_bodies::{
    BODIES, CLUSTER, body_data, golden, reframe, split_sections, view_any, with_command_header,
    with_section,
};
use crate::utils::golden_roles::{golden_role, role_bytes};

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// Quads over a simulated store, opened fresh, and tenant 0's declarations written into its store at their
/// versions; the data dictionary's answer for the tenant at a version is what a server builds the Describe answer
/// from.
struct Dictionary {
    store: SimStore,
    quads: QuadStore<SimStore, SimListCache<CachedList>, TestBlockSource>,
}

impl Dictionary {
    fn new() -> TestResult<Self> {
        let store = SimStore::new(Seed::new(9), FaultConfig::none(), SimClock::new());
        let cache = SimListCache::new(64, Seed::new(9).stream(Stream::Cache));
        let quads = QuadStore::open(
            Lists::new(store.clone(), cache, ListsConfig::default()),
            TestBlockSource::new(NonZeroU64::MIN.saturating_add(999)),
            RandomSource::new(9),
        )
        .context("the quads open")?;
        Ok(Self { store, quads })
    }

    fn declare(
        &self,
        sequence: u64,
        declaration: PredicateDeclaration,
        version: u64,
    ) -> TestResult {
        let predicate = SystemId::new(
            TypePrefix::RESOURCE,
            Bucket::new(0).context("bucket 0")?,
            Sequence::new(sequence).context("a sequence from 1")?,
        );
        let mut batch = WriteBatch::new();
        batch.push(Catalog::entry(
            0,
            predicate,
            &Declaration::Predicate(declaration),
            version,
        ));
        self.store
            .apply(batch, PersistMode::Sync)
            .context("the declaration is written")?;
        Ok(())
    }

    fn describe(&self, read_ts: u64) -> TestResult<Describe> {
        let d = DataDictionary::new(0)
            .describe(&self.quads, read_ts)
            .context("the dictionary answers")?;
        Ok(Describe::new(0, read_ts)
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
            .with_relations(d.relations().iter().map(declared_relation).collect()))
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
fn offset_past_the_end(w: &mut Steps) -> TestResult {
    // The golden Read: section 2, the pattern's term text, moved past the body's end.
    let bytes = golden("read")?.bytes()?;
    let mut data = body_data(&bytes).to_vec();
    let past = (data.len() + 16) as u32;
    data[16..20].copy_from_slice(&past.to_le_bytes());
    w.refusals = view_any(&reframe(&bytes, &data))?
        .err()
        .into_iter()
        .collect();
    w.bytes = data;
    Ok(())
}

#[then("the frame is refused with an error naming the section")]
fn section_named(w: &mut Steps) -> TestResult {
    let len = u32::from_le_bytes(w.bytes[20..24].try_into()?);
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
    Ok(())
}

// Data requests, prepared statements and describe.

fn prepare_view_frame() -> TestResult<Vec<u8>> {
    let mut terms = TermTable::new();
    let name = terms.iri("http://ex.org/name");
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Parameter(0),
        PatternSlot::Bound(name),
        PatternSlot::Unbound,
    );
    Ok(
        Prepare::new(0, 0, 0, terms, pattern, vec![ColumnType::Node], Vec::new())
            .frame(Header::new(CLUSTER, Command::Prepare).with_stream(3))?,
    )
}

fn execute_frame(statement: u64) -> TestResult<Vec<u8>> {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Ok(Execute::new(statement, 5, 0, terms, vec![a], Vec::new())
        .frame(Header::new(CLUSTER, Command::Execute).with_stream(5))?)
}

#[when("a statement is prepared, the catalog of its tenant changes, and the statement is executed")]
fn prepared_then_changed(w: &mut Steps) -> TestResult {
    let dictionary = Dictionary::new()?;
    dictionary.declare(7, name(), 10)?;
    let before = dictionary.describe(10)?.schema_id();
    let mut cache = StatementCache::new(16);
    let request = prepare_view_frame()?;
    let prepared = cache
        .prepare(
            &PrepareView::from_frame(&FrameView::validate(&request, CLUSTER, MAX_FRAME_SIZE)?)?,
            before,
        )
        .context("the statement prepares")?;
    // The catalog changes: `age` is declared.
    dictionary.declare(
        8,
        PredicateDeclaration::new(ex("age")).with_kind(PredicateKind::Property),
        20,
    )?;
    let after = dictionary.describe(20)?.schema_id();
    assert_ne!(before, after, "a declaration changes the schema id");
    let execute = execute_frame(prepared.statement())?;
    let view = ExecuteView::from_frame(&FrameView::validate(&execute, CLUSTER, MAX_FRAME_SIZE)?)?;
    w.statement = Some(cache.execute(&view, after).map(|_| ()));
    let again = cache
        .prepare(
            &PrepareView::from_frame(&FrameView::validate(&request, CLUSTER, MAX_FRAME_SIZE)?)?,
            after,
        )
        .context("preparing again succeeds")?;
    let execute_again = execute_frame(again.statement())?;
    let view_again = ExecuteView::from_frame(&FrameView::validate(
        &execute_again,
        CLUSTER,
        MAX_FRAME_SIZE,
    )?)?;
    w.statement_again = Some(cache.execute(&view_again, after).map(|_| ()));
    w.schemas = vec![before, after];
    w.statements = vec![prepared.statement(), again.statement()];
    Ok(())
}

#[then("the execution is refused as stale, and preparing it again succeeds")]
fn refused_stale(w: &mut Steps) -> TestResult {
    let refusal = w
        .statement
        .clone()
        .context("a prepared statement")?
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal,
        StatementError::stale(w.statements[0], w.schemas[0], w.schemas[1])
    );
    assert_eq!(refusal.code(), ErrorCode::StaleStatement);
    assert_eq!(refusal.reply().code(), ErrorCode::StaleStatement);
    assert_ne!(w.statements[0], w.statements[1]);
    assert_eq!(w.statement_again, Some(Ok(())));
    Ok(())
}

#[when(
    "`name` is declared a property with a `term` index and a client sends Describe for the tenant"
)]
fn describe_name(w: &mut Steps) -> TestResult {
    let dictionary = Dictionary::new()?;
    dictionary.declare(7, name(), 10)?;
    let request =
        Describe::request(0).frame(Header::new(CLUSTER, Command::Describe).with_stream(9))?;
    let asked = DescribeView::from_frame(&FrameView::validate(&request, CLUSTER, MAX_FRAME_SIZE)?)?;
    // The server answers the tenant asked, from the dictionary.
    let answer = dictionary.describe(10)?;
    assert_eq!(asked.tenant(), answer.tenant());
    w.bytes = answer.frame(Header::new(CLUSTER, Command::Describe).with_stream(9))?;
    Ok(())
}

#[then(
    "the answer lists `name` as a property with its `term` index, as the data dictionary reports it"
)]
fn name_listed(w: &mut Steps) -> TestResult {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let view = DescribeView::from_frame(&frame)?;
    assert_eq!(
        view.property_keys().collect::<Vec<_>>(),
        vec!["http://ex.org/name"]
    );
    let answer = view.to_body()?;
    assert_eq!(answer.predicates().len(), 1);
    let declared = &answer.predicates()[0];
    assert_eq!(declared.predicate(), ex("name").as_str());
    assert_eq!(declared.kind(), DeclaredKind::Property);
    assert_eq!(declared.tokenizers(), &["term".to_string()]);
    assert_eq!(answer.read_version(), 10);
    Ok(())
}

// Result batches are columnar and dictionary-encoded.

#[when("a batch of 1,000 rows names the same IRI in every row")]
fn thousand_rows(w: &mut Steps) -> TestResult {
    let iri = ResultTerm::iri("http://ex.org/same");
    // Two unbound slots and the reifier column, unbound: no row's statement holds a statement ID.
    let mut batch = ResultBatch::new(3);
    for row in 0..1_000u64 {
        let other = ResultTerm::iri(format!("http://ex.org/n{row}"));
        batch.push_row(&[Some(&iri), Some(&other), None])?;
    }
    w.bytes = batch.frame(Header::new(CLUSTER, Command::Result).with_stream(1))?;
    Ok(())
}

#[then("its term table holds that IRI once")]
fn iri_once(w: &mut Steps) -> TestResult {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let view = ResultView::from_frame(&frame)?;
    assert_eq!(view.rows(), 1_000);
    // 1,000 distinct objects and the one IRI.
    assert_eq!(view.term_count(), 1_001);
    let mut same: Vec<u32> = Vec::new();
    for i in 0..view.term_count() as u32 {
        let term = view.term(i).context("each term of the batch")?;
        if term.text() == Some("http://ex.org/same") {
            same.push(i);
        }
    }
    assert_eq!(same, vec![0]);
    assert!((0..1_000).all(|row| view.cell(0, row) == Some(0)));
    Ok(())
}

// The conformance suite.

#[when("each golden frame is truncated at every length below its size and sent")]
fn truncate_all(w: &mut Steps) -> TestResult {
    w.truncations = 0;
    w.refusals.clear();
    for body in BODIES {
        let bytes = body.bytes()?;
        for len in 0..bytes.len() {
            let refusal = catch_unwind(AssertUnwindSafe(|| view_any(&bytes[..len])))
                .map_err(|_| format!("{} truncated at {len} panicked", body.name))
                .context("a truncated frame does not panic")??
                .err()
                .ok_or_else(|| format!("{} truncated at {len} was accepted", body.name))
                .context("a truncated frame is refused")?;
            w.refusals.push(refusal);
            w.truncations += 1;
        }
    }
    Ok(())
}

#[then("each truncation yields a `ProtocolError` and none panics")]
fn every_truncation_refused(w: &mut Steps) {
    let total: usize = BODIES.iter().map(|b| b.size as usize).sum();
    assert_eq!(w.truncations, total);
    assert_eq!(w.refusals.len(), total);
    // PROTOCOL.md section 2.5, steps 1 and 9: fewer than 256 bytes is `ShortHeader`, a whole header with fewer bytes
    // than its `size` is `LengthMismatch`; the refusals are in the order the truncations were sent.
    let expected = BODIES.iter().flat_map(|body| {
        (0..body.size as usize).map(move |len| {
            if len < 256 {
                ProtocolErrorEnum::ShortHeader { len }
            } else {
                ProtocolErrorEnum::LengthMismatch {
                    size: body.size,
                    len,
                }
            }
        })
    });
    for (refusal, want) in w.refusals.iter().zip(expected) {
        assert_eq!(refusal.0, want);
    }
}

// Requests name terms by their text.

fn frame_with(body: &impl BodyBuilder) -> TestResult<Vec<u8>> {
    Ok(body.frame(Header::new(CLUSTER, Command::Abort).with_stream(21))?)
}

#[when("a Read binds its subject to the IRI `http://ex.org/a`")]
fn read_binds_iri(w: &mut Steps) -> TestResult {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    w.bytes = frame_with(&Read::new(0, 0, terms, pattern, Vec::new()))?;
    Ok(())
}

#[then("the view reads an IRI entry whose text is `http://ex.org/a`, borrowed from the frame")]
fn iri_read(w: &mut Steps) -> TestResult {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let view = ReadView::from_frame(&frame)?;
    let subject = view.pattern().subject();
    assert!(
        matches!(subject, PatternSlot::Bound(_)),
        "the subject is bound: {subject:?}"
    );
    let PatternSlot::Bound(index) = subject else {
        return Ok(());
    };
    let term = view.terms().get(index);
    assert!(
        matches!(term, Some(RequestTermView::Iri(_))),
        "the subject is an IRI: {term:?}"
    );
    let Some(RequestTermView::Iri(text)) = term else {
        return Ok(());
    };
    assert_eq!(text, "http://ex.org/a");
    // In place: the text lies inside the frame's bytes.
    let range = w.bytes.as_ptr_range();
    assert!(range.contains(&text.as_ptr()));
    Ok(())
}

#[when("a Write inserts a statement whose subject is the blank node labelled `b1`")]
fn write_blank(w: &mut Steps) -> TestResult {
    let mut terms = TermTable::new();
    let b1 = terms.intern(RequestTerm::BlankNode("b1".to_string()))?;
    let p = terms.iri("http://ex.org/p");
    let o = terms.iri("http://ex.org/o");
    let insert = WriteChange::Insert {
        graph: None,
        subject: b1,
        predicate: p,
        object: o,
    };
    w.bytes = frame_with(&Write::new(terms, vec![insert]))?;
    Ok(())
}

#[then("the view reads a blank-node entry labelled `b1`")]
fn blank_read(w: &mut Steps) -> TestResult {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let view = WriteView::from_frame(&frame)?;
    let change = view.change(0);
    assert!(
        matches!(change, Some(WriteChange::Insert { .. })),
        "an insert: {change:?}"
    );
    let Some(WriteChange::Insert { subject, .. }) = change else {
        return Ok(());
    };
    assert_eq!(
        view.terms().get(subject),
        Some(RequestTermView::BlankNode("b1"))
    );
    Ok(())
}

#[when("an Execute binds the literal `35` of datatype `http://www.w3.org/2001/XMLSchema#integer`")]
fn execute_typed(w: &mut Steps) -> TestResult {
    let mut terms = TermTable::new();
    let n = terms.intern(RequestTerm::Literal {
        lexical: "35".to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".to_string()),
    })?;
    w.bytes = frame_with(&Execute::new(1, 5, 0, terms, vec![n], Vec::new()))?;
    Ok(())
}

#[then("the view reads a literal entry of lexical form `35` and that datatype's IRI")]
fn typed_read(w: &mut Steps) -> TestResult {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let view = ExecuteView::from_frame(&frame)?;
    assert_eq!(
        view.terms().get(view.parameters()[0]),
        Some(RequestTermView::Literal {
            lexical: "35",
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer")
        })
    );
    Ok(())
}

#[when("an Operation's value is `chat` tagged `fr`")]
fn operation_tagged(w: &mut Steps) -> TestResult {
    let mut terms = TermTable::new();
    let p = terms.iri("http://ex.org/name");
    let chat = terms.intern(RequestTerm::LanguageLiteral {
        lexical: "chat".to_string(),
        language: "fr".to_string(),
        direction: None,
    })?;
    let operation = CommitOperation::InsertIfAbsent {
        graph: None,
        predicate: p,
        value: chat,
        properties: Vec::new(),
    };
    w.bytes = frame_with(&Operation::new(terms, operation))?;
    Ok(())
}

#[then("the view reads a literal entry of lexical form `chat` and language tag `fr`")]
fn tagged_read(w: &mut Steps) -> TestResult {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let view = OperationView::from_frame(&frame)?;
    let operation = view.operation();
    assert!(
        matches!(operation, CommitOperation::InsertIfAbsent { .. }),
        "an insert-if-absent: {operation:?}"
    );
    let CommitOperation::InsertIfAbsent { value, .. } = operation else {
        return Ok(());
    };
    assert_eq!(
        view.terms().get(value),
        Some(RequestTermView::LanguageLiteral {
            lexical: "chat",
            language: "fr",
            direction: None
        })
    );
    Ok(())
}

#[when(
    "a Write's object is the triple term of `http://ex.org/a`, `http://ex.org/knows` and `http://ex.org/b`"
)]
fn write_triple(w: &mut Steps) -> TestResult {
    let mut terms = TermTable::new();
    let (a, knows, b) = (
        terms.iri("http://ex.org/a"),
        terms.iri("http://ex.org/knows"),
        terms.iri("http://ex.org/b"),
    );
    let says = terms.iri("http://ex.org/says");
    let triple = terms.intern(RequestTerm::Triple {
        subject: a,
        predicate: knows,
        object: b,
    })?;
    let insert = WriteChange::Insert {
        graph: None,
        subject: a,
        predicate: says,
        object: triple,
    };
    w.bytes = frame_with(&Write::new(terms, vec![insert]))?;
    Ok(())
}

#[then("the view reads a triple-term entry naming three earlier entries, those IRIs")]
fn triple_read(w: &mut Steps) -> TestResult {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let view = WriteView::from_frame(&frame)?;
    let change = view.change(0);
    assert!(
        matches!(change, Some(WriteChange::Insert { .. })),
        "an insert: {change:?}"
    );
    let Some(WriteChange::Insert { object, .. }) = change else {
        return Ok(());
    };
    let term = view.terms().get(object);
    assert!(
        matches!(term, Some(RequestTermView::Triple { .. })),
        "a triple term: {term:?}"
    );
    let Some(RequestTermView::Triple {
        subject,
        predicate,
        object: o,
    }) = term
    else {
        return Ok(());
    };
    assert!(subject < object && predicate < object && o < object);
    let text = |i| view.terms().get(i);
    assert_eq!(
        [text(subject), text(predicate), text(o)],
        [
            Some(RequestTermView::Iri("http://ex.org/a")),
            Some(RequestTermView::Iri("http://ex.org/knows")),
            Some(RequestTermView::Iri("http://ex.org/b")),
        ]
    );
    Ok(())
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
fn execute_with(records: &[Vec<u8>]) -> TestResult<Vec<u8>> {
    let body = crate::utils::golden_bodies::join_sections(
        &[],
        &[
            records.concat(),
            b"a".to_vec(),
            0u32.to_le_bytes().to_vec(),
            Vec::new(),
        ],
    );
    Ok(reframe(&golden("execute")?.bytes()?, &body))
}

#[when("a triple term's entry names an entry at or after its own index")]
fn forward_index(w: &mut Steps) -> TestResult {
    // Entry 1 names entry 2, which does not precede it.
    w.bytes = execute_with(&[record(1, [0, 1, 1, 0]), record(5, [0, 0, 2, 0])])?;
    w.refusals = view_any(&w.bytes)?.err().into_iter().collect();
    Ok(())
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
fn too_deep(w: &mut Steps) -> TestResult {
    let mut records = vec![record(1, [0, 1, 1, 0])];
    for k in 0..33u32 {
        records.push(record(5, [k, 0, 0, 0]));
    }
    w.bytes = execute_with(&records)?;
    w.refusals = view_any(&w.bytes)?.err().into_iter().collect();
    Ok(())
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
fn literal_at_a_node_slot(w: &mut Steps) -> TestResult {
    let mut cache = StatementCache::new(16);
    let request = prepare_view_frame()?;
    let prepared = cache
        .prepare(
            &PrepareView::from_frame(&FrameView::validate(&request, CLUSTER, MAX_FRAME_SIZE)?)?,
            1,
        )
        .context("the statement prepares")?;
    let mut terms = TermTable::new();
    let literal = terms.intern(RequestTerm::Literal {
        lexical: "35".into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
    })?;
    let execute = Execute::new(prepared.statement(), 5, 0, terms, vec![literal], Vec::new())
        .frame(Header::new(CLUSTER, Command::Execute).with_stream(7))?;
    let view = ExecuteView::from_frame(&FrameView::validate(&execute, CLUSTER, MAX_FRAME_SIZE)?)?;
    w.statement = Some(cache.execute(&view, 1).map(|_| ()));
    w.statements = vec![prepared.statement()];
    Ok(())
}

#[then("the execution is refused with code 11, `detail` the statement")]
fn refused_not_a_node(w: &mut Steps) -> TestResult {
    let refusal = w
        .statement
        .clone()
        .context("a prepared statement")?
        .err()
        .context("a refusal")?;
    let reply = refusal.reply();
    assert_eq!(reply.code(), ErrorCode::Parameters);
    assert_eq!(reply.code().number(), 11);
    assert_eq!(reply.detail(), w.statements[0]);
    Ok(())
}

#[when(
    "a literal `\"0.1234567890123456789\"^^xsd:decimal`, of 19 fraction digits, is written and read back"
)]
fn decimal_past_its_payload(w: &mut Steps) -> TestResult {
    let lexical = "0.1234567890123456789";
    let literal = oxrdf::Literal::new_typed_literal(lexical, oxrdf::vocab::xsd::DECIMAL);
    let encoded = EncodedTerm::literal(literal.as_ref(), |_| {
        Err(ultragraphdb_core_terms::TermError::malformed(
            Vec::new(),
            "a known datatype takes no UID",
        ))
    })
    .context("the literal encodes")?;
    let text = encoded.strings().next().map(|(_, text)| text.to_string());
    let term = ResultTerm::new(encoded.term().clone());
    let term = match &text {
        Some(text) => term.with_text(text.clone()),
        None => term,
    };
    let mut batch = ResultBatch::new(2);
    batch.push_row(&[Some(&term), None])?;
    let bytes = batch.frame(Header::new(CLUSTER, Command::Result).with_stream(3))?;
    let view = ResultView::from_frame(&FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE)?)?;
    let read = view.term(0).context("term 0 of the batch")?;
    w.terms = vec![(read.encoding().to_vec(), read.text().map(str::to_string))];
    Ok(())
}

#[then(
    "its term carries tag `0x31`, a known datatype's code and the hash of its lexical form, and the batch's text holds the lexical form unchanged"
)]
fn kept_by_its_lexical_form(w: &mut Steps) {
    let lexical = "0.1234567890123456789";
    let (encoding, text) = &w.terms[0];
    // Tag 0x31, the code of xsd:decimal (3), then the 128-bit hash of the lexical form: 18 bytes.
    assert_eq!(encoding.len(), 18);
    assert_eq!(encoding[0], 0x31);
    assert_eq!(encoding[1], 3);
    assert_eq!(
        &encoding[2..],
        ultragraphdb_core_terms::Hash128::of(lexical.as_bytes()).as_bytes()
    );
    assert_eq!(text.as_deref(), Some(lexical));
}

// A write commits in one frame.

/// The refusal of a frame through the view `V`; none when it reads.
fn view_refusal<'a, V: BodyView<'a>>(
    frame: &FrameView<'a>,
) -> Option<ultragraphdb_error_protocol::ProtocolError> {
    V::from_frame(frame).err()
}

#[when("a DirectWrite holds no change, or a ConditionalWrite no operation")]
fn a_malformed_write(w: &mut Steps) -> TestResult {
    let header = Header::new(CLUSTER, Command::Abort);
    let direct = DirectWrite::new(0, TermTable::new(), Vec::new()).frame(header.clone())?;
    let conditional =
        ConditionalWrite::new(0, TermTable::new(), Vec::new(), Vec::new()).frame(header)?;
    w.refusals = [
        view_refusal::<DirectWriteView>(&FrameView::validate(&direct, CLUSTER, MAX_FRAME_SIZE)?),
        view_refusal::<ConditionalWriteView>(&FrameView::validate(
            &conditional,
            CLUSTER,
            MAX_FRAME_SIZE,
        )?),
    ]
    .into_iter()
    .flatten()
    .collect();
    Ok(())
}

#[then("it is refused with `CountMismatch`, naming `changes` or `operations`")]
fn refused_naming_what_it_lacks(w: &mut Steps) {
    let refusal = |command: Command, field: &'static str| ProtocolErrorEnum::CountMismatch {
        command: command.number(),
        field,
        count: 0,
        expected: 1,
    };
    assert_eq!(
        w.refusals.iter().map(|e| e.0.clone()).collect::<Vec<_>>(),
        vec![
            refusal(Command::DirectWrite, "changes"),
            refusal(Command::ConditionalWrite, "operations"),
        ]
    );
}

#[when(
    "a DirectWrite or ConditionalWrite holds a name, or names a request number that does not fit its session"
)]
fn a_write_of_two_encodings(w: &mut Steps) -> TestResult {
    let name = b"http://ex.org/name";
    let frames = [
        (golden("direct_write")?.bytes()?, 6),
        (golden("conditional_write")?.bytes()?, 7),
    ];
    w.refusals = Vec::new();
    for (bytes, sections) in &frames {
        let named = with_section(bytes, *sections, 5, |text| text.extend_from_slice(name));
        let named = with_section(&named, *sections, 4, |ends| {
            ends.extend_from_slice(&u32::try_from(name.len()).unwrap_or(0).to_le_bytes());
        });
        let request_0 = with_command_header(bytes, |h| h[16..24].fill(0));
        let no_session = with_command_header(bytes, |h| h[..16].fill(0));
        for frame in [named, request_0, no_session] {
            w.refusals.extend(view_any(&frame)?.err());
        }
    }
    Ok(())
}

#[then("it is refused with `CountMismatch` naming `names`, or `FieldValue` naming `request`")]
fn refused_naming_names_or_request(w: &mut Steps) -> TestResult {
    let direct_bytes = golden("direct_write")?.bytes()?;
    let conditional_bytes = golden("conditional_write")?.bytes()?;
    let direct_request = DirectWriteView::from_frame(&FrameView::validate(
        &direct_bytes,
        CLUSTER,
        MAX_FRAME_SIZE,
    )?)?
    .request();
    let conditional_request = ConditionalWriteView::from_frame(&FrameView::validate(
        &conditional_bytes,
        CLUSTER,
        MAX_FRAME_SIZE,
    )?)?
    .request();
    let names = |command: Command| ProtocolErrorEnum::CountMismatch {
        command: command.number(),
        field: "names",
        count: 1,
        expected: 0,
    };
    let request = |command: Command, value: u64| ProtocolErrorEnum::FieldValue {
        command: command.number(),
        field: "request",
        value,
    };
    let (direct, conditional) = (Command::DirectWrite, Command::ConditionalWrite);
    assert_eq!(
        w.refusals.iter().map(|e| e.0.clone()).collect::<Vec<_>>(),
        vec![
            names(direct),
            request(direct, 0),
            request(direct, direct_request),
            names(conditional),
            request(conditional, 0),
            request(conditional, conditional_request),
        ]
    );
    Ok(())
}

// Roles exchange role commands.

/// The golden role frame `name`.
fn role_frame(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The refusal of a frame through the view `V`, which must refuse it.
fn role_refusal<'a, V: BodyView<'a>>(bytes: &'a [u8]) -> TestResult<ProtocolErrorEnum> {
    let frame = FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?;
    Ok(V::from_frame(&frame).err().context("a refusal")?.0)
}

/// The refusals of the frames `frames`, in order, by a sequence of a Scan of `ranges` ranges.
fn sequence_refusals(frames: &[Vec<u8>], ranges: u32) -> TestResult<Vec<ProtocolError>> {
    let mut sequence = EntriesSequence::new(ranges);
    let mut refusals = Vec::new();
    for bytes in frames {
        let frame = FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?;
        refusals.extend(sequence.accept(&frame).err());
    }
    Ok(refusals)
}

#[when("a client connection sends a block-lease command")]
fn a_client_sends_a_lease(w: &mut Steps) -> TestResult {
    let lease = role_frame("lease")?;
    let frame = FrameView::validate_client(&lease, CLUSTER, MAX_FRAME_SIZE)?;
    w.refusals = frame
        .command()
        .on_client_connection()
        .err()
        .into_iter()
        .collect();
    Ok(())
}

#[then("the frame is refused with a protocol error naming the command, and nothing is leased")]
fn refused_naming_the_command(w: &mut Steps) {
    assert_eq!(
        w.refusals.iter().map(|e| e.0.clone()).collect::<Vec<_>>(),
        vec![ProtocolErrorEnum::CommandNotServed {
            command: Command::Lease.number()
        }]
    );
}

#[when("a Scan names a transaction and the IRIs its first ranges look up")]
fn a_scan_naming_iris(w: &mut Steps) -> TestResult {
    w.bytes = role_frame("scan_transaction_terms")?;
    Ok(())
}

#[then("the view reads each IRI's text from the Scan's term table, borrowed from the frame")]
fn the_view_reads_each_iri(w: &mut Steps) -> TestResult {
    let frame = FrameView::validate(&w.bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let view = ScanView::from_frame(&frame)?;
    let terms = view.terms();
    assert_eq!(terms.len(), 2);
    assert!(view.range_count() >= terms.len());
    for index in 0..2 {
        let Some(RequestTermView::Iri(iri)) = terms.get(index) else {
            return Err(TestError::new("an IRI entry"));
        };
        let start = w.bytes.as_ptr_range();
        assert!(start.contains(&iri.as_ptr()), "borrowed from the frame");
    }
    assert_eq!(view_any(&w.bytes)?, Ok(()));
    Ok(())
}

#[when("a Scan with `transaction` zero holds a term")]
fn a_scan_of_no_transaction_with_a_term(w: &mut Steps) -> TestResult {
    let scan = role_frame("scan")?;
    let named = role_frame("scan_transaction_terms")?;
    let records = split_sections(body_data(&named), 0, 4)
        .into_iter()
        .nth(2)
        .context("the term table's records")?;
    w.bytes = with_section(&scan, 4, 2, |section| *section = records);
    Ok(())
}

#[when("a Scan's term table holds more terms than the Scan has ranges")]
fn a_scan_of_more_terms_than_ranges(w: &mut Steps) -> TestResult {
    let named = role_frame("scan_transaction_terms")?;
    w.bytes = with_section(&named, 4, 0, |ranges| ranges.truncate(24));
    Ok(())
}

#[then("it is refused with `CountMismatch` naming `terms`")]
fn refused_naming_terms(w: &mut Steps) -> TestResult {
    assert!(matches!(
        role_refusal::<ScanView>(&w.bytes)?,
        ProtocolErrorEnum::CountMismatch { field: "terms", .. }
    ));
    Ok(())
}

#[when(
    "a Scan names a transaction with `at` not 1, a nonzero `epoch`, or a term that is not an IRI"
)]
fn a_scan_read_otherwise(w: &mut Steps) -> TestResult {
    let named = role_frame("scan_transaction_terms")?;
    w.frames = vec![
        with_command_header(&named, |h| h[0] = 2),
        with_command_header(&named, |h| h[16] = 1),
        // The second entry's kind byte a blank node's, which lays out as an IRI's does.
        with_section(&named, 4, 2, |records| records[24] = 2),
    ];
    Ok(())
}

#[then("it is refused with `FieldValue` naming `at`, `epoch` or `scan term`")]
fn refused_naming_at_epoch_or_term(w: &mut Steps) -> TestResult {
    let mut fields = Vec::new();
    for bytes in &w.frames {
        if let ProtocolErrorEnum::FieldValue { field, .. } = role_refusal::<ScanView>(bytes)? {
            fields.push(field);
        }
    }
    assert_eq!(fields, ["at", "epoch", "scan term"]);
    Ok(())
}

#[when("a Scan whose `epoch` is 0 holds `deadline_ms` above 0, with a transaction or with none")]
fn a_scan_with_the_wrong_deadline(w: &mut Steps) -> TestResult {
    w.frames = vec![
        with_command_header(&role_frame("scan")?, |h| {
            h[16..24].fill(0);
            h[52..56].fill(0);
            h[52] = 1;
        }),
        with_command_header(&role_frame("scan_transaction")?, |h| h[52] = 1),
    ];
    Ok(())
}

#[then("each is refused with `FieldValue` naming `deadline_ms`")]
fn refused_naming_the_deadline(w: &mut Steps) -> TestResult {
    let refusals = w
        .frames
        .iter()
        .map(|bytes| role_refusal::<ScanView>(bytes))
        .collect::<TestResult<Vec<_>>>()?;
    let field = |value| ProtocolErrorEnum::FieldValue {
        command: Command::Scan.number(),
        field: "deadline_ms",
        value,
    };
    assert_eq!(refusals, [field(1), field(1)]);
    Ok(())
}

#[when("a list's object records span two Entries frames, the first with `continues` 1")]
fn a_list_across_two_frames(w: &mut Steps) -> TestResult {
    w.frames = vec![
        role_frame("entries_continues")?,
        role_frame("entries_continued")?,
    ];
    Ok(())
}

#[then(
    "the second's first entry holds the same key, and the records of both are the list's, in order"
)]
fn the_receiver_takes_both(w: &mut Steps) -> TestResult {
    let (first, second) = (
        w.frames.first().context("the first frame")?,
        w.frames.last().context("the second frame")?,
    );
    let (first_frame, second_frame) = (
        FrameView::validate(first, CLUSTER, MAX_FRAME_SIZE)?,
        FrameView::validate(second, CLUSTER, MAX_FRAME_SIZE)?,
    );
    let mut sequence = EntriesSequence::new(1);
    let continued = sequence.accept(&first_frame)?;
    let opened = sequence.accept(&second_frame)?;
    assert!(continued.continues());
    let last = continued.entries().iter().last().context("a last entry")?;
    let next = opened.entries().iter().next().context("a first entry")?;
    assert_eq!(last.key(), next.key());
    let (head, tail) = (
        last.value().context("the value's head")?,
        next.value().context("the value's tail")?,
    );
    let list = [head, tail].concat();
    let whole = ObjectRecords::read(&list)?;
    let (first_part, second_part) = (ObjectRecords::read(head)?, ObjectRecords::read(tail)?);
    let in_order: Vec<_> = first_part.iter().chain(second_part.iter()).collect();
    assert_eq!(in_order, whole.iter().collect::<Vec<_>>());
    // Each record's encoding and statement IDs, laid out again, are the list's bytes.
    let mut rebuilt = ObjectRecordsBuilder::new();
    for record in &in_order {
        let statements: Vec<[u8; 16]> = record.statement_ids().collect();
        rebuilt.push(record.encoding(), &statements);
    }
    assert_eq!(rebuilt.value(), list.as_slice());
    Ok(())
}

#[when("a frame holds `continues` 1 with `end` not 0, or with no entry")]
fn a_continuing_frame_that_cannot(w: &mut Steps) -> TestResult {
    let header = Header::new(CLUSTER, Command::Abort);
    w.frames = vec![
        with_command_header(&role_frame("entries_continues")?, |h| h[8] = 1),
        Entries::continuing(0, 0).frame(header)?,
    ];
    Ok(())
}

#[then("it is refused with `FieldValue` naming `continues`")]
fn refused_naming_continues(w: &mut Steps) -> TestResult {
    assert_eq!(w.frames.len(), 2);
    for bytes in &w.frames {
        assert!(matches!(
            role_refusal::<EntriesView>(bytes)?,
            ProtocolErrorEnum::FieldValue {
                field: "continues",
                value: 1,
                ..
            }
        ));
    }
    Ok(())
}

#[when(
    "the frame after one with `continues` 1 holds no entry, or a first key that is not the same"
)]
fn a_continuation_that_stops(w: &mut Steps) -> TestResult {
    let header = Header::new(CLUSTER, Command::Abort);
    let continues = role_frame("entries_continues")?;
    let empty = Entries::new(0, EntriesEnd::More, 0).frame(header.clone())?;
    let mut other = Entries::new(0, EntriesEnd::More, 0);
    other.push(b"\x10list-c", b"");
    w.refusals = Vec::new();
    for next in [empty, other.frame(header)?] {
        let refused = sequence_refusals(&[continues.clone(), next], 1)?;
        w.refusals.extend(refused);
    }
    Ok(())
}

#[then("the receiver refuses it with `FieldValue` naming `entry order`")]
fn refused_naming_entry_order(w: &mut Steps) {
    assert_eq!(w.refusals.len(), 2);
    for refusal in &w.refusals {
        assert!(matches!(
            refusal.0,
            ProtocolErrorEnum::FieldValue {
                field: "entry order",
                value: 0,
                ..
            }
        ));
    }
}

#[when(
    "a frame carries `LAST` before the last range has ended, or the last range ends with no `LAST`"
)]
fn an_answer_ending_early(w: &mut Steps) -> TestResult {
    let header = Header::new(CLUSTER, Command::Abort);
    let ended = Entries::new(0, EntriesEnd::Ended, 0);
    let flagged = ended.frame(header.clone().with_flags(FLAG_LAST))?;
    let bare = ended.frame(header)?;
    w.refusals = sequence_refusals(&[flagged], 2)?;
    w.refusals.extend(sequence_refusals(&[bare], 1)?);
    Ok(())
}

#[then("the receiver refuses it with `FieldValue` naming `end`")]
fn refused_naming_end(w: &mut Steps) {
    assert_eq!(w.refusals.len(), 2);
    for refusal in &w.refusals {
        assert!(matches!(
            refusal.0,
            ProtocolErrorEnum::FieldValue {
                field: "end",
                value: 0,
                ..
            }
        ));
    }
}

#[when("an object record's encoding or statement IDs run past the value")]
fn a_value_ending_inside_a_record(w: &mut Steps) -> TestResult {
    let bytes = role_frame("entries_continued")?;
    let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let view = EntriesView::from_frame(&frame)?;
    let value = view
        .entries()
        .iter()
        .next()
        .and_then(|entry| entry.value())
        .context("a value")?;
    // The first record's encoding cut short, and the last record's statement ID.
    w.frames = vec![
        value.get(..10).context("a prefix")?.to_vec(),
        value
            .get(..value.len().saturating_sub(1))
            .context("a prefix")?
            .to_vec(),
    ];
    Ok(())
}

#[then("the reader refuses it with `EntryOutOfBounds`")]
fn refused_naming_the_record(w: &mut Steps) -> TestResult {
    let entries: Vec<_> = w
        .frames
        .iter()
        .map(|value| ObjectRecords::read(value).err().map(|e| e.0))
        .collect();
    assert_eq!(entries.len(), 2);
    assert!(matches!(
        entries.first(),
        Some(Some(ProtocolErrorEnum::EntryOutOfBounds { entry: 0, .. }))
    ));
    assert!(matches!(
        entries.last(),
        Some(Some(ProtocolErrorEnum::EntryOutOfBounds { entry: 1, .. }))
    ));
    Ok(())
}
