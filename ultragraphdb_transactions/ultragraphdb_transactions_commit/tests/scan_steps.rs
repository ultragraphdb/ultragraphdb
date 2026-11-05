/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read through compute at the transaction's level: a transaction scanning as a Scan naming it does, its spans decided by the
//! commit step against real writes.

use std::num::NonZeroUsize;
use std::ops::ControlFlow;

use cucumber::{then, when};
use oxrdf::{GraphName, Literal, NamedNode};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey};
use ultragraphdb_core_mapping::{Change, StatementRef};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{ConflictKind, ErrorCode};
use ultragraphdb_store_quads::{KernelRow, KernelScan, PageEnd, id_key};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_commit::{CommitError, Reply, RoundBounds};
use ultragraphdb_transactions_txn::{Mode, Transaction, TxnBuffers, TxnError, TxnLimits};

use crate::Steps;
use crate::utils::fixture::{
    Applier, Driver, TENANT, Txn, applier, begin, delete_literal, ex, id_of, insert,
    insert_literal, prepare, setup, write,
};

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

/// The ID `txn` reads `<name>` by.
fn id(txn: &mut Txn<'_>, name: &str) -> TestResult<SystemId> {
    txn.resolve_iri(ex(name).as_ref(), 0)?
        .context("the IRI resolves")
}

fn forward_key(subject: SystemId, predicate: SystemId) -> Vec<u8> {
    QuadKey::Forward {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate,
        subject,
    }
    .to_bytes()
}

fn reverse_key(predicate: SystemId, object: SystemId) -> Vec<u8> {
    QuadKey::Reverse {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate,
        object: EncodedTerm::id(object),
    }
    .to_bytes()
}

/// The key just past `key`, so a range of it holds it alone.
fn past(key: &[u8]) -> Vec<u8> {
    let mut past = key.to_vec();
    past.push(0);
    past
}

/// What `txn` scans of `keyspace` from `from` to `to`, at most `limit` lists, and where a next scan begins.
fn scan(
    txn: &mut Txn<'_>,
    keyspace: Keyspace,
    range: (&[u8], &[u8]),
    limit: usize,
) -> TestResult<(Vec<KernelRow>, Option<Vec<u8>>)> {
    let limit = NonZeroUsize::new(limit).context("a limit")?;
    let request = KernelScan::new(
        keyspace,
        TENANT,
        range.0.to_vec(),
        txn.read_version(),
        limit,
    )
    .to(range.1.to_vec());
    let mut rows = Vec::new();
    let next = txn.scan_into(&request, 0, &mut rows)?;
    Ok((rows, next))
}

/// The objects of the one list of `key` that `txn` reads.
fn objects(txn: &mut Txn<'_>, key: &[u8]) -> TestResult<Vec<EncodedTerm>> {
    let (rows, next) = scan(txn, Keyspace::Forward, (key, &past(key)), 8)?;
    assert_eq!(next, None, "one list ends the range");
    Ok(rows
        .iter()
        .flat_map(|row| row.objects().iter().map(|(object, _)| object.clone()))
        .collect())
}

/// `reader` commits a write after `writer` commits `change`.
fn decided(
    applier: &Applier,
    mut reader: Txn<'_>,
    change: ultragraphdb_core_mapping::Change,
) -> TestResult<Vec<Reply>> {
    let mut driver = Driver::new(applier, unbounded());
    let mut writer = begin(applier)?;
    write(&mut writer, vec![change])?;
    write(&mut reader, vec![insert("x", "seen", "y")])?;
    driver.submit(1, prepare(writer)?)?;
    driver.submit(2, prepare(reader)?)?;
    driver.sync()?;
    driver.sync()?;
    Ok(driver.replies.clone())
}

fn second_is(replies: &[Reply], kind: ConflictKind) -> TestResult {
    assert!(
        replies.first().context("T2's reply")?.outcome().is_ok(),
        "T2 commits"
    );
    assert_eq!(
        replies.get(1).context("T1's reply")?.outcome(),
        &Err(CommitError::conflict(kind))
    );
    Ok(())
}

