/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The catalog-version scenarios over the commit step: versions announced by a close and taking effect as rounds of
//! their own, builds started and swapped per partition, recovery running a build's backfill again and asking for default
//! entries again. A partition is a commit step over a store of its own; the metadata group is a test double, its own
//! half `ultragraphdb_cluster_metadata`'s in a cluster, which no store or transactions crate depends on.

use cucumber::{then, when};
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::{Epoch, Namespace, Version};
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_catalog::{HeldVersion, TestCatalogSource};
use ultragraphdb_transactions_commit::{CommitStep, Pipeline, Replay, RoundBounds, Sessions};
use ultragraphdb_transactions_txn::{Prepared, TxnBuffers};

use crate::Steps;
use crate::utils::fixture::{
    Applier, Driver, NS, applier, applier_over, begin, catalog_proposal, ex, id_of, insert_literal,
    open_wal, prepare, setup, sim_store, term_version, version, write,
};

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

/// Version `version` of the catalog of `ns`, as a close announces it.
fn announced(version: u64) -> ultragraphdb_cluster_sequencer::CatalogVersion {
    ultragraphdb_cluster_sequencer::CatalogVersion::new(Namespace::new(NS), version)
}

/// A driver over `applier` whose latest delivered close is `delivered`, with catalog version 1 of `ns` in effect, an
/// empty version the partition loaded.
fn driver_at(applier: &Applier, delivered: u64) -> TestResult<Driver<'_>> {
    applier.load_catalog(&CatalogContents::new(NS, 1, Vec::new()))?;
    let mut driver = Driver::new(applier, unbounded());
    driver.step = CommitStep::new(unbounded(), applier.quads().lists().applied_ts())
        .with_delivered(Epoch::new(delivered).context("the epoch")?)
        .with_catalog(&[announced(1)]);
    Ok(driver)
}

/// Commits `contents` to `source`, in effect from `effective`, as the metadata group holds it.
fn commit_version(
    source: &TestCatalogSource,
    contents: &CatalogContents,
    effective: u64,
) -> TestResult {
    let bytes = contents.encode()?;
    source.commit(HeldVersion::new(
        contents.namespace(),
        contents.version(),
        effective,
        bytes,
    ));
    Ok(())
}

/// The store keys of the catalog keyspace `applier`'s store holds: none, since no partition writes one.
fn catalog_keys(applier: &Applier) -> usize {
    let catalog = Keyspace::Catalog.byte();
    applier
        .quads()
        .store()
        .contents()
        .keys()
        .keys()
        .filter(|k| k.first() == Some(&catalog))
        .count()
}

/// The subjects the `tokenizer` index of `predicate` lists under `token` in `applier`'s store, at the watermark.
fn listed(
    applier: &Applier,
    predicate: &str,
    tokenizer: u8,
    token: &str,
) -> TestResult<Vec<ultragraphdb_core_ids::SystemId>> {
    let key = ultragraphdb_core_keys::QuadKey::ValueIndex {
        namespace: NS,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: id_of(applier, predicate)?,
        tokenizer,
        token: token.as_bytes().to_vec(),
    }
    .to_bytes();
    Ok(applier
        .quads()
        .objects(&key, applier.quads().lists().applied_ts())
        .context("the index reads")?
        .iter()
        .filter_map(|(o, _)| o.as_id())
        .collect())
}

/// Dgraph's identifiers of `term` and `exact` (`tok/tok.go`).
const TERM: u8 = 0x01;
const EXACT: u8 = 0x02;

#[when("versions 3 and 4 of `ns` both take effect at epoch 40, each starting an index build")]
fn two_builds(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    applier.sequence_builds();
    let _version: u64 = setup(
        &applier,
        vec![
            insert_literal("a", "p", "apple"),
            insert_literal("a", "q", "quince"),
        ],
    )?;
    let mut driver = driver_at(&applier, 38)?;
    driver.close_with(39, vec![announced(3), announced(4)], Vec::new())?;
    // Version 4 builds on version 3: `p`'s index, then `q`'s beside it.
    let three = term_version(NS, 3, "p");
    let four = CatalogContents::new(
        NS,
        4,
        vec![
            Declaration::Predicate(
                PredicateDeclaration::new(ex("p")).with_tokenizers(vec!["term".into()]),
            ),
            Declaration::Predicate(
                PredicateDeclaration::new(ex("q")).with_tokenizers(vec!["term".into()]),
            ),
        ],
    );
    driver.submit(1, catalog_proposal(&applier, &three)?)?;
    driver.sync()?;
    assert!(applier.rebuilding(), "version 3's build runs");
    // Version 4 waits for the build's end.
    assert!(Prepared::catalog(&applier, &four, TxnBuffers::new()).is_err());
    applier.executor().run_until_idle();
    let end = applier
        .take_build_end()
        .context("version 3's build ended")?;
    driver.submit(2, Prepared::end_build(&applier, end, TxnBuffers::new())?)?;
    driver.sync()?;
    driver.submit(3, catalog_proposal(&applier, &four)?)?;
    driver.sync()?;
    assert!(applier.rebuilding(), "version 4's build runs");
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert_literal("b", "q", "pear")])?;
    driver.submit(4, prepare(txn)?)?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    w.versions = driver
        .appended
        .iter()
        .map(|r| u64::from(r.transactions()))
        .collect();
    Ok(())
}

