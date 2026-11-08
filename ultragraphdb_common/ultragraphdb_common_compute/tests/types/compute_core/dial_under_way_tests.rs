/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A dial under way that its read gave up: whoever reads next dials for itself.

use std::time::Duration;

use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::played::played_cluster::{PlayedCluster, quiet_times, run};
use crate::played::played_data::OneList;
use crate::played::played_read::{Bound, cancel, read, send, snapshot_of};

/// How long the frontend holds compute's question where a partition listens, while the read asking it is cancelled.
const HELD: Duration = Duration::from_millis(500);

/// A read cancelled while its core dials a partition leaves the next read to dial it, which reads at once.
#[test]
fn test_a_read_cancelled_while_its_core_dials_leaves_the_next_to_dial() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let list = OneList::new("subject", "predicate", 2)?;
        list.hold_in(played.partition(1)?);
        let bound = Bound {
            subject: Some(list.subject.iri.clone()),
            predicate: Some(list.predicate.iri.clone()),
            object: None,
        };
        played.frontend().delay_members(HELD);
        let mut client = played.client().await?;
        let stream = send(&mut client, &snapshot_of(&bound)?).await?;
        let asked = async {
            while played.frontend().members_asked() == 0 {
                compio::time::sleep(Duration::from_millis(5)).await;
            }
        };
        compio::time::timeout(HELD * 2, asked)
            .await
            .map_err(|_| TestError::new("compute never asked the frontend for the members"))?;
        cancel(&mut client, stream).await?;
        played.frontend().delay_members(Duration::ZERO);
        compio::time::sleep(HELD / 5).await;
        let mut next = played.client().await?;
        let answer = compio::time::timeout(HELD * 4, read(&mut next, &snapshot_of(&bound)?))
            .await
            .map_err(|_| TestError::new("the read after the cancelled one did not end"))??;
        assert_eq!(answer.answered()?.column(0), list.object_iris());
        Ok(())
    })
}
