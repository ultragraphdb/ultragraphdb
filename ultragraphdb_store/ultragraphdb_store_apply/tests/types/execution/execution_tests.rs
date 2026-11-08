/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An execution held open across a transaction's requests: each changeset applied on top of the ones before, reads
//! of its own writes over its snapshot, recorded in its read set.

use std::num::{NonZeroU64, NonZeroUsize};
use std::ops::ControlFlow;

use oxrdf::{GraphName, NamedNode, Term};
use ultragraphdb_core_ids::{BucketGroup, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey};
use ultragraphdb_core_mapping::{Change, StatementRef};
use ultragraphdb_store_apply::ChangesetErrorEnum;
use ultragraphdb_store_quads::{KernelRow, KernelScan, Page, PageEnd, QuadErrorEnum, id_key};
use ultragraphdb_traits_store::Store;

use crate::utils::changesets::{
    TENANT, TestChangesets, changeset, changesets, delete, ex, insert, uid,
};
use crate::utils::partitions::bucket_of;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

fn read(
    execution: &mut ultragraphdb_store_apply::Execution<
        '_,
        ultragraphdb_store_sim::SimStore,
        ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
        ultragraphdb_traits_blocks::TestBlockSource,
    >,
    subject: &str,
    predicate: &str,
) -> TestResult<Vec<Term>> {
    let mut out = vec![Term::from(ex("stale"))];
    let (s, p) = (ex(subject), ex(predicate));
    execution
        .objects_into(
            GraphName::DefaultGraph.as_ref(),
            s.as_ref().into(),
            p.as_ref(),
            &mut out,
        )
        .context("the read")?;
    out.sort_by_key(Term::to_string);
    Ok(out)
}

fn commit(applier: &TestChangesets, text: &str) -> TestResult<u64> {
    Ok(applier
        .apply(&changeset(vec![insert(text)?]))
        .context("applied")?
        .commit_ts())
}

#[test]
fn test_an_execution_reads_its_own_writes_over_its_snapshot() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <z>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    assert_eq!((execution.tenant(), execution.read_ts()), (TENANT, at));
    execution.apply(&changeset(vec![insert("<a> <knows> <b>")?]))?;
    execution.apply(&changeset(vec![insert("<n> <knows> <a>")?]))?;
    assert_eq!(
        read(&mut execution, "a", "knows")?,
        vec![Term::from(ex("b")), Term::from(ex("z"))]
    );
    // A subject the execution created reads its delta.
    assert_eq!(
        read(&mut execution, "n", "knows")?,
        vec![Term::from(ex("a"))]
    );
    let executed = execution.finish()?;
    let forward = QuadKey::Forward {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate: uid(&applier, "knows")?,
        subject: uid(&applier, "a")?,
    }
    .to_bytes();
    assert!(
        executed.reads().lists().contains(&forward),
        "the read is a whole-list read"
    );
    assert_eq!(executed.read_ts(), at);
    Ok(())
}

#[test]
fn test_an_execution_at_an_earlier_version_reads_that_snapshot() -> TestResult {
    let applier = changesets()?;
    let first = commit(&applier, "<a> <knows> <b>")?;
    let _commit: u64 = commit(&applier, "<a> <knows> <c>")?;
    let mut execution = applier.begin(TENANT, first).context("begun")?;
    assert_eq!(
        read(&mut execution, "a", "knows")?,
        vec![Term::from(ex("b"))]
    );
    Ok(())
}

#[test]
fn test_a_read_of_an_unknown_subject_and_predicate_records_the_subject_absent() -> TestResult {
    // Neither term exists: the subject is looked up first, so its absence is what a later creation of it conflicts on.
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    assert!(read(&mut execution, "ghost", "phantom")?.is_empty());
    let executed = execution.finish()?;
    assert!(
        executed
            .reads()
            .absent()
            .contains(&taken(&executed, "ghost")?)
    );
    Ok(())
}

