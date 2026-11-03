/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataState;
use ultragraphdb_core_ids::{Epoch, Namespace, TypePrefix};

#[test]
fn test_the_limits() {
    assert_eq!(MetadataState::PENDING_MAX, 256);
    assert_eq!(MetadataState::RETAINED_MAX, 1_024);
    assert_eq!(MetadataState::DEFAULTS_MAX, 1_048_576);
    assert_eq!(MetadataState::PRINCIPALS_MAX, 65_536);
    assert_eq!(MetadataState::EVICTIONS_PENDING_MAX, 65_536);
}

#[test]
fn test_a_new_state_holds_nothing() {
    let state = MetadataState::new();
    assert_eq!(state, MetadataState::default());
    assert_eq!(state.last_learned(), Epoch::ZERO);
    assert_eq!(state.current_version(Namespace::new(1)), 0);
    assert!(state.pending_versions().is_empty());
    assert!(state.namespaces().is_empty());
    assert_eq!(state.next_sequence(TypePrefix::RESOURCE), 1);
    assert!(state.placement().is_none());
    assert_eq!(state.principals(), 0);
    assert_eq!(state.sessions(), 0);
    assert!(state.pending_evictions().is_empty());
}
