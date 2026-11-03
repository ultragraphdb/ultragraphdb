/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An execution held open across a transaction's requests: each changeset applied on top of the ones before, reads
//! of its own writes over its snapshot, recorded in its read set.

use oxrdf::{GraphName, Term};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};

use crate::utils::changesets::{NS, TestChangesets, changeset, changesets, ex, insert, uid};
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
    let mut execution = applier.begin(NS, at).context("begun")?;
    assert_eq!((execution.namespace(), execution.read_ts()), (NS, at));
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
        namespace: NS,
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
    let mut execution = applier.begin(NS, first).context("begun")?;
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
    let mut execution = applier.begin(NS, at).context("begun")?;
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
    let mut execution = applier.begin(NS, at).context("begun")?;
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
    let mut writer = applier.begin(NS, at).context("begun")?;
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
    let mut execution = applier.begin(NS, at).context("begun")?;
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
            .id_of(NS, ex("new").as_str())
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
    let mut execution = applier.begin(NS, at).context("begun")?;
    execution.apply(&changeset(vec![insert("<new> <knows> <b>")?]))?;
    let created = id_now(&applier)?;
    assert_eq!(id_now(&applier)?, created, "the open execution holds it");
    // The derivation's first read of the store fails, and finishing it fails with it.
    applier
        .quads()
        .store()
        .inject(ultragraphdb_store_sim::Fault::FailNextRead);
    let failed = execution.finish();
    assert!(failed.is_err(), "the read the derivation needs fails");
    assert_ne!(
        id_now(&applier)?,
        created,
        "released by the failed finish, its ID is never used again"
    );
    assert_eq!(
        applier
            .quads()
            .id_of(NS, ex("new").as_str())
            .context("the dictionary reads")?,
        None,
        "nothing is written"
    );
    Ok(())
}
