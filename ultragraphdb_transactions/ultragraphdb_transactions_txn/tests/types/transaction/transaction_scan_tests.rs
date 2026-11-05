/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Scan naming a transaction, page by page: its pages concatenate to its scan's rows over its own writes, its read set holds
//! one range however many pages it took, each page's span as `PROTOCOL.md` section 13.13 states, and a span with no room ends it.

use std::num::NonZeroUsize;
use std::ops::ControlFlow;

use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey};
use ultragraphdb_core_mapping::{Change, Changeset};
use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_store_quads::{KernelRow, KernelScan, PageEnd};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_txn::{Mode, TxnError, TxnLimits};

use crate::utils::fixture::{
    Applier, NS, SECOND, Txn, applier, begin, begin_with, changeset, ex, insert, store,
};

/// The delete of `<subject> <predicate> <object>`.
fn delete(subject: &str, predicate: &str, object: &str) -> Change {
    Change::Delete {
        graph: oxrdf::GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
        object: ex(object).into(),
    }
}

/// What the transaction writes over [`seeded`]: a list it adds to, a list it empties and a list only it holds. It reads
/// `<held> <knows>`, so it holds that list too.
fn overlay() -> Changeset {
    changeset(vec![
        insert("a", "knows", "b"),
        insert("new", "knows", "a"),
        insert("new", "knows", "b"),
        delete("gone", "knows", "z"),
    ])
}

fn seeded() -> TestResult<Applier> {
    let applier = applier()?;
    store(
        &applier,
        vec![
            insert("a", "knows", "y"),
            insert("a", "knows", "z"),
            insert("held", "knows", "y"),
            insert("gone", "knows", "z"),
            insert("c", "knows", "w"),
            insert("c", "knows", "x"),
            // Every term the overlay names is held, so each transaction reads it by one ID.
            insert("new", "named", "b"),
        ],
    )?;
    Ok(applier)
}

fn overlaid(applier: &Applier) -> TestResult<Txn<'_>> {
    let mut txn = begin(applier, Mode::ReadWrite)?;
    txn.write(&overlay(), 0)?;
    let _held = crate::utils::fixture::objects(&mut txn, "held", "knows", 0)?;
    Ok(txn)
}

fn forward(txn: &Txn<'_>, limit: NonZeroUsize) -> KernelScan {
    KernelScan::new(Keyspace::Forward, NS, Vec::new(), txn.read_version(), limit)
}

/// The transaction's pages of `scan`'s range at `budget` objects a page: the rows, whole, with the span after each page held to
/// section 13.13.
fn paged(
    txn: &mut Txn<'_>,
    scan: &KernelScan,
    budget: usize,
    limit: NonZeroUsize,
) -> TestResult<Vec<KernelRow>> {
    let (start, end) = scan.bounds();
    let past = end.context("a namespace's keyspace ends")?;
    let (mut rows, mut from, mut after, mut left) =
        (Vec::<KernelRow>::new(), start.clone(), None, limit);
    loop {
        let this = KernelScan::new(scan.keyspace(), NS, from.clone(), scan.version(), left)
            .to(past.clone())
            .resuming(after.take())
            .yielding(NonZeroUsize::MIN);
        let mut taken = 0;
        let page = txn.page(&this, 0, |key, object, statements| {
            if taken == budget {
                return ControlFlow::Break(());
            }
            taken += 1;
            let mut objects = match rows.last() {
                Some(row) if row.key() == key => rows
                    .pop()
                    .map_or_else(Vec::new, |popped| popped.objects().to_vec()),
                Some(_) | None => Vec::new(),
            };
            objects.push((object.clone(), statements.to_vec()));
            rows.push(KernelRow::new(key.to_vec(), objects));
            ControlFlow::Continue(())
        })?;
        let reach = match page.end() {
            PageEnd::Exhausted => past.clone(),
            PageEnd::Stopped { .. } | PageEnd::Paused { .. } => {
                [rows.last().context("a row")?.key(), &[0]].concat()
            }
            PageEnd::Yielded { from: next } => next.clone(),
        };
        assert_eq!(
            txn.scanned_ranges(),
            vec![(start.clone(), Some(reach))],
            "one range, at {budget} objects a page"
        );
        let rows_left = left.get() - page.rows();
        match page.into_end() {
            PageEnd::Exhausted => return Ok(rows),
            PageEnd::Stopped { mut last } => {
                last.push(0);
                from = last;
                left = limit;
            }
            PageEnd::Yielded { from: next } => from = next,
            PageEnd::Paused {
                from: next,
                after: inside,
                ..
            } => {
                from = next;
                after = inside;
                left = NonZeroUsize::new(rows_left).context("a pause leaves a row to read")?;
            }
        }
    }
}

