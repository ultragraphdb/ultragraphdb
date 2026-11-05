/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A transaction's reads of the statements and the bound on one object's statement IDs: its own records over the stored bytes, its
//! span in the read set, the refusal at the write, and the arithmetic of `PROTOCOL.md` section 11.9 against a frame at the floor.

use std::num::NonZeroUsize;
use std::ops::ControlFlow;

use oxrdf::{GraphName, Literal, NamedNode};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey};
use ultragraphdb_core_mapping::{Change, StatementRef};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Entries, EntriesEnd, EntriesView, ErrorCode, FrameView, Header,
    MAX_FRAME_FLOOR, ObjectRecords, ObjectRecordsBuilder, WriteSetRecordView,
};
use ultragraphdb_store_quads::{KernelScan, PageEnd, STATEMENTS_PER_OBJECT_MAX, StatementRecord};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_txn::{Mode, TxnError, TxnLimits};

use crate::utils::fixture::{
    Applier, SECOND, TENANT, Txn, applier, begin, begin_with, both_postings, changeset, ex,
    held_under, parallel_edge, prepared, store,
};

const CLUSTER: u64 = 1;

/// The bytes a section 11.9 record of `encoding` under the bound's IDs takes, and its frame of one entry under `key`, built and
/// read back: its value's object record.
fn at_the_floor(key: &[u8], encoding: &[u8]) -> TestResult<usize> {
    let id = SystemId::new(TypePrefix::STATEMENT, Bucket::new(1)?, Sequence::new(1)?);
    let ids = vec![id.to_be_bytes(); STATEMENTS_PER_OBJECT_MAX];
    let mut value = ObjectRecordsBuilder::new();
    value.push(encoding, &ids);
    let mut entries = Entries::new(0, EntriesEnd::Ended, 7);
    assert!(
        entries.frame_size_with(key.len(), value.value().len()) <= MAX_FRAME_FLOOR,
        "the record leaves room for its key and the frame's own bytes"
    );
    entries.push(key, value.value());
    let frame = entries.frame(Header::new(CLUSTER, Command::Entries).with_stream(1))?;
    assert!(frame.len() <= MAX_FRAME_FLOOR, "{}", frame.len());
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_FLOOR)?;
    let read = EntriesView::from_frame(&view)?;
    let entry = read.entries().iter().next().context("the entry")?;
    assert_eq!(entry.key(), key);
    let records = ObjectRecords::read(entry.value().context("a value")?)?;
    let record = records.iter().next().context("the object record")?;
    assert_eq!(record.encoding(), encoding);
    assert_eq!(record.statement_count(), STATEMENTS_PER_OBJECT_MAX);
    Ok(value.value().len())
}

/// `PROTOCOL.md` section 11.9: the bound's IDs with a vector of 65,535 dimensions make a record of 518,151 bytes, inside a frame at
/// the floor of `max_frame` under a forward key, and with a reverse list's system ID a far smaller one.
#[test]
fn test_an_object_at_its_bound_fits_a_frame_at_the_floor_in_either_list() -> TestResult {
    let id = SystemId::new(TypePrefix::RESOURCE, Bucket::new(1)?, Sequence::new(1)?);
    let vector = [
        // A vector's tag (`PROTOCOL.md` section 10.1).
        vec![0x40],
        u16::MAX.to_le_bytes().to_vec(),
        vec![0; usize::from(u16::MAX) * 4],
    ]
    .concat();
    assert!(EncodedTerm::from_bytes(&vector)?.is_vector());
    assert_eq!(vector.len(), 262_143);
    let forward = QuadKey::Forward {
        tenant: TENANT,
        graph: Graph::Named(id),
        predicate: id,
        subject: id,
    }
    .to_bytes();
    assert_eq!(at_the_floor(&forward, &vector)?, 518_151);
    let subject = EncodedTerm::id(id);
    let reverse = QuadKey::Reverse {
        tenant: TENANT,
        graph: Graph::Named(id),
        predicate: id,
        object: subject.clone(),
    }
    .to_bytes();
    assert!(at_the_floor(&reverse, subject.as_bytes())? < 518_151);
    Ok(())
}

/// The edge reaching the bound commits as a client's does, its record within `max_write_set` and holding its own ID alone, in the
/// write-set limit of a drive with power-loss protection as well; and one past it is refused, the transaction ended, with the code
/// the protocol gives it.
#[test]
fn test_the_edge_past_the_bound_ends_its_transaction() -> TestResult {
    let applier = applier()?;
    let held_ids = held_under(&applier, STATEMENTS_PER_OBJECT_MAX - 1)?;
    let small = TxnLimits::default().with_max_write_set(261_320);
    let mut reaching = begin_with(&applier, Mode::ReadWrite, small)?;
    reaching.write(&changeset(vec![parallel_edge()]), 0)?;
    let proposal = prepared(reaching.commit(0, 0, b"", 0))?;
    let view = WriteSetRecordView::new(proposal.record())?;
    let inserted: Vec<_> = view.inserted().iter().collect();
    assert_eq!(inserted.len(), 1, "the edge's own entry");
    let entry = inserted.first().context("the entry")?;
    let id = SystemId::decode(entry.value().context("its ID")?)?;
    assert!(!held_ids.contains(&id));
    let _buffers = proposal.release(&applier);
    store(&applier, vec![parallel_edge()])?;
    let held = both_postings(&applier)?;
    assert!(
        held.iter()
            .all(|(_, ids)| ids.len() == STATEMENTS_PER_OBJECT_MAX),
        "both postings at the bound"
    );
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let refused = txn
        .write(&changeset(vec![parallel_edge()]), 0)
        .err()
        .context("one more statement")?;
    assert_eq!(refused.code(), ErrorCode::TooLarge);
    assert!(txn.is_ended());
    assert_eq!(both_postings(&applier)?, held);
    Ok(())
}

