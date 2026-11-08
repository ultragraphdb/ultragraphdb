/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The dictionary keys a page's rows want, read before the merge sees the rows: each once, in as many Scans as a Scan's ranges
//! take.

use std::time::Duration;

use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::played::played_cluster::{PlayedCluster, quiet_times, run};
use crate::played::played_data::OneList;
use crate::played::played_read::{Bound, read, snapshot_of};

/// The ranges a Scan of compute's holds at most, which `src/types/partition_rows/mod.rs` states so a frame holds them with room to
/// spare.
const RANGES_PER_SCAN: usize = 4_096;

/// How long the read of a page past a Scan's ranges may take: well under a second under a debug build, where a read naming its page
/// again at every row rather than once per window (`Page::named_more`) takes the rows' square and runs past half a minute.
const READ_BOUND: Duration = Duration::from_secs(5);

/// A page whose rows name more terms than a Scan's ranges hold reads their names in several Scans, each within the bound, and
/// every row is named.
#[test]
fn test_a_page_naming_more_terms_than_a_scan_holds_reads_them_in_several() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let objects = u64::try_from(RANGES_PER_SCAN + RANGES_PER_SCAN / 8)?;
        let list = OneList::new("subject", "predicate", objects)?;
        list.hold_in(partition);
        let bound = Bound {
            subject: Some(list.subject.iri.clone()),
            predicate: Some(list.predicate.iri.clone()),
            object: None,
        };
        let mut client = played.client().await?;
        let answer = compio::time::timeout(READ_BOUND, read(&mut client, &snapshot_of(&bound)?))
            .await
            .map_err(|_| TestError::new("the read of the page did not end within its bound"))??
            .answered()?;
        assert_eq!(answer.column(0), list.object_iris());
        let widest = partition
            .scans()
            .iter()
            .map(|scan| scan.ranges.len())
            .max()
            .unwrap_or_default();
        assert!(widest <= RANGES_PER_SCAN, "a Scan of {widest} ranges");
        Ok(())
    })
}
