/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;
use std::sync::Arc;

use loom::thread;
use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_core_mapping::{Change, Changeset, RandomSource};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_traits_blocks::TestBlockSource;

type SimChangesets =
    ChangesetApplier<SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

const TENANT: u64 = 0;

/// The stack of a model's thread, in words: 8 MiB.
const STACK_WORDS: usize = 1 << 20;

/// A changeset applier over a fault-free simulated store, shared as the commit path and its callers share it; `None`
/// when it does not open.
fn applier() -> Option<Arc<SimChangesets>> {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let cache = SimListCache::new(64, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    ChangesetApplier::new(
        Lists::new(store, cache, ListsConfig::default()),
        TestBlockSource::new(NonZeroU64::MIN.saturating_add(999)),
        RandomSource::new(1),
        executor,
        1,
    )
    .ok()
    .map(Arc::new)
}

/// One changeset inserting `<subject> <p> <o>` in the default graph.
fn insert_of(subject: &str) -> Changeset {
    let node = |name: &str| NamedNode::new_unchecked(format!("http://example.org/{name}"));
    Changeset::new(TENANT).with(Change::Insert {
        graph: GraphName::default(),
        subject: NamedOrBlankNode::from(node(subject)),
        predicate: node("p"),
        object: Term::from(node("o")),
    })
}

/// A Loom thread with room for an apply: Loom runs each thread as a coroutine on a small stack of its own, which
/// executing a changeset overflows. `None` when the thread does not start.
fn roomy<T: Send + 'static>(
    run: impl FnOnce() -> T + Send + 'static,
) -> Option<thread::JoinHandle<T>> {
    thread::Builder::new()
        .stack_size(STACK_WORDS)
        .spawn(run)
        .ok()
}

/// Runs `body` as a model, on a thread with room for an apply, under every interleaving of the threads it starts.
fn model(body: fn()) {
    loom::model(move || {
        let ran = roomy(body).map(thread::JoinHandle::join);
        assert!(matches!(ran, Some(Ok(()))), "the model's thread");
    });
}

/// The applier's watermark.
fn watermark(applier: &SimChangesets) -> u64 {
    applier.quads().lists().applied_ts()
}

/// Two changesets applied from two threads at once serialize through the apply lock: both commit, at two versions,
/// neither between the other's read and its write, and the watermark ends at the higher.
#[test]
fn test_two_applies_racing_commit_at_two_versions() {
    model(|| {
        let applier = applier();
        assert!(applier.is_some(), "the applier opens");
        let Some(applier) = applier else { return };
        let other = Arc::clone(&applier);
        let theirs = roomy(move || {
            other
                .apply(&insert_of("a"))
                .map(|applied| applied.commit_ts())
        });
        assert!(theirs.is_some(), "the other thread starts");
        let Some(theirs) = theirs else { return };
        let mine = applier
            .apply(&insert_of("b"))
            .map(|applied| applied.commit_ts());
        let theirs = theirs.join();
        assert!(theirs.is_ok(), "the other thread's apply");
        let versions = [mine.ok(), theirs.ok().and_then(Result::ok)];
        assert!(versions.iter().all(Option::is_some), "{versions:?}");
        assert_ne!(versions[0], versions[1], "{versions:?}");
        assert_eq!(
            Some(watermark(&applier)),
            versions.iter().flatten().max().copied()
        );
    });
}

/// A changeset applied while another thread drops the tenant's data: both end, in either order, with no deadlock
/// between the apply lock and the locks of the catalogs, the build and the vector queues, and the watermark ends at
/// the later of the two.
#[test]
fn test_an_apply_racing_a_drop_of_its_tenants_data_both_end() {
    model(|| {
        let applier = applier();
        assert!(applier.is_some(), "the applier opens");
        let Some(applier) = applier else { return };
        let other = Arc::clone(&applier);
        let dropping = roomy(move || other.drop_data(TENANT));
        assert!(dropping.is_some(), "the dropping thread starts");
        let Some(dropping) = dropping else { return };
        let applied = applier
            .apply(&insert_of("a"))
            .map(|applied| applied.commit_ts());
        let dropped = dropping.join();
        assert!(dropped.is_ok(), "the dropping thread");
        let versions = [applied.ok(), dropped.ok().and_then(Result::ok)];
        assert!(versions.iter().all(Option::is_some), "{versions:?}");
        assert_ne!(versions[0], versions[1], "{versions:?}");
        assert_eq!(
            Some(watermark(&applier)),
            versions.iter().flatten().max().copied()
        );
    });
}

/// The catalogs forgotten on one thread while another applies: the apply finds its tenant's catalog, loaded before
/// the forgetting or again after it, and the catalog is there once both end.
#[test]
fn test_an_apply_racing_a_forgetting_of_the_catalogs_finds_its_catalog() {
    model(|| {
        let applier = applier();
        assert!(applier.is_some(), "the applier opens");
        let Some(applier) = applier else { return };
        let other = Arc::clone(&applier);
        let forgetting = roomy(move || other.forget_catalogs());
        assert!(forgetting.is_some(), "the forgetting thread starts");
        let Some(forgetting) = forgetting else { return };
        assert!(applier.apply(&insert_of("a")).is_ok());
        assert!(forgetting.join().is_ok(), "the forgetting thread");
        assert!(applier.catalog(TENANT).is_ok());
    });
}
