/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The fixed seed set of the apply workload: with every fault class, power losses during drops and rebuilds among
//! them, and without faults; a seed's replay; and the workload's operations on fjall, closed and reopened.

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimRng, SteppedExecutor, Stream};
use ultragraphdb_traits_store::Store;

use crate::utils::apply_workload::{
    ApplyWorkload, administer, alter, applier_on, check, expected, initial_schema,
    undeclared_index_key, write,
};
use crate::utils::kill_point::{KILL_STAGES, KillPoint, say};

#[test]
fn test_seeds_without_faults() {
    let (mut checks, mut writes, mut changes, mut drops) = (0, 0, 0, 0);
    for seed in 1..=12 {
        let (_, w) = ApplyWorkload::run(Seed::new(seed), FaultConfig::none(), 300)
            .unwrap_or_else(|v| panic!("{v}"));
        checks += w.checks;
        writes += w.writes;
        changes += w.changes;
        drops += w.drops;
    }
    assert!(checks > 2000, "only {checks} checks");
    assert!(writes > 1000, "only {writes} writes");
    assert!(changes > 500, "only {changes} changes");
    assert!(drops > 50, "only {drops} drops");
}

#[test]
fn test_seeds_with_faults() {
    let (mut checks, mut drops) = (0, 0);
    for seed in 100..=123 {
        let (_, w) = ApplyWorkload::run(Seed::new(seed), ApplyWorkload::faults(), 300)
            .unwrap_or_else(|v| panic!("{v}"));
        checks += w.checks;
        drops += w.drops;
    }
    assert!(checks > 2000, "only {checks} checks");
    assert!(drops > 50, "only {drops} drops");
}

/// Power losses during drops, at every stage, and between the steps of rebuilds; crashes and power losses at every
/// stage of a rebuild, after which no key lies under an index prefix the loaded schema does not declare.
#[test]
fn test_seeds_with_power_losses_during_drops_and_rebuilds() {
    let faults = FaultConfig::none()
        .with_power_loss_during_drop(300)
        .with_crash_during_drop(100)
        .with_power_loss_between_steps(40)
        .with_power_loss_during_rebuild(200)
        .with_crash_during_rebuild(200);
    let (mut drops, mut recoveries) = (0, 0);
    for seed in 200..=211 {
        let (_, w) =
            ApplyWorkload::run(Seed::new(seed), faults, 300).unwrap_or_else(|v| panic!("{v}"));
        drops += w.drops;
        recoveries += w.recoveries;
    }
    assert!(drops > 20, "only {drops} drops");
    println!("{recoveries} recoveries checked for undeclared index keys");
    assert!(recoveries > 40, "only {recoveries} recoveries");
}

#[test]
fn test_a_seed_replays_its_run_and_its_events() {
    let run = || {
        let capture = ultragraphdb_store_sim::LogCapture::new();
        let outcome = capture.capture(|| {
            ApplyWorkload::run(Seed::new(7), ApplyWorkload::faults(), 200).expect("run")
        });
        (outcome, capture.events())
    };
    let (first, first_events) = run();
    let (again, again_events) = run();
    assert_eq!(first.0, again.0);
    assert_eq!(first.1.checks, again.1.checks);
    assert!(first_events.len() > 100, "{} events", first_events.len());
    assert_eq!(first_events, again_events);
}

