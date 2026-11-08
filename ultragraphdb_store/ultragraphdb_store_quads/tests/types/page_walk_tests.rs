/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The walk of a page of the read kernel: many lists read at small budgets concatenate to exactly what the kernel's scan returns,
//! the limit honoured across pauses inside a key, a list crossing pages read once, and a range of emptied lists taking many.

use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::sync::Arc;

use oxrdf::{Literal, Quad};
use ultragraphdb_core_codec::Posting;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::{DEL, PostingIdentity};
use ultragraphdb_store_quads::{KernelRow, KernelScan, Page, PageEnd, QuadWrite};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::helpers::{
    TestQuads, blocks, commit, ex, lists_over, quad, quads, sim, write_at,
};

const TENANT: u64 = 0;

/// Lists of one, two and five objects, of IRIs and literals alike, and of the version they were written at.
fn store() -> TestResult<(TestQuads, u64)> {
    let store = quads()?;
    let mut held: Vec<Quad> = (0..5)
        .map(|n| quad("hub", "knows", ex(&format!("o{n}"))))
        .collect();
    held.extend((0..2).map(|n| quad("a", "name", Literal::new_simple_literal(format!("v{n}")))));
    held.push(quad("b", "knows", ex("o0")));
    held.push(quad("c", "name", Literal::new_simple_literal("x")));
    let at = write_at(&store, TENANT, &held)?;
    Ok((store, at))
}

/// The forward lists of `TENANT` at `version`, at most `limit`, from `from`.
fn forward(from: Vec<u8>, version: u64, limit: usize) -> TestResult<KernelScan> {
    Ok(KernelScan::new(
        Keyspace::Forward,
        TENANT,
        from,
        version,
        NonZeroUsize::new(limit).context("a limit")?,
    ))
}

/// Everything the kernel's scan returns at `version`.
fn whole(store: &TestQuads, version: u64) -> TestResult<Vec<KernelRow>> {
    let mut rows = Vec::new();
    let resume = store.scan(&forward(Vec::new(), version, usize::MAX)?, &mut rows)?;
    assert_eq!(resume, None);
    Ok(rows)
}

