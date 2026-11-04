/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The transactions scenarios a worker's transaction carries, over a changeset applier on the simulated store. A
//! commit that is acknowledged is applied by the applier on its own; a transaction's clock is the scenario's, in
//! microseconds, so waiting 11 s is a clock 11 s later.

use cucumber::{World, then, when};
use oxrdf::vocab::xsd;
use oxrdf::{BlankNode, GraphName, Literal, Term};
use ultragraphdb_core_mapping::{Change, Changeset};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, FrameView, Header, MAX_FRAME_SIZE, RequestTerm, TermTable,
    Write, WriteChange, WriteView,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_txn::{
    Commit, FrameChanges, FrameTerms, Mode, Transaction, TxnBuffers, TxnError, TxnErrorEnum,
    TxnLimits,
};

/// The unit tests' fixture, shared by path.
pub mod utils {
    #[path = "fixture.rs"]
    pub mod fixture;
}

use crate::utils::fixture::{
    NS, SECOND, Txn, applier, begin, changeset, commit, ex, held, insert, insert_literal, objects,
    store,
};

const CLUSTER: u64 = 7;

#[derive(Debug, Default, World)]
struct Steps {
    read: Vec<Term>,
    later: Vec<Term>,
    errors: Vec<TxnError>,
    versions: Vec<u64>,
    ended: bool,
    records: Vec<Vec<u8>>,
}

#[when(
    "a client's Begin names a read version above the visibility watermark of the partition answering it"
)]
fn read_above(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let watermark = commit(&applier, vec![insert_literal("a", "name", "A")])?;
    let above = watermark + 2;
    w.versions = vec![above, watermark];
    let refused = Transaction::begin(
        &applier,
        NS,
        Mode::ReadOnly,
        above,
        0,
        TxnLimits::default(),
        TxnBuffers::new(),
    );
    w.errors = refused.err().into_iter().collect();
    // The next request on the same worker.
    let mut next = begin(&applier, Mode::ReadOnly)?;
    w.read = objects(&mut next, "a", "name", 0).context("the next request is served")?;
    Ok(())
}

#[then(
    "it is refused at once with an error naming the version and the watermark, and the worker serves the next request"
)]
fn refused_naming_both(w: &mut Steps) {
    let (above, watermark) = (w.versions[0], w.versions[1]);
    assert_eq!(
        w.errors,
        vec![TxnError::read_above_watermark(above, watermark)]
    );
    let text = w.errors[0].to_string();
    assert!(text.contains(&above.to_string()), "{text}");
    assert!(text.contains(&watermark.to_string()), "{text}");
    assert_eq!(w.read, vec![Term::from(Literal::new_simple_literal("A"))]);
}

#[when(
    "a commit setting `<a> <name> \"A\"` is acknowledged and a read-write transaction then begins and reads `name` of `a`"
)]
fn sees_acknowledged(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert_literal("a", "name", "A")])?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    w.read = objects(&mut txn, "a", "name", 0).context("the read")?;
    Ok(())
}

#[then("it reads `\"A\"`")]
fn reads_a(w: &mut Steps) {
    assert_eq!(w.read, vec![Term::from(Literal::new_simple_literal("A"))]);
}

#[when(
    "a read-only transaction reads `name` of `a`, another transaction then commits a new `name` of `a`, and the read-only transaction ends"
)]
fn read_only_ends(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    store(&applier, vec![insert_literal("a", "name", "A")])?;
    let mut txn = begin(&applier, Mode::ReadOnly)?;
    w.read = objects(&mut txn, "a", "name", 0).context("the first read")?;
    store(&applier, vec![insert_literal("a", "name", "B")])?;
    w.later = objects(&mut txn, "a", "name", SECOND).context("the read after the commit")?;
    let committed = txn.commit(0, 0, &[], 2 * SECOND);
    assert!(
        !matches!(committed, Ok(Commit::ReadWrite(_))),
        "a read-only transaction goes to the commit step"
    );
    match committed {
        Ok(_) => w.ended = true,
        Err(e) => w.errors.push(e),
    }
    Ok(())
}

#[then("it ends without an error, having read the value from before the commit")]
fn ended_without_error(w: &mut Steps) {
    assert!(w.ended, "{:?}", w.errors);
    let before = vec![Term::from(Literal::new_simple_literal("A"))];
    assert_eq!(w.read, before);
    assert_eq!(w.later, before);
}

#[when("a transaction inserts `<a> <knows> <b>` and then reads the objects of `knows` of `a`")]
fn read_own_write(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .context("the insert")?;
    w.read = objects(&mut txn, "a", "knows", 0).context("the read")?;
    Ok(())
}

#[then("the read holds `b`")]
fn holds_b(w: &mut Steps) {
    assert_eq!(w.read, vec![Term::from(ex("b"))]);
}

#[when("a read-write transaction begins, waits 11 s, and commits")]
fn waits_eleven(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .context("the insert")?;
    if let Err(e) = txn.commit(1, 1, &[], 11 * SECOND) {
        w.errors.push(e);
    }
    w.later = held(&applier, "a", "knows")?;
    Ok(())
}

