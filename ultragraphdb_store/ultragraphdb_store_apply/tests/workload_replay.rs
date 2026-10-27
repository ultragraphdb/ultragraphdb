/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A seed of the changeset workload replays its run: the same outcome, writes and checks.

use ultragraphdb_store_sim::Seed;
use ultragraphdb_tool_test::ChangesetWorkload;

#[test]
fn test_a_seed_replays_its_run() {
    let run = || {
        let (outcome, w) =
            ChangesetWorkload::run(Seed::new(7), ChangesetWorkload::faults(), 200).expect("run");
        (format!("{outcome:?}"), w.writes, w.checked)
    };
    assert_eq!(run(), run());
}