#[test]
fn test_a_read_of_unknown_terms_holds_nothing_and_records_them_absent() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    assert!(read(&mut execution, "nobody", "knows")?.is_empty());
    assert!(read(&mut execution, "a", "never")?.is_empty());
    let mut out = vec![Term::from(ex("stale"))];
    let (s, p, g) = (ex("a"), ex("knows"), ex("nograph"));
    execution.objects_into(
        GraphName::NamedNode(g.clone()).as_ref(),
        s.as_ref().into(),
        p.as_ref(),
        &mut out,
    )?;
    assert!(out.is_empty());
    let executed = execution.finish()?;
    for name in ["nobody", "never", "nograph"] {
        assert!(
            executed.reads().absent().contains(&taken(&executed, name)?),
            "{name} recorded absent"
        );
    }
    Ok(())
}

/// The ID `<name>` took in `executed`, which created it: a read names a term by ID, got or created as any IRI is.
fn taken(
    executed: &ultragraphdb_store_apply::Executed,
    name: &str,
) -> TestResult<ultragraphdb_core_ids::SystemId> {
    executed
        .terms()
        .written()
        .find(|(term, _)| *term == ex(name).as_str())
        .map(|(_, id)| id)
        .context("the execution created the IRI")
}

/// The ID a writer of `<new>` is given now, its execution released at once: the ID another write holds, or a new one.
fn id_now(applier: &TestChangesets) -> TestResult<ultragraphdb_core_ids::SystemId> {
    let at = applier.quads().lists().applied_ts();
    let mut writer = applier.begin(TENANT, at).context("begun")?;
    writer.apply(&changeset(vec![insert("<new> <knows> <a>")?]))?;
    let executed = writer.finish()?;
    let id = taken(&executed, "new")?;
    applier.release(&executed);
    Ok(id)
}

#[test]
fn test_an_open_execution_released_lets_go_of_what_it_created_without_finishing() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    execution.apply(&changeset(vec![insert("<new> <knows> <b>")?]))?;
    // While the execution is open, every writer of `<new>` takes the ID it created.
    let created = id_now(&applier)?;
    assert_eq!(id_now(&applier)?, created, "the open execution holds it");
    applier.release_execution(execution);
    assert_ne!(
        id_now(&applier)?,
        created,
        "released, its ID is never used again"
    );
    assert_eq!(
        applier
            .quads()
            .id_of(TENANT, ex("new").as_str())
            .context("the dictionary reads")?,
        None,
        "nothing is written"
    );
    assert_eq!(applier.quads().lists().applied_ts(), at);
    Ok(())
}

#[test]
fn test_a_finish_the_store_fails_releases_what_the_execution_created() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    execution.apply(&changeset(vec![insert("<new> <knows> <b>")?]))?;
    let created = id_now(&applier)?;
    assert_eq!(id_now(&applier)?, created, "the open execution holds it");
    // The derivation's first read of the store fails, and finishing it fails with it.
    applier
        .quads()
        .store()
        .inject(ultragraphdb_store_sim::Fault::FailNextRead);
    let failed = execution
        .finish()
        .err()
        .context("the read the derivation needs fails")?;
    assert!(
        matches!(
            &failed.0,
            ultragraphdb_store_apply::ChangesetErrorEnum::Quad(ultragraphdb_store_quads::QuadError(
                ultragraphdb_store_quads::QuadErrorEnum::Store(ultragraphdb_error_store::StoreError(
                    ultragraphdb_error_store::StoreErrorEnum::ReadFailed { reason, .. }
                ))
            )) if reason == "simulated read failure"
        ),
        "{failed:?}"
    );
    assert_ne!(
        id_now(&applier)?,
        created,
        "released by the failed finish, its ID is never used again"
    );
    assert_eq!(
        applier
            .quads()
            .id_of(TENANT, ex("new").as_str())
            .context("the dictionary reads")?,
        None,
        "nothing is written"
    );
    Ok(())
}

/// An execution prints its tenant and snapshot, and nothing of the writes it holds.
#[test]
fn test_an_execution_prints_its_tenant_and_snapshot() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    execution.apply(&changeset(vec![insert("<new> <knows> <b>")?]))?;
    assert_eq!(
        format!("{execution:?}"),
        format!("Execution {{ tenant: {TENANT}, read_ts: {at}, .. }}")
    );
    Ok(())
}