#[when(
    "a transaction writes `<a> <knows> <b>`, and the same transaction reads `<a> <knows> ?o` before committing"
)]
fn reads_own_write(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _set_up = setup(
        &applier,
        vec![insert("a", "knows", "c"), insert("b", "knows", "c")],
    )?;
    let mut txn = begin(&applier)?;
    let (a, knows, b) = (
        id(&mut txn, "a")?,
        id(&mut txn, "knows")?,
        id(&mut txn, "b")?,
    );
    write(&mut txn, vec![insert("a", "knows", "b")])?;
    let key = forward_key(a, knows);
    w.objects = vec![objects(&mut txn, &key)?];
    let mut other = begin(&applier)?;
    w.objects.push(objects(&mut other, &key)?);
    w.ids = vec![b];
    Ok(())
}

#[then("the read returns `<b>`, and a read by any other transaction does not")]
fn saw_own_write(w: &mut Steps) -> TestResult {
    let b = EncodedTerm::id(*w.ids.first().context("<b>")?);
    let own = w.objects.first().context("the transaction's read")?;
    let other = w.objects.get(1).context("the other's read")?;
    assert!(own.contains(&b), "{own:?}");
    assert!(!other.contains(&b), "{other:?}");
    assert_eq!(own.len(), other.len() + 1, "the stored object beneath it");
    Ok(())
}

#[when(
    "a transaction writes `<new> <p> \"x\"`, creating `<new>`, removes every object of `<a> <p> ?o`, and reads both"
)]
fn new_and_emptied(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _set_up = setup(&applier, vec![insert_literal("a", "p", "gone")])?;
    let mut txn = begin(&applier)?;
    let (a, p) = (id(&mut txn, "a")?, id(&mut txn, "p")?);
    write(
        &mut txn,
        vec![
            insert_literal("new", "p", "x"),
            delete_literal("a", "p", "gone"),
        ],
    )?;
    let new = id(&mut txn, "new")?;
    w.objects = vec![
        objects(&mut txn, &forward_key(new, p))?,
        objects(&mut txn, &forward_key(a, p))?,
    ];
    let key = id_key(TENANT, new);
    let mut text = Vec::new();
    txn.dictionary_into(&[key.as_slice()], 0, &mut text)?;
    w.names = text
        .into_iter()
        .flatten()
        .map(String::from_utf8)
        .collect::<Result<_, _>>()?;
    Ok(())
}

#[then(
    "the read returns `\"x\"` for `<new>` with `<new>` named by its text, and nothing for `<a>`"
)]
fn found_new_not_emptied(w: &mut Steps) -> TestResult {
    let new = w.objects.first().context("<new>'s read")?;
    assert_eq!(new.len(), 1, "{new:?}");
    assert_eq!(w.objects.get(1).context("<a>'s read")?, &Vec::new());
    assert_eq!(w.names, vec![ex("new").as_str().to_string()]);
    Ok(())
}

#[when("a transaction scans 10,000 lists through compute")]
fn ten_thousand(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let changes = (0..10_000)
        .map(|n| insert(&format!("s{n}"), "p", "o"))
        .collect();
    let _set_up = setup(&applier, changes)?;
    let mut txn = begin(&applier)?;
    let p = id_of(&applier, "p")?;
    let (mut from, mut rows) = (Vec::new(), 0);
    let limit = NonZeroUsize::new(1_000).context("a limit")?;
    loop {
        let request = KernelScan::new(Keyspace::Forward, TENANT, from, txn.read_version(), limit);
        let mut page = Vec::new();
        let next = txn.scan_into(&request, 0, &mut page)?;
        rows += page.len();
        let Some(resume) = next else {
            break;
        };
        from = resume;
    }
    w.versions = vec![rows as u64, txn.held_lists() as u64];
    write(&mut txn, vec![insert("x", "seen", "y")])?;
    let prepared = prepare(txn)?;
    let (executed, _buffers) = prepared.into_parts();
    let reads = executed.reads();
    let scanned = |key: &[u8]| {
        QuadKey::parse(key).is_ok_and(
            |(parsed, _)| matches!(parsed, QuadKey::Forward { predicate, .. } if predicate == p),
        )
    };
    w.checks = vec![
        reads.ranges().len() == 1,
        !reads.lists().iter().any(|key| scanned(key)),
    ];
    Ok(())
}