/// One limited scan of the range read page by page at `budget` objects a page: the rows it gives, and where the next scan
/// begins. Each page asks only for the rows the limit leaves.
fn paged_scan(
    store: &TestQuads,
    from: Vec<u8>,
    version: u64,
    limit: usize,
    budget: usize,
) -> TestResult<(Vec<KernelRow>, Option<Vec<u8>>)> {
    let (mut rows, mut from, mut after, mut left) = (Vec::<KernelRow>::new(), from, None, limit);
    // Every page but the last takes an object, so a scan takes at most one page an object and one more.
    let pages_max = whole(store, version)?
        .iter()
        .map(|row| row.objects().len())
        .sum::<usize>()
        + 1;
    for _ in 0..pages_max {
        let scan = forward(from.clone(), version, left)?.resuming(after.take());
        let mut taken = 0;
        let page = store.page(&scan, |key, object, statements| {
            if taken == budget {
                return ControlFlow::Break(());
            }
            taken += 1;
            take(&mut rows, key, object, statements);
            ControlFlow::Continue(())
        })?;
        left -= page.rows();
        match page.into_end() {
            PageEnd::Exhausted => return Ok((rows, None)),
            end @ PageEnd::Stopped { .. } => return Ok((rows, end.resume_from())),
            end @ PageEnd::Yielded { .. } => {
                return Err(TestError::new(format!("no scan here yields: {end:?}")));
            }
            PageEnd::Paused { last: None, .. } => {
                return Err(TestError::new("the budget took an object"));
            }
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
    Err(TestError::new(format!(
        "more than {pages_max} pages: a page took nothing new"
    )))
}

/// `object` taken into `rows`, the last row's when it is of `key`.
fn take(rows: &mut Vec<KernelRow>, key: &[u8], object: &EncodedTerm, statements: &[SystemId]) {
    let object = (object.clone(), statements.to_vec());
    let mut objects = match rows.last() {
        Some(row) if row.key() == key => rows
            .pop()
            .map_or_else(Vec::new, |popped| popped.objects().to_vec()),
        Some(_) | None => Vec::new(),
    };
    objects.push(object);
    rows.push(KernelRow::new(key.to_vec(), objects));
}

#[test]
fn test_pages_at_any_budget_concatenate_to_the_scan() -> TestResult {
    let (store, at) = store()?;
    let expected = whole(&store, at)?;
    assert!(expected.len() > 3, "{expected:?}");
    for budget in 1..=12 {
        let (rows, resume) = paged_scan(&store, Vec::new(), at, usize::MAX, budget)?;
        assert_eq!(rows, expected, "at {budget} objects a page");
        assert_eq!(resume, None);
    }
    Ok(())
}

/// Many lists of one to four objects, at small budgets and limits: every scan's pages concatenate to the scan's rows.
#[test]
fn test_pages_of_many_lists_at_small_budgets_concatenate_to_the_scan() -> TestResult {
    let store = quads()?;
    let held: Vec<Quad> = (0..64)
        .flat_map(|subject| {
            (0..=subject % 4)
                .map(move |object| quad(&format!("s{subject}"), "knows", ex(&format!("o{object}"))))
        })
        .collect();
    let at = write_at(&store, TENANT, &held)?;
    let expected = whole(&store, at)?;
    assert_eq!(expected.len(), 64);
    for budget in [1, 2, 3, 5] {
        for limit in [1, 3, 7, usize::MAX] {
            let (mut rows, mut resume, mut scans) = (Vec::new(), Some(Vec::new()), 0);
            // Ends: each scan gives at least one row of a range holding finitely many.
            while let Some(from) = resume {
                scans += 1;
                if scans > expected.len() {
                    return Err(TestError::new("a scan gave no row"));
                }
                let (scan, next) = paged_scan(&store, from, at, limit, budget)?;
                assert!(scan.len() <= limit);
                rows.extend(scan);
                resume = next;
            }
            assert_eq!(rows, expected, "limit {limit}, {budget} objects a page");
        }
    }
    Ok(())
}

#[test]
fn test_the_limit_counts_a_key_paused_inside_once() -> TestResult {
    let (store, at) = store()?;
    let expected = whole(&store, at)?;
    for limit in 1..=expected.len() {
        for budget in 1..=4 {
            let (mut rows, mut resume, mut scans) = (Vec::new(), Some(Vec::new()), 0);
            // Ends: each scan gives at least one row of a range holding finitely many.
            while let Some(from) = resume {
                scans += 1;
                if scans > expected.len() {
                    return Err(TestError::new("a scan gave no row"));
                }
                let (scan, next) = paged_scan(&store, from, at, limit, budget)?;
                assert!(
                    scan.len() <= limit,
                    "{} rows at limit {limit}, {budget} objects a page",
                    scan.len()
                );
                if next.is_some() {
                    assert_eq!(scan.len(), limit, "stopped at its limit");
                }
                rows.extend(scan);
                resume = next;
            }
            assert_eq!(rows, expected, "limit {limit}, {budget} objects a page");
        }
    }
    Ok(())
}

/// A page's ends: inside a key it names the key and the last object taken, between keys just past the last, and with nothing
/// taken no last key; rows count what it completed.
#[test]
fn test_a_page_says_where_it_paused_and_what_it_completed() -> TestResult {
    let (store, at) = store()?;
    let rows = whole(&store, at)?;
    let longest = rows
        .iter()
        .position(|row| row.objects().len() > 1)
        .context("a list of several objects")?;
    let key_of = |index: usize| -> TestResult<Vec<u8>> {
        Ok(rows.get(index).context("a row")?.key().to_vec())
    };
    let objects_before: usize = rows
        .iter()
        .take(longest)
        .map(|row| row.objects().len())
        .sum();
    let budgeted = |budget: usize| -> TestResult<Page> {
        let mut taken = 0;
        Ok(
            store.page(&forward(Vec::new(), at, usize::MAX)?, |_, _, _| {
                if taken == budget {
                    return ControlFlow::Break(());
                }
                taken += 1;
                ControlFlow::Continue(())
            })?,
        )
    };

    let nothing = budgeted(0)?;
    assert_eq!(nothing.rows(), 0);
    assert_eq!(
        nothing.end(),
        &PageEnd::Paused {
            last: None,
            from: key_of(0)?,
            after: None,
        }
    );

    let inside = budgeted(objects_before + 1)?;
    assert_eq!(inside.rows(), longest);
    let PageEnd::Paused { last, from, after } = inside.end() else {
        return Err(TestError::new("paused"));
    };
    assert_eq!(last.as_deref(), Some(key_of(longest)?.as_slice()));
    assert_eq!(from, &key_of(longest)?);
    assert!(after.is_some(), "it resumes after the object taken");

    let upto = objects_before + rows.get(longest).context("a row")?.objects().len();
    let between = budgeted(upto)?;
    assert_eq!(between.rows(), longest + 1);
    let mut past = key_of(longest)?;
    past.push(0);
    assert_eq!(
        between.end(),
        &PageEnd::Paused {
            last: Some(key_of(longest)?),
            from: past,
            after: None,
        }
    );
    Ok(())
}

/// A sink that takes nothing of the key it resumes inside has given nothing, whatever pages before it gave.
#[test]
fn test_a_resumed_page_taking_nothing_has_no_last_and_keeps_its_place() -> TestResult {
    let (store, at) = store()?;
    let rows = whole(&store, at)?;
    let row = rows
        .iter()
        .find(|row| row.objects().len() > 1)
        .context("a list of several objects")?;
    let after = identity_of(row.objects().first().context("an object")?)?;
    let scan = forward(row.key().to_vec(), at, usize::MAX)?.resuming(Some(after.clone()));
    let page = store.page(&scan, |_, _, _| ControlFlow::Break(()))?;
    assert_eq!(page.rows(), 0);
    assert_eq!(
        page.end(),
        &PageEnd::Paused {
            last: None,
            from: row.key().to_vec(),
            after: Some(after),
        }
    );
    Ok(())
}

/// The identity of an object of a list: a reference's by its ID's 128 bits.
fn identity_of(object: &(EncodedTerm, Vec<SystemId>)) -> TestResult<PostingIdentity> {
    let (term, _) = object;
    match term.as_id() {
        Some(id) => Ok(PostingIdentity::new(id.to_u128(), Vec::new())),
        None => Ok(PostingIdentity::new(
            u128::from(xxhash_rust::xxh3::xxh3_64(term.as_bytes())),
            term.as_bytes().to_vec(),
        )),
    }
}

/// A key the scan resumes inside with nothing left after its posting completes a row with no object: it was a row already, so
/// the limit counts it.
#[test]
fn test_a_resumed_key_with_nothing_left_completes_its_row() -> TestResult {
    let (store, at) = store()?;
    let rows = whole(&store, at)?;
    let row = rows.first().context("a row")?;
    let after = identity_of(row.objects().last().context("an object")?)?;
    let scan = forward(row.key().to_vec(), at, 1)?.resuming(Some(after));
    let mut offered = Vec::new();
    let page = store.page(&scan, |key, _, _| {
        offered.push(key.to_vec());
        ControlFlow::Continue(())
    })?;
    assert_eq!(page.rows(), 1);
    assert_eq!(
        page.end(),
        &PageEnd::Stopped {
            last: row.key().to_vec()
        }
    );
    assert!(offered.is_empty(), "{offered:?}");
    Ok(())
}

/// Two literals of one fingerprint share a posting key: a page paused between them loses neither.
#[test]
fn test_a_page_paused_between_two_literals_of_one_fingerprint_loses_neither() -> TestResult {
    let store = ultragraphdb_store_quads::QuadStore::open(
        lists_over(sim()),
        blocks(),
        ultragraphdb_core_mapping::RandomSource::new(7),
    )?
    .with_fingerprint(|_| 9);
    let at = write_at(
        &store,
        TENANT,
        &[
            quad("a", "name", Literal::new_simple_literal("one")),
            quad("a", "name", Literal::new_simple_literal("two")),
            quad("a", "name", Literal::new_simple_literal("three")),
        ],
    )?;
    let expected = whole(&store, at)?;
    let objects = expected.first().context("a row")?.objects().len();
    assert_eq!(objects, 3);
    let (rows, resume) = paged_scan(&store, Vec::new(), at, usize::MAX, 1)?;
    assert_eq!(rows, expected);
    assert_eq!(resume, None);
    Ok(())
}

/// A list crossing pages is read once: the first pause keeps it, each page after reads from what the one before kept, and the
/// pages still give what the kernel's scan gives.
#[test]
fn test_a_list_crossing_pages_is_read_once() -> TestResult {
    let (store, at) = store()?;
    let rows = whole(&store, at)?;
    let row = rows
        .iter()
        .find(|row| row.objects().len() > 2)
        .context("a list of several objects")?;
    let (key, expected) = (row.key().to_vec(), row.objects().len());
    let past = [key.as_slice(), &[0]].concat();
    let one_object = |scan: &KernelScan, seen: &mut usize| -> TestResult<Page> {
        let mut taken = false;
        Ok(store.page(scan, |_, _, _| {
            if taken {
                return ControlFlow::Break(());
            }
            taken = true;
            *seen += 1;
            ControlFlow::Continue(())
        })?)
    };
    let mut seen = 0;
    let scan = forward(key.clone(), at, usize::MAX)?.to(past.clone());
    let (end, kept) = one_object(&scan, &mut seen)?.into_parts();
    let kept = kept.context("the first pause keeps the list")?;
    let PageEnd::Paused { mut after, .. } = end else {
        return Err(TestError::new("paused inside the list"));
    };
    while seen < expected - 1 {
        let resumed = forward(key.clone(), at, usize::MAX)?
            .to(past.clone())
            .resuming(after.take())
            .holding(Some(Arc::clone(&kept)));
        let (next, held) = one_object(&resumed, &mut seen)?.into_parts();
        let held = held.context("it stays kept")?;
        assert!(Arc::ptr_eq(&held, &kept), "no page read it again");
        let PageEnd::Paused { after: inside, .. } = next else {
            return Err(TestError::new("paused inside the list"));
        };
        after = inside;
    }
    let last = forward(key.clone(), at, usize::MAX)?
        .to(past)
        .resuming(after)
        .holding(Some(Arc::clone(&kept)));
    let (finished, held) = one_object(&last, &mut seen)?.into_parts();
    assert_eq!(finished, PageEnd::Exhausted);
    assert_eq!(held, None, "the list's end lets it go");
    assert_eq!(seen, expected, "every object once");
    Ok(())
}

/// A page handing back after its bound of keys giving nothing says where it goes on and takes no row for that key, and pages at
/// that bound concatenate to the scan's rows, a scan naming no bound reading through the key.
#[test]
fn test_a_page_hands_back_after_its_keys_giving_nothing() -> TestResult {
    let (store, at) = store()?;
    let rows = whole(&store, at)?;
    let (emptied, object) = rows
        .iter()
        .find_map(|row| match row.objects() {
            [(object, _)] => object.as_id().map(|id| (row.key().to_vec(), id)),
            _ => None,
        })
        .context("a list of one ID")?;
    let mut write = QuadWrite::new(&store, TENANT, store.lists().request(at));
    write.request().get_from_delta(&emptied).add_posting(
        Posting::default().with_uid(object.to_u128()).with_op(DEL),
        false,
    );
    let after = commit(&store, vec![write])?
        .first()
        .copied()
        .context("commits")?;
    let expected = whole(&store, after)?;
    assert_eq!(expected.len() + 1, rows.len(), "the list holds nothing now");
    let (mut from, mut got, mut yielded) = (Vec::new(), Vec::new(), 0);
    // Each page but the last gives a row or hands back after a key giving nothing.
    let pages_max = rows.len() + 1;
    let mut pages = 0;
    loop {
        pages += 1;
        if pages > pages_max {
            return Err(TestError::new(format!(
                "more than {pages_max} pages: a page moved on from nothing"
            )));
        }
        let scan = forward(from, after, usize::MAX)?.yielding(NonZeroUsize::MIN);
        let page = store.page(&scan, |key, term, ids| {
            take(&mut got, key, term, ids);
            ControlFlow::Continue(())
        })?;
        let end = page.into_end();
        if let PageEnd::Yielded { from: next } = &end {
            assert_eq!(*next, [emptied.as_slice(), &[0]].concat());
            yielded += 1;
        }
        match end.resume_from() {
            Some(next) => from = next,
            None => break,
        }
    }
    assert_eq!(got, expected);
    assert_eq!(yielded, 1, "a key giving a row spends no bound");
    Ok(())
}

/// The objects of `hub`'s `knows` read from `after` as a Scan names it: the page's objects and how it ended.
fn resumed_at(
    store: &TestQuads,
    version: u64,
    after: &[u8],
    limit: usize,
) -> TestResult<(Vec<EncodedTerm>, Page)> {
    let hub = store
        .id_of(TENANT, ex("hub").as_str())?
        .context("the subject")?;
    let knows = store
        .id_of(TENANT, ex("knows").as_str())?
        .context("the predicate")?;
    let key = ultragraphdb_core_keys::QuadKey::Forward {
        tenant: TENANT,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: knows,
        subject: hub,
    }
    .to_bytes();
    let scan = forward(key.clone(), version, limit)?
        .to([key.as_slice(), &[0]].concat())
        .resuming(Some(store.resume_at(Keyspace::Forward, after)?));
    let mut objects = Vec::new();
    let page = store.page(&scan, |_, object, _| {
        objects.push(object.clone());
        ControlFlow::Continue(())
    })?;
    Ok((objects, page))
}

/// A range's `after` reads the list from that object on, the object itself when the list holds it, and the next object when it
/// does not; none past the last gives no row, since the page reads no row of a key it found empty.
#[test]
fn test_a_range_resumes_its_list_at_the_object_its_after_names() -> TestResult {
    let (store, at) = store()?;
    let hub = store
        .id_of(TENANT, ex("hub").as_str())?
        .context("the subject")?;
    let knows = store
        .id_of(TENANT, ex("knows").as_str())?
        .context("the predicate")?;
    let key = ultragraphdb_core_keys::QuadKey::Forward {
        tenant: TENANT,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: knows,
        subject: hub,
    }
    .to_bytes();
    let whole: Vec<EncodedTerm> = store
        .objects(&key, at)?
        .into_iter()
        .map(|(object, _)| object)
        .collect();
    assert_eq!(whole.len(), 5);
    for (from, object) in whole.iter().enumerate() {
        let (read, page) = resumed_at(&store, at, object.as_bytes(), 8)?;
        assert_eq!(read, whole.get(from..).unwrap_or_default(), "at {object:?}");
        assert_eq!((page.end(), page.rows()), (&PageEnd::Exhausted, 1));
    }
    // An ID the list does not hold, in the bucket below the second object's: the second object is the one after it.
    let second = whole.get(1).context("a second object")?;
    let absent = second.as_id().context("an ID")?.to_u128() - (1 << 24);
    let absent = EncodedTerm::id(SystemId::from_u128(absent)?);
    assert!(!whole.contains(&absent));
    let (next, _page) = resumed_at(&store, at, absent.as_bytes(), 8)?;
    assert_eq!(next, whole.get(1..).unwrap_or_default());
    let last = whole.last().context("a last object")?;
    let beyond = EncodedTerm::id(SystemId::from_u128(
        last.as_id().context("an ID")?.to_u128() + (1 << 48),
    )?);
    let (nothing, page) = resumed_at(&store, at, beyond.as_bytes(), 1)?;
    assert!(nothing.is_empty());
    assert_eq!((page.end(), page.rows()), (&PageEnd::Exhausted, 0));
    Ok(())
}

/// A page pausing before it took anything of a key resumed at an object keeps that position, so the next page reads the object,
/// and keeps no list: it paused inside none.
#[test]
fn test_a_pause_before_the_object_an_after_names_keeps_the_position() -> TestResult {
    let (store, at) = store()?;
    let hub = store
        .id_of(TENANT, ex("hub").as_str())?
        .context("the subject")?;
    let knows = store
        .id_of(TENANT, ex("knows").as_str())?
        .context("the predicate")?;
    let key = ultragraphdb_core_keys::QuadKey::Forward {
        tenant: TENANT,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: knows,
        subject: hub,
    }
    .to_bytes();
    let objects = store.objects(&key, at)?;
    let (object, _) = objects.get(2).context("a third object")?;
    let identity = store.resume_at(Keyspace::Forward, object.as_bytes())?;
    let scan = forward(key.clone(), at, 8)?.resuming(Some(identity.clone()));
    let paused = store.page(&scan, |_, _, _| ControlFlow::Break(()))?;
    let (end, held) = paused.into_parts();
    let PageEnd::Paused { last, after, .. } = end else {
        return Err(TestError::new("the sink stopped it"));
    };
    assert_eq!((last, after), (None, Some(identity)));
    assert!(
        held.is_none(),
        "a page taking nothing of the list keeps none"
    );
    Ok(())
}