#[test]
fn test_a_transaction_s_pages_concatenate_to_its_scan_and_hold_one_range() -> TestResult {
    let applier = seeded()?;
    let expected = {
        let mut txn = overlaid(&applier)?;
        let scan = forward(&txn, NonZeroUsize::MAX);
        let mut rows = Vec::new();
        assert_eq!(txn.scan_into(&scan, 0, &mut rows)?, None);
        rows
    };
    assert_eq!(
        expected.len(),
        5,
        "<a>, <held>, <c>, and <new>'s two: {expected:?}"
    );
    for budget in 1..=4 {
        for limit in [NonZeroUsize::MIN, NonZeroUsize::MAX] {
            let mut txn = overlaid(&applier)?;
            let held = txn.held_lists();
            let scan = forward(&txn, limit);
            let rows = paged(&mut txn, &scan, budget, limit)?;
            assert_eq!(rows, expected, "{budget} objects a page, limit {limit}");
            assert_eq!(txn.held_lists(), held, "nothing newly held");
        }
    }
    Ok(())
}

/// A scan at its limit resumes just past its last row, and the two spans are one range.
#[test]
fn test_a_scan_stopped_at_its_limit_resumes_where_its_span_ended() -> TestResult {
    let applier = seeded()?;
    let mut txn = overlaid(&applier)?;
    let scan = forward(&txn, NonZeroUsize::MIN);
    let mut rows = vec![KernelRow::new(b"before".to_vec(), Vec::new())];
    let resume = txn
        .scan_into(&scan, 0, &mut rows)?
        .context("the limit stopped it")?;
    let last = rows.last().context("a row")?.key();
    assert_eq!(resume, [last, &[0]].concat());
    assert_eq!(rows.len(), 2, "appended to what it held");
    let start = scan.bounds().0;
    assert_eq!(txn.scanned_ranges(), vec![(start, Some(resume))]);
    Ok(())
}

/// The key of `<subject> <knows>`, in the store as `txn` resolves it.
fn knows_key(txn: &mut Txn<'_>, subject: &str) -> TestResult<Vec<u8>> {
    let (s, p) = (ex(subject), ex("knows"));
    Ok(QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: txn.resolve_iri(p.as_ref(), 0)?.context("the predicate")?,
        subject: txn.resolve_iri(s.as_ref(), 0)?.context("the subject")?,
    }
    .to_bytes())
}

/// The scan of `key`'s list alone.
fn of_key(txn: &Txn<'_>, key: &[u8]) -> KernelScan {
    KernelScan::new(
        Keyspace::Forward,
        NS,
        key.to_vec(),
        txn.read_version(),
        NonZeroUsize::MAX,
    )
    .to([key, &[0]].concat())
}

