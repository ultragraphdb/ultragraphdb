/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleClient, RoleCredentials, RoleSecret, Server, Session, listen,
    serve_connection,
};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{
    Address, Endpoint, ReceivedFrame, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, ErrorCode, ErrorReply, FLAG_LAST, Header, Join, Members,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

const CLUSTER: u64 = 7;
const PASSWORD: &str = "s3cret";

static FILES: AtomicU32 = AtomicU32::new(0);

/// Credentials of `PASSWORD`.
fn credentials() -> TestResult<RoleCredentials> {
    // Tests run side by side, so each writes a file of its own.
    let file = std::env::temp_dir().join(format!(
        "serve-secret-{}-{}",
        std::process::id(),
        FILES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&file, PASSWORD)?;
    Ok(RoleSecret::read(&file.display().to_string())?.credentials()?)
}

/// Answers a Members with no member, and refuses what else its endpoint serves as unavailable.
#[derive(Debug, Clone, Copy)]
struct Listing;

impl Session for Listing {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        if frame.command() != Command::Members.number() {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Unavailable, "no"));
        }
        let header = Header::new(CLUSTER, Command::Members)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        Members::new().frame(header).map_or_else(
            |rule| Answer::Refuse(ErrorReply::protocol(&rule)),
            Answer::Reply,
        )
    }
}

/// A server at `endpoint` listening at `text`; the address it holds.
async fn serving(text: &str, endpoint: Endpoint) -> TestResult<Address> {
    let listener = listen(text).await?;
    let address = listener.local_address()?;
    let server = Server::new(CLUSTER, "test", endpoint)?.with_iterations(ROLE_ITERATIONS);
    let held = credentials()?;
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, held) = (server.clone(), held.clone());
            spawn(async move { serve_connection(carrier, &server, held, Listing).await }).detach();
        }
    })
    .detach();
    Ok(address)
}

fn refused_with(error: &TransportError) -> Option<ErrorCode> {
    if let TransportErrorEnum::Refused { code, .. } = &error.0 {
        Some(*code)
    } else {
        None
    }
}

/// A TCP address of this host that was free a moment ago.
fn free_tcp() -> TestResult<String> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    Ok(format!("tcp://127.0.0.1:{port}"))
}

/// The socket path for `name`, short enough for the platform's limit wherever the tests run.
fn socket(name: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/ugdb-{name}-{}.sock", std::process::id()))
}

/// A role with the secret authenticates, and its requests are answered on either carrier.
#[test]
fn test_a_role_with_the_secret_exchanges_role_commands_on_either_carrier() -> TestResult {
    let unix = socket("exchange");
    Runtime::new()?.block_on(async {
        for text in [free_tcp()?, format!("unix://{}", unix.display())] {
            let address = serving(&text, Endpoint::Frontend).await?;
            let mut client = RoleClient::dial(&address, CLUSTER, "storage", PASSWORD).await?;
            let reply = client.request(&Members::new()).await?;
            assert_eq!(reply.command(), Command::Members.number(), "{text}");
            drop(reply);
            let again = client.request(&Members::new()).await?;
            assert_eq!(again.stream(), 2, "each request takes a stream of its own");
            drop(again);
            let refused = client.request(&Join::compute("tcp://c:1")).await;
            let error = refused.err().context("a refusal")?;
            assert_eq!(refused_with(&error), Some(ErrorCode::Unavailable), "{text}");
        }
        Ok(())
    })
}

/// A wrong secret and an unknown name are refused alike, and a command the endpoint does not serve by its rule.
#[test]
fn test_a_wrong_secret_or_name_is_refused_with_code_19() -> TestResult {
    Runtime::new()?.block_on(async {
        let address = serving(&free_tcp()?, Endpoint::Compute).await?;
        for (name, password) in [("storage", "wrong"), ("alice", PASSWORD)] {
            let refused = RoleClient::dial(&address, CLUSTER, name, password)
                .await
                .err()
                .context("a refusal")?;
            assert_eq!(
                refused_with(&refused),
                Some(ErrorCode::AuthenticationFailed),
                "{name}"
            );
        }
        let mut client = RoleClient::dial(&address, CLUSTER, "storage", PASSWORD).await?;
        let served = client.request(&Members::new()).await;
        let error = served.err().context("a refusal")?;
        assert_eq!(refused_with(&error), Some(ErrorCode::Protocol));
        Ok(())
    })
}

/// A peer of another cluster gets no answer it could take for its own.
#[test]
fn test_another_clusters_role_is_refused() -> TestResult {
    Runtime::new()?.block_on(async {
        let address = serving(&free_tcp()?, Endpoint::Frontend).await?;
        assert!(
            RoleClient::dial(&address, CLUSTER + 1, "storage", PASSWORD)
                .await
                .is_err()
        );
        Ok(())
    })
}

/// A role's socket is mode 0600, and a socket a dead process left is replaced; anything else at the path is left alone.
#[test]
fn test_a_socket_is_private_and_replaces_a_stale_one() -> TestResult {
    let path = socket("mode");
    let text = format!("unix://{}", path.display());
    Runtime::new()?.block_on(async {
        drop(listen(&text).await?);
        assert!(std::fs::symlink_metadata(&path)?.file_type().is_socket());
        let listener = listen(&text).await?;
        let mode = std::fs::metadata(&path)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        drop(listener);
        std::fs::remove_file(&path)?;
        std::fs::write(&path, b"not a socket")?;
        let refused = listen(&text).await.err().context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_UNAVAILABLE);
        assert_eq!(std::fs::read(&path)?, b"not a socket");
        std::fs::remove_file(&path)?;
        Ok(())
    })
}

/// An address that does not parse is the configuration's fault.
#[test]
fn test_a_malformed_address_is_a_configuration_error() -> TestResult {
    Runtime::new()?.block_on(async {
        let refused = listen("http://nowhere").await.err().context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_CONFIG);
        Ok(())
    })
}
