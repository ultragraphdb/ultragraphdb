/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition's page of rows as the merge drains it: each row named before the merge sees it, and no further than the rows
//! the read may still return.

use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::apart::apart_cluster::run;
use crate::apart::apart_reads::{Lists, answer_on, holding, of_p, rows_listed};

/// A read whose limit stops inside a page names the rows it returns in one Scan and reads no name past them: its partition is
/// asked the dictionary Scan of its terms, the list's Scan and one Scan of names.
#[test]
fn test_a_page_names_no_row_past_the_limit() -> TestResult {
    const LISTS: Lists<'static> = &[(
        "http://ex.org/s1",
        &[
            "http://ex.org/o1",
            "http://ex.org/o2",
            "http://ex.org/o3",
            "http://ex.org/o4",
        ],
    )];
    run(async {
        let (cluster, mut client) = holding(LISTS, false).await?;
        let answer = answer_on(&mut client, &of_p(2, Vec::new())).await?;
        let rows = rows_listed(LISTS);
        assert_eq!(answer.whole()?, rows.get(..2).context("two rows")?);
        let scans = cluster.partition(1)?.scans();
        assert_eq!(scans.len(), 3, "{scans:?}");
        Ok(())
    })
}