#[then(
    "the commit fails with a retryable error naming the 10 s limit, and nothing it wrote is held"
)]
fn past_ten(w: &mut Steps) {
    assert_eq!(w.errors, vec![TxnError::past_limit(10_000)]);
    assert!(w.errors[0].is_retryable());
    assert!(w.errors[0].to_string().contains("10 s"), "{}", w.errors[0]);
    assert!(matches!(w.errors[0].0, TxnErrorEnum::PastLimit { .. }));
    assert!(w.later.is_empty());
}

#[when("a transaction inserts `<a> <knows> <b>` in one request and aborts in the next")]
fn inserts_and_aborts(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .context("the insert")?;
    let _buffers = txn.abort();
    w.later = held(&applier, "a", "knows")?;
    Ok(())
}

#[then("`<a> <knows> <b>` is not held")]
fn not_held(w: &mut Steps) {
    assert!(w.later.is_empty(), "{:?}", w.later);
}

/// The changes the scenario writes, each kind of term in its place, the language tag as a client may send it.
fn varied() -> Changeset {
    let graph = ex("g");
    let blank = BlankNode::new_unchecked("b1");
    changeset(vec![
        insert("a", "knows", "b"),
        insert_literal("a", "name", "plain"),
        Change::Insert {
            graph: GraphName::NamedNode(graph),
            subject: blank.into(),
            predicate: ex("age"),
            object: Literal::new_typed_literal("42", xsd::INTEGER).into(),
        },
        Change::Insert {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("label"),
            object: Literal::new_language_tagged_literal_unchecked("hallo", "de-de").into(),
        },
        Change::InsertEdge {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("likes"),
            object: ex("c").into(),
            properties: vec![(ex("since"), Literal::new_simple_literal("2026").into())],
        },
        Change::Delete {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("name"),
            object: Literal::new_simple_literal("plain").into(),
        },
    ])
}

/// The Write frame holding what [`varied`] holds.
fn varied_frame() -> TestResult<Vec<u8>> {
    let mut table = TermTable::new();
    let iri = |terms: &mut TermTable, name: &str| terms.iri(ex(name).as_str());
    let (a, knows, b) = (
        iri(&mut table, "a"),
        iri(&mut table, "knows"),
        iri(&mut table, "b"),
    );
    let name = iri(&mut table, "name");
    let plain = table.intern(RequestTerm::Literal {
        lexical: "plain".to_string(),
        datatype: None,
    })?;
    let named = iri(&mut table, "g");
    let blank = table.intern(RequestTerm::BlankNode("b1".to_string()))?;
    let age = iri(&mut table, "age");
    let integer = table.intern(RequestTerm::Literal {
        lexical: "42".to_string(),
        datatype: Some(xsd::INTEGER.as_str().to_string()),
    })?;
    let label = iri(&mut table, "label");
    let hallo = table.intern(RequestTerm::LanguageLiteral {
        lexical: "hallo".to_string(),
        language: "DE-de".to_string(),
        direction: None,
    })?;
    let (likes, c, since) = (
        iri(&mut table, "likes"),
        iri(&mut table, "c"),
        iri(&mut table, "since"),
    );
    let year = table.intern(RequestTerm::Literal {
        lexical: "2026".to_string(),
        datatype: None,
    })?;
    let insert = |in_graph, subject, predicate, object| WriteChange::Insert {
        graph: in_graph,
        subject,
        predicate,
        object,
    };
    let write = Write::new(
        table,
        vec![
            insert(None, a, knows, b),
            insert(None, a, name, plain),
            insert(Some(named), blank, age, integer),
            insert(None, a, label, hallo),
            WriteChange::InsertEdge {
                graph: None,
                subject: a,
                predicate: likes,
                object: c,
                properties: vec![(since, year)],
            },
            WriteChange::Delete {
                graph: None,
                subject: a,
                predicate: name,
                object: plain,
            },
        ],
    );
    Ok(write.frame(Header::new(CLUSTER, Command::Write).with_stream(1))?)
}

/// The record a transaction of its own commits, having written what `write` writes.
fn record_of(write: impl FnOnce(&mut Txn<'_>) -> TestResult) -> TestResult<Vec<u8>> {
    let applier = applier()?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    write(&mut txn)?;
    let Commit::ReadWrite(prepared) = txn.commit(1, 1, &[], 0)? else {
        return Err(TestError::new("a write commits read-write"));
    };
    Ok(prepared.record().to_vec())
}

#[when("the same changes are applied once from a Write frame and once as a `Changeset`")]
fn both_paths(w: &mut Steps) -> TestResult {
    w.records
        .push(record_of(|txn| Ok(txn.write(&varied(), 0)?))?);
    let frame = varied_frame()?;
    w.records.push(record_of(|txn| {
        let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
        let write = WriteView::from_frame(&view)?;
        let terms = FrameTerms::of(write.terms())?;
        Ok(txn.write(&FrameChanges::new(NS, &write, &terms)?, 0)?)
    })?);
    Ok(())
}

#[then("the two write-set records are byte-identical")]
fn byte_identical(w: &mut Steps) {
    assert_eq!(w.records.len(), 2);
    assert!(!w.records[0].is_empty());
    assert_eq!(w.records[0], w.records[1]);
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
