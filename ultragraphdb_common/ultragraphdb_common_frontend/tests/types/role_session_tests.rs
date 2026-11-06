/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{OwnerStart, RoleLinks, RoleSession, RoleShared, start_owner};
use ultragraphdb_common_role::{
    ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, listen, serve_connection,
};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_protocol_transport::Endpoint;
use ultragraphdb_protocol_wire::{BodyView, Command, ErrorCode, ErrorReplyView, Subscribe};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const CLUSTER: u64 = 7;

fn directory(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("role-session-{name}-{}", std::process::id()))
}

/// A subscription whose next close the sequencer no longer holds is ended with Error 1 on its stream, so its storage process
/// learns it is behind instead of waiting on a stream that carries nothing again (`PROTOCOL.md` §13.3).
#[test]
fn test_a_subscription_past_the_closes_held_ends_with_an_error() -> TestResult {
    let data = directory("trimmed");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    std::fs::create_dir_all(&data)?;
    Runtime::new()?.block_on(async {
        let retained = NonZeroU32::new(2).context("two closes")?;
        let sequencer = SequencerConfig::new(1, 5)?.with_closes_retained(retained);
        let start = OwnerStart::new(MetadataConfig::new(), sequencer, 1);
        let owner = start_owner(
            ServiceConfig::default(),
            CLUSTER,
            data.clone(),
            start,
            Vec::new(),
        )
        .await?;
        let handle = owner.handle().clone();
        let mut after = 0;
        while after < 8 {
            after = handle
                .next_close(after)
                .await
                .map_err(|refusal| TestError::new(refusal.message().to_string()))?
                .epoch()
                .get();
        }
        let file = data.join("role-secret");
        std::fs::write(&file, "a role secret")?;
        let secret = RoleSecret::read(&file.display().to_string())?;
        let cluster = RoleCluster::new(CLUSTER, secret.clone(), Duration::from_secs(1));
        let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
        let shared = RoleShared::new(cluster, handle, links, Readiness::default());
        let server =
            Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
        let credentials = secret.credentials()?;
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
        let address = listener.local_address()?;
        spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let (server, credentials) = (server.clone(), credentials.clone());
                let session = RoleSession::new(shared.clone());
                spawn(
                    async move { serve_connection(carrier, &server, credentials, session).await },
                )
                .detach();
            }
        })
        .detach();
        let mut storage = RoleClient::dial(&address, CLUSTER, "storage", secret.password()).await?;
        storage.send_only(&Subscribe::new(1)).await?;
        let frame = compio::time::timeout(Duration::from_secs(2), storage.next_frame())
            .await
            .map_err(|_| TestError::new("no frame came on the subscription within 2 s"))??;
        let view = frame.view()?;
        assert_eq!(view.command(), Command::Error);
        assert_eq!(view.header().stream(), storage.last_stream());
        assert_eq!(
            ErrorReplyView::from_frame(&view)?.code(),
            ErrorCode::Protocol
        );
        Ok(())
    })
}
