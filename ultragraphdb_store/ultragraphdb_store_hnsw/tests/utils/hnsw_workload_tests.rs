/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The fixed seed set of the vector-index workload: a hermetic test of the index on the simulated store, with and
//! without faults; and its operations once on fjall.

use std::collections::BTreeSet;

use ultragraphdb_core_tok::VEC_KEYWORD;
use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimRng};

use ultragraphdb_traits_index::Metric;

use crate::utils::helpers::{delete, draw, insert, lists_over, opts, oracle_score, pred, search};
use crate::utils::hnsw_workload::{HnswWorkload, graph_defect, names_dead};

#[test]
fn test_seeds_without_faults() {
    let (mut inserts, mut searches) = (0, 0);
    for seed in 1..=12 {
        let (outcome, i, s) = HnswWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 2)
            .unwrap_or_else(|v| panic!("{v}"));
        assert!(!outcome.contents().keys().is_empty());
        inserts += i;
        searches += s;
    }
    assert!(inserts > 800, "only {inserts} inserts checked");
    assert!(searches > 2000, "only {searches} searches checked");
}

#[test]
fn test_seeds_with_faults() {
    let (mut inserts, mut searches) = (0, 0);
    for seed in 100..=129 {
        let (_, i, s) = HnswWorkload::run(Seed::new(seed), HnswWorkload::faults(), 300, 2)
            .unwrap_or_else(|v| panic!("{v}"));
        inserts += i;
        searches += s;
    }
    assert!(inserts > 1000, "only {inserts} inserts checked");
    assert!(searches > 1000, "only {searches} searches checked");
}

/// The capture keeps tracing's process-wide callsite interest asking its subscriber, so events whose callsites
/// another test thread reached first are recorded too.
#[test]
fn test_a_seed_replays_its_run_and_its_events() {
    let run = || {
        let capture = ultragraphdb_store_sim::LogCapture::new();
        let outcome = capture.capture(|| {
            HnswWorkload::run(Seed::new(7), HnswWorkload::faults(), 250, 2).expect("run")
        });
        (outcome, capture.events())
    };
    let (first, first_events) = run();
    let (again, again_events) = run();
    assert_eq!(first.0, again.0);
    assert_eq!((first.1, first.2), (again.1, again.2));
    assert_eq!(first_events, again_events);
}

/// The workload's operations and rules on fjall: inserts, deletes and searches, the stored graph closed and the
/// lists written after a delete without the dead node.
#[test]
fn test_the_workload_rules_hold_on_fjall() {
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let dir = root.join(format!("hnsw-workload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let lists = lists_over(FjallStore::open(&dir, Durability::Process).expect("the store opens"));
    let p = pred("emb");
    let o = opts(&[
        ("maxLevels", "3"),
        ("efConstruction", "8"),
        ("efSearch", "6"),
    ]);
    let mut rng = SimRng::new(11);
    let mut live: Vec<(u64, Vec<f32>)> = Vec::new();
    let mut dead = BTreeSet::new();
    for uid in 1..=150u64 {
        if uid % 7 == 0 && live.len() > 3 {
            let (gone, _) = live.remove(rng.index(live.len()));
            delete(&lists, &p, gone).unwrap();
            dead.insert(gone);
        }
        let v: Vec<f32> = (0..3)
            .map(|_| (draw(&mut rng) * 2.0 - 1.0) as f32)
            .collect();
        let edges = insert(&lists, &p, &o, uid, &v, &mut || draw(&mut rng)).unwrap();
        assert_eq!(
            names_dead(&edges, &format!("{p}{VEC_KEYWORD}"), &dead),
            None,
            "insert of {uid}"
        );
        live.push((uid, v));
    }
    assert_eq!(graph_defect(&lists, &p, lists.latest_ts()).unwrap(), None);
    for _ in 0..50 {
        let q: Vec<f32> = (0..3)
            .map(|_| (draw(&mut rng) * 2.0 - 1.0) as f32)
            .collect();
        let found = search(&lists, &p, &o, &q, 4, None).unwrap();
        assert_eq!(found.len(), 4, "{} live", live.len());
        let widen = |v: &[f32]| v.iter().map(|&f| f64::from(f)).collect::<Vec<f64>>();
        let scores: Vec<f64> = found
            .iter()
            .map(|u| {
                let (_, v) = live.iter().find(|(l, _)| l == u).expect("a live vector");
                oracle_score(Metric::Euclidean, &widen(v), &widen(&q))
            })
            .collect();
        assert!(scores.windows(2).all(|w| w[0] <= w[1]), "{scores:?}");
    }
    drop(lists);
    let _ = std::fs::remove_dir_all(&dir);
}