/// The subjects of the statements an execution named by their reifier and resolved, in order, across its changesets:
/// a property set and one removed through a reifier, and a statement deleted by it; a statement named by its triple,
/// and a reifier naming no statement, add none.
#[test]
fn test_an_execution_gives_the_subjects_its_reifiers_resolved_to() -> TestResult {
    use crate::utils::changesets::{insert_edge, postings, term};
    use oxrdf::NamedNode;
    use ultragraphdb_core_mapping::{Change, StatementRef};
    let applier = changesets()?;
    let _applied = applier
        .apply(&changeset(vec![
            insert_edge("<a> <knows> <b>", &[("since", "2020")])?,
            insert_edge("<c> <knows> <d>", &[("since", "2021")])?,
        ]))
        .context("applied")?;
    let reifier = |subject: &str| -> TestResult<NamedNode> {
        let id = postings(&applier, subject, "knows")?
            .first()
            .and_then(|(_, ids)| ids.first().copied())
            .context("a statement ID")?;
        Ok(NamedNode::new(applier.quads().iri_of(TENANT, id)?.0)?)
    };
    let by_reifier = |subject: &str| -> TestResult<StatementRef> {
        Ok(StatementRef::Reifier {
            graph: GraphName::DefaultGraph,
            reifier: reifier(subject)?,
        })
    };
    let at = applier.quads().lists().applied_ts();
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    assert!(execution.reified_subjects().is_empty());
    execution.apply(&changeset(vec![
        Change::SetProperty {
            statement: by_reifier("c")?,
            predicate: ex("weight"),
            value: term("1")?,
        },
        Change::SetProperty {
            statement: StatementRef::Triple {
                graph: GraphName::DefaultGraph,
                subject: ex("a").into(),
                predicate: ex("knows"),
                object: ex("b").into(),
            },
            predicate: ex("weight"),
            value: term("2")?,
        },
        Change::DeleteStatement {
            graph: GraphName::DefaultGraph,
            reifier: ex("nothing"),
        },
    ]))?;
    assert_eq!(execution.reified_subjects(), &[uid(&applier, "c")?]);
    execution.apply(&changeset(vec![
        Change::DeleteProperty {
            statement: by_reifier("a")?,
            predicate: ex("since"),
            value: None,
        },
        Change::DeleteStatement {
            graph: GraphName::DefaultGraph,
            reifier: reifier("c")?,
        },
    ]))?;
    assert_eq!(
        execution.reified_subjects(),
        &[
            uid(&applier, "c")?,
            uid(&applier, "a")?,
            uid(&applier, "c")?
        ]
    );
    Ok(())
}

type TestExecution<'a> = ultragraphdb_store_apply::Execution<
    'a,
    ultragraphdb_store_sim::SimStore,
    ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
    ultragraphdb_traits_blocks::TestBlockSource,
>;

/// Lists of several objects committed for [`overlaid`].
fn committed(applier: &TestChangesets) -> TestResult {
    let changes = [
        "<a> <knows> <y>",
        "<a> <knows> <z>",
        "<held> <knows> <y>",
        "<gone> <knows> <z>",
        "<c> <knows> <w>",
        "<c> <knows> <x>",
    ]
    .iter()
    .map(|text| insert(text))
    .collect::<TestResult<Vec<_>>>()?;
    let _applied = applier.apply(&changeset(changes))?;
    Ok(())
}

/// An execution over [`committed`]'s lists: one it adds to, one it reads whole, one it emptied, one it left alone, and one only
/// it holds.
fn overlaid(applier: &TestChangesets) -> TestResult<TestExecution<'_>> {
    let at = applier.quads().lists().applied_ts();
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    execution.apply(&changeset(vec![
        insert("<a> <knows> <b>")?,
        insert("<new> <knows> <a>")?,
        insert("<new> <knows> <b>")?,
        delete("<gone> <knows> <z>")?,
    ]))?;
    let _whole = read(&mut execution, "held", "knows")?;
    Ok(execution)
}

/// The forward lists of the tenant as `execution` sees them, whole.
fn forward(version: u64) -> TestResult<KernelScan> {
    Ok(KernelScan::new(
        Keyspace::Forward,
        TENANT,
        Vec::new(),
        version,
        NonZeroUsize::MAX,
    ))
}