#[then(
    "the build of version 3 begins in one round and that of version 4 in a later round, and no transaction aborts for either"
)]
fn two_rounds(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.versions,
        vec![1, 1, 1, 1],
        "each build's start a round of its own"
    );
    let outcomes: Vec<u64> = w.replies.iter().map(|r| r.ticket()).collect();
    assert_eq!(outcomes, vec![1, 2, 3, 4]);
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    let epochs: Vec<u64> = w
        .replies
        .iter()
        .filter_map(|r| r.outcome().clone().ok())
        .map(|v| Version::from_u64(v).epoch().get())
        .collect();
    assert!(epochs.iter().all(|e| *e == 40), "{epochs:?}");
    Ok(())
}

#[when(
    "a build of a term index on `name` has started, its backfill is half written, and the process is killed before the build's completion record is durable"
)]
async fn crash_during_build(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let (_storage, opened) = open_wal().await?;
    let mut pipe: Pipeline<'_, _, _, _, _, _> =
        Pipeline::new(&applier, opened, unbounded()).context("it opens")?;
    for (ticket, subject, value) in [(1, "a", "big apple"), (2, "b", "red pear")] {
        let mut txn = begin(&applier)?;
        write(&mut txn, vec![insert_literal(subject, "name", value)])?;
        pipe.commit(ticket, prepare(txn)?, 0)?;
        pipe.sync(0).await.context("synced and applied")?;
    }
    let indexing = term_version(NS, 2, "name");
    commit_version(&w.source, &indexing, 1)?;
    let refusals = pipe.install(3, &indexing, 0)?;
    assert!(refusals.is_empty(), "{refusals:?}");
    pipe.sync(0).await.context("the build's start is durable")?;
    assert!(applier.rebuilding(), "the backfill runs, never finished");
    let mut log = pipe.into_log();
    // The process is killed: the store's state is lost, and recovery replays the log into an empty store.
    let recovered = applier_over(sim_store())?;
    recovered.sequence_builds();
    let replayed = Replay::run_with_catalog(
        0,
        &recovered,
        &mut log,
        Sessions::DEFAULT_CAPACITY,
        &w.source,
    )
    .await
    .context("replayed")?;
    w.versions = vec![replayed.rounds() as u64];
    w.checks = vec![recovered.rebuilding()];
    let mut reopened: Pipeline<'_, _, _, _, _, _> =
        Pipeline::new(&recovered, log, unbounded()).context("it opens again")?;
    recovered.executor().run_until_idle();
    reopened.poll_builds(0).context("the end is proposed")?;
    reopened
        .sync(0)
        .await
        .context("the completion record is durable")?;
    w.checks.push(recovered.rebuilding());
    w.ids = [
        listed(&recovered, "name", TERM, "apple")?,
        listed(&recovered, "name", TERM, "pear")?,
    ]
    .concat();
    w.expected_ids = vec![id_of(&recovered, "a")?, id_of(&recovered, "b")?];
    Ok(())
}

#[then(
    "recovery runs the backfill again from the build's start version, and once its completion record is durable the index lists every subject the data holds"
)]
fn backfill_again(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.versions,
        vec![3],
        "the two writes and the build's start replayed"
    );
    assert_eq!(
        w.checks,
        vec![true, false],
        "the build ran again and was swapped in"
    );
    assert_eq!(w.ids, w.expected_ids);
    Ok(())
}

#[when("a predicate is declared and another is first written")]
fn declared_and_written(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let mut driver = driver_at(&applier, 0)?;
    let declaring = CatalogContents::new(
        NS,
        2,
        vec![Declaration::Predicate(
            PredicateDeclaration::new(ex("name")).with_tokenizers(vec!["exact".into()]),
        )],
    );
    commit_version(&w.source, &declaring, 2)?;
    driver.close_with(1, vec![announced(2)], Vec::new())?;
    driver.submit(1, catalog_proposal(&applier, &declaring)?)?;
    driver.sync()?;
    let mut txn = begin(&applier)?;
    write(
        &mut txn,
        vec![
            insert_literal("a", "name", "Ann"),
            insert_literal("a", "alias", "Al"),
        ],
    )?;
    driver.submit(2, prepare(txn)?)?;
    driver.sync()?;
    for entry in applier.take_default_entries() {
        let _created: bool = w.source.record_default(&entry)?;
    }
    w.replies = driver.replies.clone();
    w.versions = vec![catalog_keys(&applier) as u64];
    Ok(())
}

