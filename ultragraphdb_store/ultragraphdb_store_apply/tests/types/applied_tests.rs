/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey, Region};

use crate::utils::changesets::{NS, apply, changesets, insert, uid, write};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A list an insert tests is read from the store, so it counts among the lists read and its region among the regions
/// read, though the read set holds the statement rather than the list.
#[test]
fn test_a_tested_list_counts_as_read_from_the_store() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <c>", "<b> <p> <q>"])?;
    let applied = apply(&a, vec![insert("<a> <knows> <b>")?]).context("applies")?;
    let key = QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(&a, "knows")?,
        subject: uid(&a, "a")?,
    }
    .to_bytes();
    assert_eq!(applied.lists_read(), 1);
    assert!(applied.read().contains(&Region::of(&key)?));
    assert!(applied.reads().lists().is_empty());
    Ok(())
}