/// A span with no room in the read set ends the transaction, whatever its earlier pages left: the page is refused with
/// `TooLarge`, so what its sink took is not sent, and no later page reads anything or calls its sink.
#[test]
fn test_a_page_past_the_read_set_s_room_ends_the_transaction() -> TestResult {
    let applier = seeded()?;
    // The room one span takes, and a second span's beside it none.
    let room = (0..4_096)
        .find(|limit| {
            let limits = TxnLimits::default().with_max_read_set(*limit);
            begin_with(&applier, Mode::ReadWrite, limits).is_ok_and(|mut txn| {
                knows_key(&mut txn, "a").is_ok_and(|key| {
                    txn.page(&of_key(&txn, &key), 0, |_, _, _| ControlFlow::Continue(()))
                        .is_ok()
                })
            })
        })
        .context("a span fits somewhere")?;
    let limits = TxnLimits::default().with_max_read_set(room);
    let mut txn = begin_with(&applier, Mode::ReadWrite, limits)?;
    let (a, c) = (knows_key(&mut txn, "a")?, knows_key(&mut txn, "c")?);
    let _first = txn.page(&of_key(&txn, &a), 0, |_, _, _| ControlFlow::Continue(()))?;
    let scan = of_key(&txn, &c);
    let mut offered = 0;
    let refused = txn
        .page(&scan, 0, |_, _, _| {
            offered += 1;
            ControlFlow::Continue(())
        })
        .err()
        .context("no room")?;
    assert_eq!(refused.code(), ErrorCode::TooLarge, "{refused}");
    assert!(
        offered > 0,
        "the walk came first, and its frame is not sent"
    );
    assert!(txn.is_ended());
    assert_eq!(
        txn.scanned_ranges(),
        Vec::new(),
        "it ended, recording nothing further"
    );
    let mut again = 0;
    let ended = txn
        .page(&scan, 0, |_, _, _| {
            again += 1;
            ControlFlow::Continue(())
        })
        .err()
        .context("ended")?;
    assert_eq!(ended, TxnError::ended());
    assert_eq!(again, 0);
    Ok(())
}

/// A scan refused for its room appends nothing to what `out` held.
#[test]
fn test_a_scan_past_the_read_set_s_room_appends_nothing() -> TestResult {
    let applier = seeded()?;
    let limits = TxnLimits::default().with_max_read_set(1);
    let mut txn = begin_with(&applier, Mode::ReadOnly, limits)?;
    let scan = forward(&txn, NonZeroUsize::MAX);
    let before = KernelRow::new(b"before".to_vec(), Vec::new());
    let mut rows = vec![before.clone()];
    let refused = txn
        .scan_into(&scan, 0, &mut rows)
        .err()
        .context("no room")?;
    assert_eq!(refused.code(), ErrorCode::TooLarge, "{refused}");
    assert_eq!(rows, vec![before]);
    Ok(())
}

/// A transaction past its limit or holding a keyspace of no lists ends at the page.
#[test]
fn test_a_page_past_the_transaction_s_limit_or_of_no_lists_ends_it() -> TestResult {
    let applier = seeded()?;
    let mut late = begin(&applier, Mode::ReadOnly)?;
    let scan = forward(&late, NonZeroUsize::MAX);
    let past_limit = late
        .page(&scan, 3_600 * SECOND, |_, _, _| ControlFlow::Continue(()))
        .err()
        .context("past its limit")?;
    assert_eq!(past_limit.code(), ErrorCode::PastLimit, "{past_limit}");
    assert!(late.is_ended());

    let mut marks = begin(&applier, Mode::ReadOnly)?;
    let of_marks = KernelScan::new(
        Keyspace::Marks,
        NS,
        Vec::new(),
        marks.read_version(),
        NonZeroUsize::MIN,
    );
    let refused = marks
        .page(&of_marks, 0, |_, _, _| ControlFlow::Continue(()))
        .err()
        .ok_or_else(|| TestError::new("a keyspace of no lists"))?;
    assert!(
        matches!(
            refused.0,
            ultragraphdb_transactions_txn::TxnErrorEnum::Changeset(_)
        ),
        "{refused:?}"
    );
    assert!(marks.is_ended());
    Ok(())
}