#[then(
    "its read set holds one range and no list key, and it holds none of those lists in memory afterwards"
)]
fn holds_nothing(w: &mut Steps) {
    assert_eq!(w.versions, vec![10_000, 0]);
    assert_eq!(w.checks, vec![true, true]);
}

/// `txn` reads the lists of `<a>`'s and `<z>`'s `<knows>` a frame of one object at a time, as a Scan's frames do: every object.
fn read_in_frames(txn: &mut Txn<'_>, end: &[u8], from: Vec<u8>) -> TestResult<usize> {
    let (mut from, mut after, mut objects) = (from, None, 0);
    let limit = NonZeroUsize::MAX;
    loop {
        let request = KernelScan::new(Keyspace::Forward, TENANT, from, txn.read_version(), limit)
            .to(end.to_vec())
            .resuming(after.take());
        let mut taken = 0;
        let page = txn.page(&request, 0, |_, _, _| {
            if taken == 1 {
                return ControlFlow::Break(());
            }
            taken += 1;
            ControlFlow::Continue(())
        })?;
        objects += taken;
        match page.into_end() {
            PageEnd::Exhausted => return Ok(objects),
            PageEnd::Stopped { .. } | PageEnd::Paused { last: None, .. } => {
                return Err(TestError::new("the frame took one object"));
            }
            PageEnd::Yielded { from: next } => from = next,
            PageEnd::Paused {
                from: next,
                after: inside,
                ..
            } => {
                from = next;
                after = inside;
            }
        }
    }
}

#[when(
    "T1 reads a range of two subjects' lists through compute one object a frame, T2 commits an insert into a list it read, and T1 commits a write"
)]
fn read_across_frames(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _set_up = setup(
        &applier,
        vec![
            insert("a", "knows", "o"),
            insert("a", "knows", "p"),
            insert("z", "knows", "o"),
            insert("z", "knows", "p"),
        ],
    )?;
    let (a, z, knows) = (
        id_of(&applier, "a")?,
        id_of(&applier, "z")?,
        id_of(&applier, "knows")?,
    );
    let (low, high) = {
        let (x, y) = (forward_key(a, knows), forward_key(z, knows));
        if x < y { (x, y) } else { (y, x) }
    };
    let end = past(&high);

    let mut recording = begin(&applier)?;
    let objects = read_in_frames(&mut recording, &end, low.clone())?;
    write(&mut recording, vec![insert("x", "seen", "y")])?;
    let (executed, _buffers) = prepare(recording)?.into_parts();
    let reads = executed.reads();
    let scanned = |key: &[u8]| {
        QuadKey::parse(key).is_ok_and(|(parsed, _)| {
            matches!(parsed, QuadKey::Forward { predicate, .. } if predicate == knows)
        })
    };
    w.versions = vec![objects as u64];
    w.checks = vec![
        reads.ranges().len() == 1,
        !reads.lists().iter().any(|key| scanned(key)),
    ];

    let mut reader = begin(&applier)?;
    let _read = read_in_frames(&mut reader, &end, low)?;
    let inside = subject_of(&applier, &high)?;
    w.replies = decided(&applier, reader, insert(inside, "knows", "new"))?;
    Ok(())
}

#[then("its read set holds one range and no list key, and T1 aborts with a conflict of kind Range")]
fn one_range_conflicts(w: &mut Steps) -> TestResult {
    assert_eq!(w.versions, vec![4], "every object, a frame each");
    assert_eq!(w.checks, vec![true, true]);
    second_is(&w.replies, ConflictKind::Range)
}

