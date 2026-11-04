/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{
    AskedCredentials, ClientSession, OwnerHandle, OwnerStart, start_owner,
};
use ultragraphdb_common_role::{RoleClient, Server, listen, serve_connection};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_protocol_scram::{Credentials, ScramVerifier};
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportErrorEnum};
use ultragraphdb_protocol_wire::{ErrorCode, Members};
use ultragraphdb_tool_bdd::{Context, TestResult};

const CLUSTER: u64 = 7;
const ITERATIONS: u32 = 4_096;

fn directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("client-session-{name}-{}", std::process::id()))
}

/// A client endpoint over `owner` at a free port; its address.
async fn endpoint(owner: OwnerHandle) -> TestResult<Address> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
    let address = listener.local_address()?;
    let server = Server::new(CLUSTER, "test", Endpoint::Client)?.with_iterations(ITERATIONS);
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let credentials = AskedCredentials::default();
            let session = ClientSession::new(owner.clone(), credentials.clone());
            let server = server.clone();
            spawn(async move { serve_connection(carrier, &server, credentials, session).await })
                .detach();
        }
    })
    .detach();
    Ok(address)
}

/// A principal the owner holds authenticates on the client endpoint by the verifier it asks the owner for; a wrong
/// password and an unknown name are refused alike, and a request past authentication is not served yet.
#[test]
fn test_authentication_asks_the_owner_for_the_verifier() -> TestResult {
    let data = directory("verifier");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let address = endpoint(owner.handle().clone()).await?;
        let mut alice = RoleClient::dial(&address, CLUSTER, "alice", "pw").await?;
        let asked = alice
            .request(&Members::new())
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(
                &asked.0,
                TransportErrorEnum::Refused {
                    code: ErrorCode::Protocol,
                    ..
                }
            ),
            "a role command is not served at the client endpoint: {asked}"
        );
        for (name, password) in [("alice", "wrong"), ("nobody", "pw")] {
            let refused = RoleClient::dial(&address, CLUSTER, name, password)
                .await
                .err()
                .context("a refusal")?;
            let code = if let TransportErrorEnum::Refused { code, .. } = &refused.0 {
                Some(*code)
            } else {
                None
            };
            assert_eq!(
                code,
                Some(ErrorCode::AuthenticationFailed),
                "{name}: {refused}"
            );
        }
        Ok(())
    })
}

/// What the owner answered reaches the machine for that name alone, once: nothing of it stays on the connection.
#[test]
fn test_an_answer_serves_its_own_name_once() -> TestResult {
    let credentials = AskedCredentials::default();
    let verifier = ScramVerifier::derive("pw", b"salt", ITERATIONS)?;
    credentials.answered("alice".to_string(), Some((7, verifier.clone())));
    assert!(credentials.lookup("bob").is_none());
    assert!(
        credentials.lookup("alice").is_none(),
        "the wrong name spent it"
    );
    credentials.answered("alice".to_string(), Some((7, verifier)));
    let (principal, _) = credentials.lookup("alice").context("the answer")?;
    assert_eq!(principal, 7);
    assert!(credentials.lookup("alice").is_none());
    credentials.answered("alice".to_string(), None);
    assert!(credentials.lookup("alice").is_none());
    Ok(())
}