/// `execution`'s pages of `scan`'s range at `budget` objects a page, `limit` rows a scan: the rows, whole, with the span after
/// each page held to what section 13.13 states.
fn paged(
    execution: &mut TestExecution<'_>,
    scan: &KernelScan,
    budget: usize,
    limit: NonZeroUsize,
) -> TestResult<Vec<KernelRow>> {
    let start = scan.bounds().0;
    let end = scan.bounds().1;
    let (mut rows, mut from, mut after, mut left) =
        (Vec::<KernelRow>::new(), start.clone(), None, limit);
    loop {
        let this = KernelScan::new(scan.keyspace(), TENANT, from.clone(), scan.version(), left)
            .to(end.clone().context("a tenant's keyspace ends")?)
            .resuming(after.take());
        let mut taken = 0;
        let page = execution.page(&this, |key, object, statements| {
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
        // Through the past key at the end; just past the last key read when stopped or paused.
        let reach = match page.end() {
            PageEnd::Exhausted => end.clone(),
            PageEnd::Stopped { .. } | PageEnd::Paused { .. } => {
                let last = rows.last().context("a row")?.key();
                Some([last, &[0]].concat())
            }
            PageEnd::Yielded { from: next } => Some(next.clone()),
        };
        assert_eq!(
            execution.scanned_ranges(),
            vec![(start.clone(), reach)],
            "one range however many pages, at {budget} objects a page"
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

/// Pages over an execution's writes concatenate to what its scan returns, and leave one range and no list newly held: an added
/// key is read, an emptied list gives no row, and a held list reads what it holds.
#[test]
fn test_an_execution_s_pages_concatenate_to_its_scan_and_hold_one_range() -> TestResult {
    let applier = changesets()?;
    committed(&applier)?;
    let expected = {
        let mut execution = overlaid(&applier)?;
        let mut rows = Vec::new();
        let scan = forward(execution.read_ts())?;
        assert_eq!(execution.scan(&scan, &mut rows)?, None);
        rows
    };
    let gone = QuadKey::Forward {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate: uid(&applier, "knows")?,
        subject: uid(&applier, "gone")?,
    }
    .to_bytes();
    assert!(expected.iter().all(|row| row.key() != gone), "{expected:?}");
    assert_eq!(
        expected.len(),
        4,
        "<a>, <held>, <c> and <new>: {expected:?}"
    );
    for budget in 1..=4 {
        for limit in [NonZeroUsize::MIN, NonZeroUsize::MAX] {
            let mut execution = overlaid(&applier)?;
            let held = execution.held_lists();
            let scan = forward(execution.read_ts())?;
            let rows = paged(&mut execution, &scan, budget, limit)?;
            assert_eq!(rows, expected, "{budget} objects a page, limit {limit}");
            assert_eq!(execution.held_lists(), held, "nothing newly held");
        }
    }
    Ok(())
}

/// A range holding nothing is recorded through its past key, which is what proves absence.
#[test]
fn test_a_page_of_an_empty_range_covers_it_through_its_past_key() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    let start = QuadKey::Forward {
        tenant: TENANT + 1,
        graph: DEFAULT_GRAPH,
        predicate: uid(&applier, "knows")?,
        subject: uid(&applier, "a")?,
    }
    .to_bytes();
    let past = [start.as_slice(), &[0]].concat();
    let scan = KernelScan::new(
        Keyspace::Forward,
        TENANT + 1,
        start.clone(),
        at,
        NonZeroUsize::MIN,
    )
    .to(past.clone());
    let page = execution.page(&scan, |_, _, _| ControlFlow::Break(()))?;
    assert_eq!((page.end(), page.rows()), (&PageEnd::Exhausted, 0));
    assert_eq!(execution.scanned_ranges(), vec![(start, Some(past))]);
    Ok(())
}

/// A sink taking nothing leaves a paused end with no last key, and nothing recorded.
#[test]
fn test_a_page_whose_sink_took_nothing_records_nothing() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    let page = execution.page(&forward(at)?, |_, _, _| ControlFlow::Break(()))?;
    assert!(
        matches!(page.end(), PageEnd::Paused { last: None, .. }),
        "{page:?}"
    );
    assert_eq!(execution.scanned_ranges(), Vec::new());
    assert_eq!(execution.read_counts().ranges(), 0);
    Ok(())
}

#[test]
fn test_a_page_of_a_keyspace_holding_no_lists_is_refused_recording_nothing() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    let scan = KernelScan::new(Keyspace::Marks, TENANT, Vec::new(), at, NonZeroUsize::MIN);
    let refused = execution
        .page(&scan, |_, _, _| ControlFlow::Continue(()))
        .err()
        .context("refused")?;
    let corrupted = matches!(
        &refused.0,
        ChangesetErrorEnum::Quad(q) if matches!(q.0, QuadErrorEnum::Corrupted { .. })
    );
    assert!(corrupted, "{refused:?}");
    assert_eq!(execution.scanned_ranges(), Vec::new());
    Ok(())
}

/// Keys with their stored values, in key order.
type Values = Vec<(Vec<u8>, Vec<u8>)>;

/// What `execution` reads of the statements, each key's value, and how the page ended.
fn statements_read(
    execution: &mut TestExecution<'_>,
    scan: &KernelScan,
) -> TestResult<(Values, Page)> {
    let mut read = Vec::new();
    let page = execution
        .values_page(scan, |key, value| {
            read.push((key.to_vec(), value.to_vec()));
            ControlFlow::Continue(())
        })
        .context("the read")?;
    Ok((read, page))
}

/// The record of the one statement ID of `<a> <knows> <b>` at the watermark, its key and stored bytes.
fn stored_statement(a: &TestChangesets) -> TestResult<(Vec<u8>, Vec<u8>)> {
    let id = *crate::utils::changesets::postings(a, "a", "knows")?
        .first()
        .context("a posting")?
        .1
        .first()
        .context("a statement ID")?;
    let version = a.quads().lists().applied_ts();
    let record = a
        .quads()
        .statement(TENANT, DEFAULT_GRAPH, id, version)?
        .context("a record")?;
    let key = QuadKey::Statement {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        statement: id,
    }
    .to_bytes();
    Ok((key, record.encode()))
}

/// The statement of `<a> <knows> <b>`, named by its reifier.
fn reifier_of_a(a: &TestChangesets) -> TestResult<StatementRef> {
    let id = *crate::utils::changesets::postings(a, "a", "knows")?
        .first()
        .context("a posting")?
        .1
        .first()
        .context("a statement ID")?;
    Ok(StatementRef::Reifier {
        graph: GraphName::DefaultGraph,
        reifier: NamedNode::new(a.quads().iri_of(TENANT, id)?.0)?,
    })
}

/// A read of the statements over an execution's writes: its own property over the stored bytes, a removal absent, a statement of
/// its own found, and its span a range scanned.
#[test]
fn test_a_page_of_the_statements_sees_the_execution_s_own_records() -> TestResult {
    let a = changesets()?;
    crate::utils::changesets::write_changes(
        &a,
        vec![
            crate::utils::changesets::insert_edge("<a> <knows> <b>", &[("since", "2020")])?,
            crate::utils::changesets::insert_edge("<c> <knows> <d>", &[("since", "2021")])?,
        ],
    )?;
    let read_ts = a.quads().lists().applied_ts();
    let (key, stored) = stored_statement(&a)?;
    let scan = KernelScan::new(
        Keyspace::Statements,
        TENANT,
        Vec::new(),
        read_ts,
        NonZeroUsize::new(16).context("a limit")?,
    );
    let mut execution = a.begin(TENANT, read_ts)?;
    let (before, page) = statements_read(&mut execution, &scan)?;
    assert_eq!(before.len(), 2);
    assert!(before.contains(&(key.clone(), stored.clone())));
    assert_eq!(page.end(), &PageEnd::Exhausted);
    let (start, end) = scan.bounds();
    assert_eq!(execution.scanned_ranges(), vec![(start, end)]);
    let statement = reifier_of_a(&a)?;
    execution.apply(&changeset(vec![
        Change::SetProperty {
            statement: statement.clone(),
            predicate: ex("until"),
            value: crate::utils::changesets::term("2030")?,
        },
        Change::DeleteProperty {
            statement,
            predicate: ex("since"),
            value: None,
        },
    ]))?;
    let (over, _page) = statements_read(&mut execution, &scan)?;
    let mine = over
        .iter()
        .find(|(k, _)| *k == key)
        .context("the statement's record")?;
    assert_ne!(mine.1, stored, "its own record over the stored bytes");
    let decoded = ultragraphdb_store_quads::StatementRecord::decode(&key, &mine.1)?;
    let until = execution
        .find_iri(ex("until").as_ref())?
        .context("the property's ID")?;
    assert_eq!(
        decoded
            .properties()
            .iter()
            .map(|(p, _)| *p)
            .collect::<Vec<_>>(),
        vec![until],
        "its own property, the one it removed gone"
    );
    assert_eq!(over.len(), 2);
    execution.apply(&changeset(vec![crate::utils::changesets::insert_edge(
        "<e> <knows> <f>",
        &[("since", "2022")],
    )?]))?;
    assert_eq!(statements_read(&mut execution, &scan)?.0.len(), 3);
    Ok(())
}

/// Stopped at its limit a read's span reaches just past the last key it gave; paused having given nothing it records nothing; and a
/// keyspace of lists is refused, recording nothing.
#[test]
fn test_a_page_of_the_statements_records_what_it_covered() -> TestResult {
    let a = changesets()?;
    crate::utils::changesets::write_changes(
        &a,
        vec![
            crate::utils::changesets::insert_edge("<a> <knows> <b>", &[("since", "2020")])?,
            crate::utils::changesets::insert_edge("<c> <knows> <d>", &[("since", "2021")])?,
        ],
    )?;
    let read_ts = a.quads().lists().applied_ts();
    let limited = |limit| -> TestResult<KernelScan> {
        Ok(KernelScan::new(
            Keyspace::Statements,
            TENANT,
            Vec::new(),
            read_ts,
            NonZeroUsize::new(limit).context("a limit")?,
        ))
    };
    let mut paused = a.begin(TENANT, read_ts)?;
    let scan = limited(16)?;
    let nothing = paused.values_page(&scan, |_, _| ControlFlow::Break(()))?;
    assert_eq!(nothing.rows(), 0);
    assert!(paused.scanned_ranges().is_empty());
    let lists = KernelScan::new(
        Keyspace::Forward,
        TENANT,
        Vec::new(),
        read_ts,
        NonZeroUsize::MIN,
    );
    assert!(
        paused
            .values_page(&lists, |_, _| ControlFlow::Continue(()))
            .is_err()
    );
    assert!(paused.scanned_ranges().is_empty());
    let mut stopped = a.begin(TENANT, read_ts)?;
    let (read, page) = statements_read(&mut stopped, &limited(1)?)?;
    let [(key, _)] = read.as_slice() else {
        return Err(TestError::new("one record"));
    };
    assert_eq!(page.end(), &PageEnd::Stopped { last: key.clone() });
    let (start, _) = limited(1)?.bounds();
    let past = [key.as_slice(), &[0]].concat();
    assert_eq!(stopped.scanned_ranges(), vec![(start, Some(past))]);
    Ok(())
}

/// What an execution's reads come to, asked before it finishes, is what its read set then holds: each list read whole and each
/// mark read, each statement tested, each range scanned and each term found absent, with the bytes of their keys.
#[test]
fn test_an_execution_s_read_counts_are_what_its_read_set_holds() -> TestResult {
    let applier = changesets()?;
    committed(&applier)?;
    let at = applier.quads().lists().applied_ts();
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    execution.apply(&changeset(vec![insert("<a> <knows> <b>")?]))?;
    let _whole = read(&mut execution, "held", "knows")?;
    let _none = read(&mut execution, "ghost", "knows")?;
    let entity = SystemId::new(TypePrefix::RESOURCE, bucket_of("o")?, Sequence::new(5)?);
    execution.mark(
        entity,
        BucketGroup::new(7)?,
        NonZeroU64::new(4).context("a generation")?,
    )?;
    let _page = execution.page(&forward(at)?, |_, _, _| ControlFlow::Continue(()))?;
    let counts = execution.read_counts();
    let executed = execution.finish()?;
    let reads = executed.reads();
    for (held, what) in [
        (reads.lists().len(), "a list and a mark"),
        (reads.statements().len(), "a statement tested"),
        (reads.ranges().len(), "a range"),
        (reads.absent().len(), "a term absent"),
    ] {
        assert!(held > 0, "the read set holds {what}");
    }
    let key_bytes = reads.lists().iter().map(Vec::len).sum::<usize>()
        + reads
            .statements()
            .iter()
            .map(|(key, object)| key.len() + object.len())
            .sum::<usize>()
        + reads
            .ranges()
            .iter()
            .map(|(start, end)| start.len() + end.as_ref().map_or(0, Vec::len))
            .sum::<usize>();
    assert_eq!(
        counts,
        ultragraphdb_store_apply::ReadCounts::new(
            key_bytes,
            reads.lists().len(),
            reads.statements().len(),
            reads.ranges().len(),
            reads.absent().len(),
        )
    );
    Ok(())
}

/// An execution holds no list until it reads one, and then each list it read whole: a read's subject's predicate list, consulted
/// first, and its forward list, the two its read set holds.
#[test]
fn test_an_execution_holds_the_lists_it_read() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    assert_eq!(execution.held_lists(), 0);
    let _whole = read(&mut execution, "a", "knows")?;
    let held = execution.held_lists();
    let executed = execution.finish()?;
    assert_eq!(executed.reads().lists().len(), 2, "{:?}", executed.reads());
    assert_eq!(held, executed.reads().lists().len());
    Ok(())
}

/// An execution knows an IRI once a read of it resolved it, and not before.
#[test]
fn test_an_execution_knows_the_iris_its_reads_resolved() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    assert!(!execution.knows_iri(ex("a").as_str()));
    let _whole = read(&mut execution, "a", "knows")?;
    assert!(execution.knows_iri(ex("a").as_str()));
    assert!(execution.knows_iri(ex("knows").as_str()));
    assert!(!execution.knows_iri(ex("b").as_str()), "named by no read");
    Ok(())
}