#[then(
    "no partition's store holds a catalog key for either, and the metadata group holds both entries"
)]
fn both_entries(w: &mut Steps) -> TestResult {
    use ultragraphdb_traits_catalog::CatalogSource;
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    assert_eq!(w.versions, vec![0], "no catalog key");
    let held = w
        .source
        .version_at(NS, u64::MAX)?
        .context("the declaration's version")?;
    let contents = CatalogContents::decode(NS, held.version(), held.contents())?;
    assert_eq!(
        contents.declarations().len(),
        1,
        "the declaration of `name`"
    );
    let defaults: Vec<String> = w
        .source
        .defaults(NS)
        .iter()
        .map(|e| e.predicate().to_string())
        .collect();
    assert!(
        defaults.contains(&ex("alias").as_str().to_string()),
        "{defaults:?}"
    );
    assert!(
        !defaults.contains(&ex("name").as_str().to_string()),
        "a declared predicate has no default entry"
    );
    Ok(())
}

#[when(
    "a commit first writes the undeclared `alias` and the process is killed before its default-entry command is sent, and the partition recovers"
)]
async fn killed_before_the_command(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let (_storage, opened) = open_wal().await?;
    let mut pipe: Pipeline<'_, _, _, _, _, _> =
        Pipeline::new(&applier, opened, unbounded()).context("it opens")?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert_literal("a", "alias", "Al")])?;
    pipe.commit(1, prepare(txn)?, 0)?;
    pipe.sync(0).await.context("synced and applied")?;
    let mut log = pipe.into_log();
    // Killed: the command the applied round asked for is never sent, and the store's state is lost.
    drop(applier);
    let recovered = applier_over(sim_store())?;
    let _replayed = Replay::run_with_catalog(
        0,
        &recovered,
        &mut log,
        Sessions::DEFAULT_CAPACITY,
        &w.source,
    )
    .await
    .context("replayed")?;
    for entry in recovered.take_default_entries() {
        let _created: bool = w.source.record_default(&entry)?;
    }
    Ok(())
}

#[then(
    "replay sends the command for `alias`, and once it is answered the metadata group holds its record"
)]
fn record_held(w: &mut Steps) -> TestResult {
    let defaults: Vec<String> = w
        .source
        .defaults(NS)
        .iter()
        .map(|e| e.predicate().to_string())
        .collect();
    assert_eq!(defaults, vec![ex("alias").as_str().to_string()]);
    Ok(())
}

/// A partition over a store of its own holding `<a> <name> "big apple"`.
fn partition() -> TestResult<Applier> {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert_literal("a", "name", "big apple")])?;
    Ok(applier)
}

#[when(
    "an `exact` index is declared over two partitions, and partition 1's build completes before partition 2's"
)]
fn two_partitions_swap(w: &mut Steps) -> TestResult {
    let (one, two) = (partition()?, partition()?);
    one.sequence_builds();
    two.sequence_builds();
    let exact = CatalogContents::new(
        NS,
        2,
        vec![Declaration::Predicate(
            PredicateDeclaration::new(ex("name")).with_tokenizers(vec!["exact".into()]),
        )],
    );
    let (mut first, mut second) = (driver_at(&one, 0)?, driver_at(&two, 0)?);
    for (driver, applier) in [(&mut first, &one), (&mut second, &two)] {
        driver.close_with(1, vec![announced(2)], Vec::new())?;
        driver.submit(1, catalog_proposal(applier, &exact)?)?;
        driver.sync()?;
    }
    one.executor().run_until_idle();
    let first_end = one.take_build_end().context("partition 1's build ended")?;
    first.submit(2, Prepared::end_build(&one, first_end, TxnBuffers::new())?)?;
    first.sync()?;
    let tokenizers = |a: &Applier| -> TestResult<Vec<String>> {
        Ok(a.catalog(NS)?
            .predicate(id_of(a, "name")?)
            .context("`name` is declared")?
            .tokenizers()
            .to_vec())
    };
    w.terms = Vec::new();
    w.names = vec![tokenizers(&one)?.join(","), tokenizers(&two)?.join(",")];
    two.executor().run_until_idle();
    let second_end = two.take_build_end().context("partition 2's build ended")?;
    second.submit(2, Prepared::end_build(&two, second_end, TxnBuffers::new())?)?;
    second.sync()?;
    w.names.push(tokenizers(&two)?.join(","));
    w.versions = vec![catalog_keys(&one) as u64, catalog_keys(&two) as u64];
    w.ids = [
        listed(&one, "name", EXACT, "big apple")?,
        listed(&two, "name", EXACT, "big apple")?,
    ]
    .concat();
    w.expected_ids = vec![id_of(&one, "a")?, id_of(&two, "a")?];
    Ok(())
}

