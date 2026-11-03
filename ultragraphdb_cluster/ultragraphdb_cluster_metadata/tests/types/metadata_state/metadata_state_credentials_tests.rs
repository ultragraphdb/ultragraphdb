/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataState};
use ultragraphdb_protocol_scram::{
    Credentials, MIN_ITERATIONS, ScramClient, ScramServer, ScramVerifier,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A state holding `ann` with the verifier of `password` as principal 1, and `bob` as 2.
fn holding_ann(password: &str) -> TestResult<MetadataState> {
    let mut state = MetadataState::new();
    let ann = ScramVerifier::derive(password, &[7; 16], MIN_ITERATIONS)?;
    state.apply(&MetadataCommand::CreatePrincipal {
        id: 1,
        name: "ann".to_string(),
        verifier: ann,
        administrator: true,
    })?;
    state.apply(&MetadataCommand::CreatePrincipal {
        id: 2,
        name: "bob".to_string(),
        verifier: ScramVerifier::derive("bob's own password", &[8; 16], MIN_ITERATIONS)?,
        administrator: false,
    })?;
    Ok(state)
}

#[test]
fn test_a_lookup_answers_the_principal_and_its_verifier() -> TestResult {
    let state = holding_ann("ann's own password")?;
    let (id, verifier) = state.lookup("ann").context("ann")?;
    assert_eq!(id, 1);
    assert_eq!(verifier.salt(), &[7; 16]);
    assert_eq!(state.lookup("bob").map(|(bob, _)| bob), Some(2));
    assert!(state.lookup("carol").is_none());
    Ok(())
}

/// Runs a SCRAM exchange of `name` and `password` against `state`; the principal authenticated, or `None`.
fn authenticate(state: &MetadataState, name: &str, password: &str) -> TestResult<Option<u64>> {
    let mut client = ScramClient::new(name, password, "clientnonce")?;
    let mut server = ScramServer::new("servernonce", [1; 32]).with_iterations(MIN_ITERATIONS);
    let server_first = server.first(&client.first_message()?, |n| state.lookup(n))?;
    let client_final = client.final_message(&server_first)?;
    Ok(server
        .last(&client_final)
        .ok()
        .map(|(_, principal)| principal))
}

#[test]
fn test_a_scram_exchange_authenticates_against_the_state() -> TestResult {
    let state = holding_ann("ann's own password")?;
    assert_eq!(authenticate(&state, "ann", "ann's own password")?, Some(1));
    assert_eq!(authenticate(&state, "ann", "a wrong password")?, None);
    assert_eq!(authenticate(&state, "carol", "ann's own password")?, None);
    Ok(())
}