/// The dictionary as an execution sees it: what the store holds, what the execution created over it, and nothing for a key
/// neither holds, into a vector cleared first.
#[test]
fn test_an_execution_reads_the_dictionary_with_its_own_terms_over_the_store() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<a> <knows> <b>")?;
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    execution.apply(&changeset(vec![insert("<new> <knows> <a>")?]))?;
    let new = execution
        .find_iri(ex("new").as_ref())?
        .context("<new>'s ID")?;
    let nobody = SystemId::new(
        TypePrefix::RESOURCE,
        bucket_of("nobody")?,
        Sequence::new(9_999)?,
    );
    let keys = [
        id_key(TENANT, uid(&applier, "a")?),
        id_key(TENANT, new),
        id_key(TENANT, nobody),
    ];
    assert_eq!(
        applier.quads().store().get_versionless(&keys[1])?,
        None,
        "<new> is the execution's alone"
    );
    let borrowed: Vec<&[u8]> = keys.iter().map(Vec::as_slice).collect();
    let mut out = vec![Some(b"stale".to_vec())];
    execution.dictionary_into(&borrowed, &mut out)?;
    assert_eq!(
        out,
        vec![
            Some(ex("a").as_str().as_bytes().to_vec()),
            Some(ex("new").as_str().as_bytes().to_vec()),
            None,
        ]
    );
    Ok(())
}

/// A page of the referrer sets reads what the store holds, an execution keeping no referrer set of its own, and records its
/// span.
#[test]
fn test_a_page_of_the_referrer_sets_reads_the_stored_sets_and_records_its_span() -> TestResult {
    let applier = changesets()?;
    let at = commit(&applier, "<s> <knows> <hub>")?;
    let key = QuadKey::Referrers {
        tenant: TENANT,
        entity: uid(&applier, "hub")?,
    }
    .to_bytes();
    let stored = applier
        .quads()
        .store()
        .get_at(&key, at)?
        .context("the hub's referrer set")?
        .value()
        .to_vec();
    let mut execution = applier.begin(TENANT, at).context("begun")?;
    let scan = KernelScan::new(
        Keyspace::Referrers,
        TENANT,
        Vec::new(),
        at,
        NonZeroUsize::MIN,
    );
    let (read, page) = statements_read(&mut execution, &scan)?;
    assert_eq!(read, vec![(key, stored)]);
    assert_eq!(page.end(), &PageEnd::Exhausted);
    assert_eq!(execution.scanned_ranges(), vec![scan.bounds()]);
    Ok(())
}
