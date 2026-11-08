/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The keyspace a read takes in a partition by its bound positions (`PROTOCOL.md` §11.12, quad-reads "Access paths follow the
//! bound positions").

use ultragraphdb_tool_bdd::TestResult;

use crate::played::played_cluster::{PlayedCluster, quiet_times, run};
use crate::played::played_data::{TwoLists, forward, subject_predicates};
use crate::played::played_read::{Bound, read, snapshot_of};

/// A bound subject alone reads its predicates' list, then the forward list of each predicate it names.
#[test]
fn test_a_bound_subject_alone_reads_each_list_its_predicates_list_names() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let lists = TwoLists::new("subject")?;
        lists.hold_in(partition);
        let bound = Bound {
            subject: Some(lists.subject.iri.clone()),
            predicate: None,
            object: None,
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(answer.texts(), lists.rows());
        let firsts: Vec<Vec<u8>> = partition
            .scans()
            .into_iter()
            .flat_map(|scan| scan.ranges)
            .map(|range| range.first)
            .collect();
        assert!(
            firsts.contains(&subject_predicates(&lists.subject)),
            "{firsts:?}"
        );
        for list in &lists.lists {
            assert!(
                firsts.contains(&forward(&list.predicate, &list.subject)),
                "{firsts:?}"
            );
        }
        Ok(())
    })
}