#[when(
    "transaction T1 reads `<nobody> <p> ?o` through compute, finding nothing, transaction T2 commits `<nobody> <p> \"x\"`, and T1 commits a write"
)]
fn nobody(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _set_up = setup(&applier, vec![insert_literal("a", "p", "held")])?;
    let mut reader = begin(&applier)?;
    let (nobody, p) = (id(&mut reader, "nobody")?, id(&mut reader, "p")?);
    assert_eq!(objects(&mut reader, &forward_key(nobody, p))?, Vec::new());
    w.replies = decided(&applier, reader, insert_literal("nobody", "p", "x"))?;
    Ok(())
}

#[then("T1 aborts with a conflict of kind AbsentTerm")]
fn absent_term(w: &mut Steps) -> TestResult {
    second_is(&w.replies, ConflictKind::AbsentTerm)
}

#[when("a transaction resolves an unheld skolem IRI")]
fn unheld_skolem(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let mut txn = begin(&applier)?;
    let skolem = NamedNode::new(format!(
        "{}.well-known/genid/000000-{:026x}",
        ultragraphdb_store_quads::tenant_base(TENANT),
        1
    ))?;
    w.checks = vec![txn.resolve_iri(skolem.as_ref(), 0)?.is_none()];
    write(&mut txn, vec![insert("x", "seen", "y")])?;
    let (executed, _buffers) = prepare(txn)?.into_parts();
    let created = executed.reads().absent().len();
    w.versions = vec![created as u64];
    // The write's own new terms are absent; the skolem IRI is not among them.
    w.expected_ids = executed.terms().written().map(|(_, id)| id).collect();
    Ok(())
}

#[then("it answers no ID, and its read set records no absent term")]
fn recorded_nothing(w: &mut Steps) {
    assert_eq!(w.checks, vec![true]);
    assert_eq!(
        w.versions,
        vec![w.expected_ids.len() as u64],
        "only the write's own terms"
    );
}

#[when(
    "transaction T1 reads `<a> <knows> ?o` through compute, finding nothing, transaction T2 commits `<a> <knows> <b>`, and T1 commits a write"
)]
fn forward_range(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _set_up = setup(
        &applier,
        vec![insert("a", "other", "b"), insert("c", "knows", "b")],
    )?;
    let mut reader = begin(&applier)?;
    let (a, knows) = (id(&mut reader, "a")?, id(&mut reader, "knows")?);
    assert_eq!(objects(&mut reader, &forward_key(a, knows))?, Vec::new());
    w.replies = decided(&applier, reader, insert("a", "knows", "b"))?;
    Ok(())
}

#[then("T1 aborts with a conflict of kind Range")]
fn range_conflict(w: &mut Steps) -> TestResult {
    second_is(&w.replies, ConflictKind::Range)
}

#[when(
    "transaction T1 reads `?s <knows> <b>` through compute, transaction T2 commits `<c> <knows> <b>` in the same partition, and T1 commits a write"
)]
fn reverse_range(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _set_up = setup(
        &applier,
        vec![insert("a", "knows", "b"), insert("c", "other", "d")],
    )?;
    let mut reader = begin(&applier)?;
    let (knows, b) = (id(&mut reader, "knows")?, id(&mut reader, "b")?);
    let key = reverse_key(knows, b);
    let (rows, next) = scan(&mut reader, Keyspace::Reverse, (&key, &past(&key)), 8)?;
    assert_eq!((rows.len(), next), (1, None), "<a> holds it");
    w.replies = decided(&applier, reader, insert("c", "knows", "b"))?;
    Ok(())
}

/// T1 having read the first of two subjects' lists with limit 1, and the range's keys.
struct Stopped<'a> {
    reader: Txn<'a>,
    resume: Vec<u8>,
    high: Vec<u8>,
    end: Vec<u8>,
}