#[then(
    "queries reading partition 1 use the new declaration, queries reading partition 2 the interim one until its completion record is durable, and neither swap writes a catalog key"
)]
fn each_swaps(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.names,
        vec!["exact".to_string(), String::new(), "exact".to_string()],
        "partition 1 swapped, partition 2 interim, then swapped"
    );
    assert_eq!(w.versions, vec![0, 0]);
    assert_eq!(w.ids, w.expected_ids);
    Ok(())
}

#[when(
    "an index is added while epoch 48 is the latest close, over two partitions, and the sequencer closes epoch 49"
)]
fn switch_together(w: &mut Steps) -> TestResult {
    let (one, two) = (applier()?, applier()?);
    let indexing = term_version(NS, 2, "name");
    commit_version(&w.source, &indexing, 50)?;
    let mut epochs = Vec::new();
    let mut listed_words = Vec::new();
    for applier in [&one, &two] {
        let _version: u64 = setup(
            applier,
            vec![crate::utils::fixture::insert("z", "seen", "name")],
        )?;
        let mut driver = driver_at(applier, 48)?;
        let mut txn = begin(applier)?;
        write(&mut txn, vec![insert_literal("a", "name", "plum")])?;
        driver.submit(1, prepare(txn)?)?;
        driver.sync()?;
        listed_words.push(listed(applier, "name", TERM, "plum")?.len());
        driver.close_with(49, vec![announced(2)], Vec::new())?;
        driver.submit(2, catalog_proposal(applier, &indexing)?)?;
        driver.sync()?;
        let mut later = begin(applier)?;
        write(&mut later, vec![insert_literal("b", "name", "fig")])?;
        driver.submit(3, prepare(later)?)?;
        driver.sync()?;
        listed_words.push(listed(applier, "name", TERM, "fig")?.len());
        epochs.extend(
            driver
                .replies
                .iter()
                .filter_map(|r| r.outcome().clone().ok())
                .map(|v| Version::from_u64(v).epoch().get()),
        );
    }
    w.epochs = epochs;
    w.versions = listed_words.into_iter().map(|n| n as u64).collect();
    Ok(())
}

#[then(
    "the change takes effect at epoch 50, and in both partitions rounds of epoch 49 maintain no entries of the index and rounds of epoch 50 do"
)]
fn switched_together(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.epochs,
        vec![49, 50, 50, 49, 50, 50],
        "a write at 49, the version and a write at 50"
    );
    assert_eq!(
        w.versions,
        vec![0, 1, 0, 1],
        "per partition: 49's write unindexed, 50's indexed"
    );
    Ok(())
}

#[when(
    "an index is added while epoch 48 is the latest close, and a partition's latest delivered close is 48"
)]
fn holds_before_stamping(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(
        &applier,
        vec![crate::utils::fixture::insert("z", "seen", "name")],
    )?;
    let mut driver = driver_at(&applier, 48)?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert_literal("a", "name", "plum")])?;
    driver.submit(1, prepare(txn)?)?;
    driver.sync()?;
    w.checks = vec![driver.step.catalog_version(NS) == 1];
    driver.close_with(49, vec![announced(2)], Vec::new())?;
    w.checks.push(driver.step.catalog_version(NS) == 2);
    driver.submit(2, catalog_proposal(&applier, &term_version(NS, 2, "name"))?)?;
    driver.sync()?;
    let mut later = begin(&applier)?;
    w.checks.push(later.catalog() == 2);
    write(&mut later, vec![insert_literal("b", "name", "fig")])?;
    driver.submit(3, prepare(later)?)?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    Ok(())
}

#[then(
    "the partition's commits carry epoch 49 until the close of 49, carrying the new version, is delivered to it, and its first commit at epoch 50 is validated against the new version"
)]
fn validated_against_new(w: &mut Steps) -> TestResult {
    assert_eq!(w.checks, vec![true, true, true]);
    let committed: Vec<(u64, u64)> = w
        .replies
        .iter()
        .filter_map(|r| r.outcome().clone().ok().map(|v| (r.ticket(), v)))
        .collect();
    assert_eq!(
        committed,
        vec![
            (1, version(49, 0)),
            (2, version(50, 0)),
            (3, version(50, 2))
        ]
    );
    Ok(())
}