/// A statement of `<a> <knows> <b>` with `<since> "2020"` and a statement of `<c> <knows> <d>`: their records' keys.
fn statements(applier: &Applier) -> TestResult<Vec<Vec<u8>>> {
    let edge = |subject: &str, object: &str| Change::InsertEdge {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex("knows"),
        object: ex(object).into(),
        properties: vec![(ex("since"), Literal::new_simple_literal("2020").into())],
    };
    store(applier, vec![edge("a", "b"), edge("c", "d")])?;
    let quads = applier.quads();
    let mut keys = Vec::new();
    for subject in ["a", "c"] {
        let id = |name: &str| -> TestResult<SystemId> {
            quads.id_of(TENANT, ex(name).as_str())?.context("an ID")
        };
        let key = QuadKey::Forward {
            tenant: TENANT,
            graph: Graph::Default,
            predicate: id("knows")?,
            subject: id(subject)?,
        }
        .to_bytes();
        let held = quads.objects(&key, quads.lists().applied_ts())?;
        let statement = *held
            .first()
            .and_then(|(_, ids)| ids.first())
            .context("a statement ID")?;
        keys.push(
            QuadKey::Statement {
                tenant: TENANT,
                graph: Graph::Default,
                statement,
            }
            .to_bytes(),
        );
    }
    Ok(keys)
}

fn statement_scan(version: u64, limit: usize) -> TestResult<KernelScan> {
    Ok(KernelScan::new(
        Keyspace::Statements,
        TENANT,
        Vec::new(),
        version,
        NonZeroUsize::new(limit).context("a limit")?,
    ))
}

/// What `txn` reads of the statements, each key's stored bytes.
fn read_all(txn: &mut Txn<'_>) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let scan = statement_scan(txn.read_version(), 16)?;
    let mut read = Vec::new();
    let page = txn.values_page(&scan, 0, |key, value| {
        read.push((key.to_vec(), value.to_vec()));
        ControlFlow::Continue(())
    })?;
    assert_eq!(page.end(), &PageEnd::Exhausted);
    Ok(read)
}

/// A transaction reads a statement's record with its own property over the stored bytes, its span a range scanned.
#[test]
fn test_a_transaction_reads_its_own_property_over_the_stored_record() -> TestResult {
    let applier = applier()?;
    let keys = statements(&applier)?;
    let first = keys.first().context("a first key")?;
    let mut txn = begin(&applier, Mode::ReadWrite)?;
    let before = read_all(&mut txn)?;
    assert_eq!(before.len(), 2);
    let (start, end) = statement_scan(0, 1)?.bounds();
    assert_eq!(txn.scanned_ranges(), vec![(start, end)]);
    let parsed = QuadKey::parse(first)?;
    let (QuadKey::Statement { statement, .. }, _) = parsed else {
        return Err(TestError::new("a statement's key"));
    };
    let reifier = NamedNode::new(applier.quads().iri_of(TENANT, statement)?.0)?;
    txn.write(
        &changeset(vec![Change::SetProperty {
            statement: StatementRef::Reifier {
                graph: GraphName::DefaultGraph,
                reifier,
            },
            predicate: ex("until"),
            value: Literal::new_simple_literal("2030").into(),
        }]),
        0,
    )?;
    let after = read_all(&mut txn)?;
    let mine = after
        .iter()
        .find(|(key, _)| key == first)
        .context("the record")?;
    let record = StatementRecord::decode(first, &mine.1)?;
    assert_eq!(record.properties().len(), 2, "its own over the stored one");
    let other = applier
        .quads()
        .statement(TENANT, Graph::Default, statement, txn.read_version())?
        .context("the stored record")?;
    assert_eq!(other.properties().len(), 1, "nothing another read sees");
    Ok(())
}

/// A span past the read set's room ends the transaction, what the sink took not to be sent; a past limit and an ended transaction
/// read nothing.
#[test]
fn test_a_read_of_the_statements_is_bounded_as_a_list_s_is() -> TestResult {
    let applier = applier()?;
    let _keys = statements(&applier)?;
    let small = TxnLimits::default().with_max_read_set(1);
    let mut txn = begin_with(&applier, Mode::ReadWrite, small)?;
    let scan = statement_scan(txn.read_version(), 16)?;
    let refused = txn
        .values_page(&scan, 0, |_, _| ControlFlow::Continue(()))
        .err()
        .context("no room")?;
    assert!(matches!(
        refused,
        TxnError(ultragraphdb_transactions_txn::TxnErrorEnum::ReadSetTooLarge { .. })
    ));
    assert!(txn.is_ended());
    let late = begin(&applier, Mode::ReadWrite)?
        .values_page(&scan, 3_600 * SECOND, |_, _| ControlFlow::Continue(()))
        .err()
        .context("past its limit")?;
    assert_eq!(late.code(), ErrorCode::PastLimit);
    let mut wrong = begin(&applier, Mode::ReadWrite)?;
    let lists = KernelScan::new(
        Keyspace::Forward,
        TENANT,
        Vec::new(),
        wrong.read_version(),
        NonZeroUsize::MIN,
    );
    assert!(
        wrong
            .values_page(&lists, 0, |_, _| ControlFlow::Continue(()))
            .is_err()
    );
    assert!(wrong.is_ended(), "a refused read ends it");
    Ok(())
}