/// Two subjects' lists of `<knows>` in key order, and T1 having read the first with limit 1.
fn stopped(applier: &Applier) -> TestResult<Stopped<'_>> {
    let _set_up = setup(
        applier,
        vec![insert("a", "knows", "o"), insert("z", "knows", "o")],
    )?;
    let mut reader = begin(applier)?;
    let (a, z, knows) = (
        id(&mut reader, "a")?,
        id(&mut reader, "z")?,
        id(&mut reader, "knows")?,
    );
    let (low, high) = {
        let (x, y) = (forward_key(a, knows), forward_key(z, knows));
        if x < y { (x, y) } else { (y, x) }
    };
    let end = past(&high);
    let (rows, next) = scan(&mut reader, Keyspace::Forward, (&low, &end), 1)?;
    assert_eq!(rows.len(), 1);
    let resume = next.context("the limit stopped it")?;
    assert_eq!(resume, past(&low), "just past what it returned");
    Ok(Stopped {
        reader,
        resume,
        high,
        end,
    })
}

/// The subject of `key`, a forward key of `<a>` or `<z>`, by name.
fn subject_of(applier: &Applier, key: &[u8]) -> TestResult<&'static str> {
    let a = id_of(applier, "a")?;
    match QuadKey::parse(key)? {
        (QuadKey::Forward { subject, .. }, _) if subject == a => Ok("a"),
        (QuadKey::Forward { .. }, _) => Ok("z"),
        (other, _) => Err(TestError::new(format!("not a forward key: {other:?}"))),
    }
}

#[when(
    "T1 reads with limit 1 a range holding `<a>` and `<z>`, T2 commits an insert past `<a>` that T1's read did not reach, and T1 commits without resuming"
)]
fn limit_only(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let Stopped { reader, high, .. } = stopped(&applier)?;
    let past_it = subject_of(&applier, &high)?;
    w.replies = decided(&applier, reader, insert(past_it, "knows", "new"))?;
    Ok(())
}

#[then("T1 commits")]
fn t1_commits(w: &mut Steps) -> TestResult {
    let second = w.replies.get(1).context("T1's reply")?;
    assert!(second.outcome().is_ok(), "{second:?}");
    Ok(())
}

#[when(
    "T1 reads with limit 1, resumes to the range's end, T2 commits an insert into the resumed part, and T1 commits a write"
)]
fn resumed(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let Stopped {
        mut reader,
        resume,
        high,
        end,
    } = stopped(&applier)?;
    let (rows, next) = scan(&mut reader, Keyspace::Forward, (&resume, &end), 1)?;
    assert_eq!((rows.len(), next), (1, None));
    let past_it = subject_of(&applier, &high)?;
    w.replies = decided(&applier, reader, insert(past_it, "knows", "new"))?;
    Ok(())
}

#[when("a transaction's reads leave no room for a span")]
fn no_room(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _set_up = setup(&applier, vec![insert("a", "knows", "b")])?;
    let held = {
        let mut txn = begin(&applier)?;
        forward_key(id(&mut txn, "a")?, id(&mut txn, "knows")?)
    };
    let limits = TxnLimits::default().with_max_read_set(held.len());
    let mut txn: Txn<'_> = Transaction::begin(
        &applier,
        TENANT,
        Mode::ReadWrite,
        0,
        0,
        limits,
        TxnBuffers::new(),
    )?;
    let request = KernelScan::new(
        Keyspace::Forward,
        TENANT,
        held.clone(),
        txn.read_version(),
        NonZeroUsize::MIN,
    )
    .to(past(&held));
    let mut rows = Vec::new();
    let refused = txn
        .scan_into(&request, 0, &mut rows)
        .err()
        .context("no room")?;
    w.refusal = Some(refused);
    w.versions = vec![rows.len() as u64];
    w.refusal_after = txn.scan_into(&request, 0, &mut rows).err();
    Ok(())
}

