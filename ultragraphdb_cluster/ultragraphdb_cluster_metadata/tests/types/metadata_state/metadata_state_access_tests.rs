/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataState};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Ann (1, an administrator) and Bob (2) with sessions 7 and 8 for Bob.
fn two_principals() -> TestResult<MetadataState> {
    let mut state = MetadataState::new();
    for (id, name, administrator) in [(1, "ann", true), (2, "bob", false)] {
        state.apply(&MetadataCommand::CreatePrincipal {
            id,
            name: name.to_string(),
            verifier: ScramVerifier::new(vec![u8::try_from(id)?; 16], 4_096, [3; 32], [4; 32]),
            administrator,
        })?;
    }
    for session in [7, 8] {
        state.apply(&MetadataCommand::RegisterSession {
            session,
            principal: 2,
            evicts: Vec::new(),
        })?;
    }
    Ok(state)
}

#[test]
fn test_principals_by_name_and_by_identity() -> TestResult {
    let state = two_principals()?;
    assert_eq!(state.principals(), 2);
    let ann = state.principal("ann").context("ann")?;
    assert_eq!((ann.id(), ann.administrator()), (1, true));
    assert_eq!(state.principal_by_id(2).map(|p| p.name()), Some("bob"));
    assert!(state.principal("carol").is_none());
    assert!(state.principal_by_id(3).is_none());
    assert!(state.principal_by_id(0).is_none());
    Ok(())
}

#[test]
fn test_a_sessions_principal_is_the_owner_check() -> TestResult {
    let state = two_principals()?;
    assert_eq!(state.sessions(), 2);
    assert_eq!(state.session_principal(7)?, 2);
    assert_eq!(state.session_principal(8)?, 2);
    assert_eq!(
        state.session_principal(9).err().context("unknown")?.0,
        ClusterError::session_evicted(9).0
    );
    assert!(state.pending_evictions().is_empty());
    Ok(())
}