/// The workload's operations and rule on fjall: every step's indexes agree with the data, across a close and a
/// reopen every hundred steps.
#[test]
fn test_the_workload_rules_hold_on_fjall() {
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let dir = root.join(format!("apply-workload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let seed = Seed::new(31);
    let mut rng = SimRng::new(31);
    let executor = SteppedExecutor::new(seed.stream(Stream::Executor));
    let mut checks = 0;
    for round in 0..3 {
        let store = FjallStore::open(&dir, Durability::Process).expect("the store opens");
        let a = applier_on(store, executor.clone(), seed).unwrap();
        if round == 0 {
            alter(&a, &initial_schema()).unwrap();
        }
        for step in 0..100 {
            let result = if rng.chance(600) {
                write(&a, &mut rng)
            } else {
                administer(&a, &mut rng).map(|_| ())
            };
            if let Err(e) = result {
                assert!(expected(&e), "step {step}: {e}");
            }
            if rng.chance(300) {
                executor.run_until_idle();
            }
            if let Some(detail) = check(&a).unwrap() {
                panic!("round {round} step {step}: {detail}");
            }
            checks += 1;
        }
        executor.run_until_idle();
        assert!(check(&a).unwrap().is_none());
    }
    assert_eq!(checks, 300);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The environment variables that make [`rebuild_kill_writer`] run: the store's directory and the stage to stop at.
const KILL_DIR: &str = "APPLY_KILL_DIR";
const KILL_STAGE: &str = "APPLY_KILL_STAGE";

/// The writer the kill test runs in a process of its own, this test binary run again: over fjall in `APPLY_KILL_DIR`,
/// it indexes `name` by term, writes, starts the exact index's build in the background, writes during it and runs it,
/// its store stopping the process at `APPLY_KILL_STAGE`. Ignored unless run by the kill test.
#[test]
#[ignore = "the kill test's writer, run in a process of its own"]
fn rebuild_kill_writer() {
    let (Some(dir), Some(stage)) = (std::env::var_os(KILL_DIR), std::env::var(KILL_STAGE).ok())
    else {
        return;
    };
    let store = FjallStore::open(&dir, Durability::Process).expect("the store opens");
    let seed = Seed::new(41);
    let executor = SteppedExecutor::new(seed.stream(Stream::Executor));
    let a = applier_on(KillPoint::new(store, &stage), executor.clone(), seed).unwrap();
    alter(&a, "name: string @index(term) .").unwrap();
    executor.run_until_idle();
    let mut rng = SimRng::new(41);
    for _ in 0..20 {
        let _ = write(&a, &mut rng);
    }
    a.lists().sync().unwrap();
    alter(&a, "name: string @index(term, exact) .").unwrap();
    for _ in 0..10 {
        let _ = write(&a, &mut rng);
    }
    a.lists().sync().unwrap();
    say("building");
    executor.run_until_idle();
    say("stage done");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// A rebuild on fjall killed at each of its stages leaves, once the store and the applier are opened again, no key
/// under an index prefix the loaded schema does not declare, every declared index agreeing with the data, no rebuild
/// record and no temporary keyspace's data in sight; killed after its swap batch, the new index is in place.
#[test]
fn test_a_rebuild_killed_at_each_stage_on_fjall_leaves_no_undeclared_index() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    for stage in KILL_STAGES {
        let dir = root.join(format!("apply-kill-{stage}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut child = Command::new(std::env::current_exe().expect("the test binary"))
            .args([
                "--exact",
                "utils::apply_workload_tests::rebuild_kill_writer",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(KILL_DIR, &dir)
            .env(KILL_STAGE, stage)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the writer starts");
        let stdout = child.stdout.take().expect("piped");
        // libtest prints the test's name without a newline before the test's own output.
        let wanted = format!("stage {stage}");
        let reached = BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .find(|l| l.ends_with(&wanted) || l.ends_with("stage done"))
            .is_some_and(|l| l.ends_with(&wanted));
        child.kill().expect("the writer is killed");
        child.wait().expect("the writer is reaped");
        assert!(reached, "the writer stopped before {stage}");
        let store = FjallStore::open(&dir, Durability::Process).expect("the store reopens");
        let name = ultragraphdb_core_keys::attr_in_root_namespace("name");
        let exact = ultragraphdb_store_apply::prefixes_to_delete_tokens_for(&name, "exact", false)
            .expect("a prefix");
        let left = {
            use ultragraphdb_traits_store::{Scan, StoreSnapshot};
            let snapshot = store.snapshot(u64::MAX).expect("readable");
            exact
                .iter()
                .map(|p| snapshot.scan(&Scan::prefix(p.clone())).count())
                .sum::<usize>()
        };
        if stage == "temporary-written" {
            assert!(
                left > 0,
                "writes during the build maintained the exact index"
            );
        }
        let records = store.rebuild_records().expect("readable").len();
        println!(
            "killed at {stage}: {left} exact keys and {records} records before the applier opens"
        );
        let seed = Seed::new(41);
        let a = applier_on(
            store,
            SteppedExecutor::new(seed.stream(Stream::Executor)),
            seed,
        )
        .expect("the applier opens");
        assert_eq!(undeclared_index_key(&a).unwrap(), None, "killed at {stage}");
        assert_eq!(check(&a).unwrap(), None, "killed at {stage}");
        assert!(
            a.lists().store().rebuild_records().unwrap().is_empty(),
            "{stage}"
        );
        let tokenizers = a
            .schema()
            .get(
                ultragraphdb_core_catalog::SchemaMode::Query,
                &ultragraphdb_core_keys::attr_in_root_namespace("name"),
            )
            .map(|s| s.tokenizer)
            .unwrap_or_default();
        let expected: &[&str] = if stage == "record-removed" {
            &["term", "exact"]
        } else {
            &["term"]
        };
        assert_eq!(tokenizers, expected, "killed at {stage}");
        println!("killed at {stage}: reopened with {tokenizers:?}");
        drop(a);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A term index built on fjall over 12,000 subjects, far more lists than the cache's eight: built whole, holding no
/// more postings in memory than the simulated store's build of half as many.
#[test]
fn test_a_rebuild_larger_than_the_cache_on_fjall() {
    use crate::utils::helpers::{attr, dkey, ikey, set_str};
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let dir = root.join(format!("apply-large-rebuild-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let seed = Seed::new(5);
    let executor = SteppedExecutor::new(seed.stream(Stream::Executor));
    let store = FjallStore::open(&dir, Durability::Process).expect("the store opens");
    let a = applier_on(store, executor.clone(), seed).unwrap();
    alter(&a, "big: string .").unwrap();
    let subjects = 12_000u64;
    let mut batch = a.lists().batch();
    let mut request = batch.request(a.lists().latest_ts());
    for uid in 1..=subjects {
        request
            .get(&dkey("big", uid))
            .unwrap()
            .add_mutation(&set_str("big", uid, &format!("w{uid} common")), None)
            .unwrap();
    }
    batch.push(request);
    a.lists().commit(batch).unwrap();
    let capture = ultragraphdb_store_sim::LogCapture::new();
    capture.capture(|| {
        alter(&a, "big: string @index(term) .").unwrap();
        executor.run_until_idle();
    });
    let event = capture
        .events()
        .into_iter()
        .find(|e| e.message() == Some("index built into the temporary keyspace"))
        .expect("the build logs its temporary keyspace");
    let held: u64 = event.field("postings_held").unwrap().parse().unwrap();
    let postings: u64 = event.field("postings").unwrap().parse().unwrap();
    println!("built {postings} postings on fjall holding at most {held}");
    assert_eq!(postings, 2 * subjects);
    assert!(held < postings / 2, "{held} of {postings} held");
    let read = |key: &[u8]| {
        let ts = a.lists().latest_ts();
        crate::utils::helpers::list_uids(&a.lists().read(key, ts).unwrap(), ts)
    };
    assert_eq!(read(&ikey("big", "term", "common")).len() as u64, subjects);
    assert_eq!(read(&ikey("big", "term", "w7777")), vec![7777]);
    let schema = a
        .schema()
        .get(ultragraphdb_core_catalog::SchemaMode::Query, &attr("big"));
    assert_eq!(schema.map(|s| s.tokenizer), Some(vec!["term".to_string()]));
    assert!(a.lists().store().rebuild_records().unwrap().is_empty());
    drop(a);
    let _ = std::fs::remove_dir_all(&dir);
}