#[then(
    "the read is refused with `TooLarge`, no entry of that span reaches the client, and the transaction's next request is refused"
)]
fn too_large(w: &mut Steps) -> TestResult {
    let refused = w.refusal.as_ref().context("a refusal")?;
    assert_eq!(refused.code(), ErrorCode::TooLarge, "{refused}");
    assert_eq!(w.versions, vec![0]);
    let after: &TxnError = w.refusal_after.as_ref().context("the next refusal")?;
    assert_eq!(after, &TxnError::ended());
    Ok(())
}

/// The reifier naming the one statement ID of `<subject> <knows> ?o`, and the key of its record.
fn statement_of(applier: &Applier, subject: &str) -> TestResult<(NamedNode, Vec<u8>)> {
    let key = forward_key(id_of(applier, subject)?, id_of(applier, "knows")?);
    let held = applier
        .quads()
        .objects(&key, applier.quads().lists().applied_ts())?;
    let statement = *held
        .first()
        .and_then(|(_, ids)| ids.first())
        .context("a statement ID")?;
    let reifier = NamedNode::new(applier.quads().iri_of(TENANT, statement)?.0)?;
    let record = QuadKey::Statement {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        statement,
    }
    .to_bytes();
    Ok((reifier, record))
}

/// The commit of a write by T1, having read the record of `<a> <knows> <b>`, after T2 sets a property through the reifier of
/// `<written> <knows> <b>`: T2's reply, then T1's.
fn record_read_against(written: &str) -> TestResult<Vec<Reply>> {
    let applier = applier()?;
    let edge = |subject: &str| Change::InsertEdge {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex("knows"),
        object: ex("b").into(),
        properties: vec![(ex("since"), Literal::new_simple_literal("2020").into())],
    };
    let _set_up = setup(&applier, vec![edge("a"), edge("c")])?;
    let (_, record) = statement_of(&applier, "a")?;
    let (reifier, _) = statement_of(&applier, written)?;
    let mut reader = begin(&applier)?;
    let scan = KernelScan::new(
        Keyspace::Statements,
        TENANT,
        record.clone(),
        reader.read_version(),
        NonZeroUsize::MIN,
    )
    .to(past(&record));
    let mut read = 0;
    let _page = reader.values_page(&scan, 0, |_, _| {
        read += 1;
        ControlFlow::Continue(())
    })?;
    assert_eq!(read, 1, "the statement's record");
    let setting = Change::SetProperty {
        statement: StatementRef::Reifier {
            graph: GraphName::DefaultGraph,
            reifier,
        },
        predicate: ex("until"),
        value: Literal::new_simple_literal("2030").into(),
    };
    decided(&applier, reader, setting)
}

#[when(
    "T1 reads the properties of the reifier `r` through compute, T2 sets a property through `r` and commits, and T1 commits a write"
)]
fn record_read_and_written(w: &mut Steps) -> TestResult {
    w.replies = record_read_against("a")?;
    // A record the read's span does not hold leaves its commit alone: the conflict is the span's, no wider.
    let another = record_read_against("c")?;
    assert!(
        another.get(1).context("T1's reply")?.outcome().is_ok(),
        "{another:?}"
    );
    Ok(())
}

#[when(
    "a transaction reads `<new> <p> ?o` through compute, finding nothing, so it records both the term's absence and the list's range, and a transaction writing `<new> <p> \"x\"` commits after its read version"
)]
fn absent_and_range(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _set_up = setup(&applier, vec![insert_literal("a", "p", "held")])?;
    let mut reader = begin(&applier)?;
    let (new, p) = (id(&mut reader, "new")?, id(&mut reader, "p")?);
    assert_eq!(objects(&mut reader, &forward_key(new, p))?, Vec::new());
    assert_eq!(reader.scanned_ranges().len(), 1, "the list's range");
    w.replies = decided(&applier, reader, insert_literal("new", "p", "x"))?;
    Ok(())
}

#[then("the reading transaction aborts with a conflict of kind AbsentTerm")]
fn reader_absent_term(w: &mut Steps) -> TestResult {
    second_is(&w.replies, ConflictKind::AbsentTerm)
}
