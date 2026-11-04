/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::ReverseQuery;
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::{Epoch, SystemId};
use ultragraphdb_core_keys::Graph;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_reverse_query_keeps_its_fields() -> TestResult {
    let predicate = SystemId::from_u128((1 << 112) | (3 << 48) | (1 << 24))?;
    let object = SystemId::from_u128((1 << 112) | (9 << 48) | (5 << 24))?;
    let query = ReverseQuery::new(
        PartitionId::new(2)?,
        4,
        Graph::Default,
        predicate,
        object,
        Epoch::new(15)?,
    );
    assert_eq!(
        (
            query.home(),
            query.namespace(),
            query.graph(),
            query.predicate(),
            query.object(),
            query.epoch()
        ),
        (
            PartitionId::new(2)?,
            4,
            Graph::Default,
            predicate,
            object,
            Epoch::new(15)?
        )
    );
    Ok(())
}
