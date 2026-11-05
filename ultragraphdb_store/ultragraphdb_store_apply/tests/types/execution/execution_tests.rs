/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An execution held open across a transaction's requests: each changeset applied on top of the ones before, reads
//! of its own writes over its snapshot, recorded in its read set.

use std::num::NonZeroUsize;
use std::ops::ControlFlow;

use oxrdf::{GraphName, Term};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey};
use ultragraphdb_store_apply::ChangesetErrorEnum;
use ultragraphdb_store_quads::{KernelRow, KernelScan, PageEnd, QuadErrorEnum};

use crate::utils::changesets::{
    TENANT, TestChangesets, changeset, changesets, delete, ex, insert, uid,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

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
