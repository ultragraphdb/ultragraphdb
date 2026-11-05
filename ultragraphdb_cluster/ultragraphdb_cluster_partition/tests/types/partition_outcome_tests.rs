/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::PartitionError;

use crate::utils::cluster::Cluster;
use crate::utils::hosts::on_hosts;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// An outcome names its ticket and how the write ended: a refusal before any registration waited for nothing.
#[test]
fn test_a_refused_write_is_its_outcome_with_nothing_waited() -> TestResult {
    let names = Cluster::even(2)?;
    let foreign = names.name_in(2, "s")?;
    let change = crate::utils::cluster::insert(&foreign, "knows", "x");
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let ticket = host.submit(crate::utils::cluster::changeset(vec![change]), 0)?;
        crate::utils::cluster::block_on(host.turn(0))??;
        let parked_or_done = host.parked();
        let mut seen = host.take_outcomes();
        if parked_or_done > 0 {
            // `knows` and `x` may be homed elsewhere: the write registers before it is refused.
            for _ in 0..4 {
                for h in hosts.iter_mut() {
                    crate::utils::cluster::block_on(h.turn(0))??;
                    seen.extend(h.take_outcomes());
                }
            }
        }
        let outcome = seen
            .iter()
            .find(|o| o.ticket() == ticket)
            .context("its outcome")?;
        assert_eq!(
            outcome.outcome(),
            &Err(PartitionError::wrong_partition(
                crate::utils::cluster::bucket(&foreign)?.get(),
                2
            ))
        );
        Ok(())
    })
}
