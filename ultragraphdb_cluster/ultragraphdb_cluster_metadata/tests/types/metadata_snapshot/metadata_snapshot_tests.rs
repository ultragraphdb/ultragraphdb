/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataSnapshot, MetadataState};
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_log::Position;

#[test]
fn test_a_snapshot_holds_its_position_and_state() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&MetadataCommand::OfferCatalog {
        tenant: Tenant::new(1),
        predecessor: 0,
        bytes: b"v1".to_vec(),
    })?;
    let snapshot = MetadataSnapshot::new(Position::new(4), state.clone());
    assert_eq!(snapshot.position(), Position::new(4));
    assert_eq!(snapshot.state(), &state);
    assert_eq!(snapshot.into_state(), state);
    Ok(())
}
