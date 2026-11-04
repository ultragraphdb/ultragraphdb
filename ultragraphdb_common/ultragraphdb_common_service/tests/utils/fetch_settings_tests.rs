/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use compio::net::TcpListener;
use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_service::{Bootstrap, RoleKind, fetch_settings};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_http_server::{
    HttpLimits, HttpRequest, HttpResponse, HttpService, serve_http,
};

const SETTINGS: &str = "[observability]\naddress = \"tcp://127.0.0.1:1\"\n";
const NO_PATIENCE: Duration = Duration::from_millis(1);

/// A config service answering every request with one status, naming a revision when it says to.
#[derive(Debug, Clone)]
struct Fixed {
    status: u16,
    revision: Option<&'static str>,
    body: &'static str,
}

impl HttpService for Fixed {
    async fn call(&self, _request: HttpRequest) -> HttpResponse {
        let response = HttpResponse::new(self.status).with_body(self.body);
        match self.revision {
            Some(revision) => response.with_header("X-Revision", revision),
            None => response,
        }
    }
}

/// What a role's variables say for a service at `port`.
fn bootstrap(port: u16, test: &str) -> TestResult<Bootstrap> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let key = std::path::Path::new(&root).join(format!("fetch-{test}-{}", std::process::id()));
    std::fs::write(&key, "a read key of at least thirty two bytes")?;
    Ok(Bootstrap::read(
        RoleKind::Storage,
        [
            (
                "ULTRAGRAPHDB_CONFIG_URL",
                format!("http://127.0.0.1:{port}"),
            ),
            ("ULTRAGRAPHDB_CONFIG_TARGET", "ci".to_string()),
            ("ULTRAGRAPHDB_CONFIG_INSTANCE", "storage_1".to_string()),
            ("ULTRAGRAPHDB_CONFIG_KEY_FILE", key.display().to_string()),
        ]
        .map(|(name, value)| (name.to_string(), value)),
    )?)
}

/// A port nothing listens at.
fn free_port() -> TestResult<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

/// Serves `service` at a fresh port on this runtime, its port returned.
async fn serving(service: Fixed) -> TestResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let limits = HttpLimits::new(0, Duration::from_secs(5));
            serve_http(stream, &limits, &service).await;
        }
    })
    .detach();
    Ok(port)
}

#[test]
fn test_settings_come_with_their_revision() -> TestResult {
    Runtime::new()?.block_on(async {
        let port = serving(Fixed {
            status: 200,
            revision: Some("7"),
            body: SETTINGS,
        })
        .await?;
        let read = bootstrap(port, "reads")?;
        let start = fetch_settings(&read, NO_PATIENCE, &AtomicBool::new(false))
            .await?
            .context("a start")?;
        assert_eq!(start.identity().revision(), 7);
        assert_eq!(start.identity().instance(), "storage_1");
        assert_eq!(start.identity().target(), "ci");
        assert_eq!(start.observability().port(), 1);
        Ok(())
    })
}

/// A refusal is not asked again, whatever the patience: this test would take a minute if it were.
#[test]
fn test_a_refusal_ends_the_start_at_once() -> TestResult {
    Runtime::new()?.block_on(async {
        let cases = [
            (
                Fixed {
                    status: 401,
                    revision: None,
                    body: "a key is required\n",
                },
                "401",
            ),
            (
                Fixed {
                    status: 404,
                    revision: None,
                    body: "target ci is retired\n",
                },
                "retired",
            ),
            (
                Fixed {
                    status: 200,
                    revision: None,
                    body: SETTINGS,
                },
                "no revision",
            ),
            (
                Fixed {
                    status: 200,
                    revision: Some("x"),
                    body: SETTINGS,
                },
                "no revision",
            ),
            (
                Fixed {
                    status: 200,
                    revision: Some("7"),
                    body: "[storge]\n",
                },
                "storge",
            ),
        ];
        for (at, (service, named)) in cases.into_iter().enumerate() {
            let port = serving(service).await?;
            let read = bootstrap(port, &format!("refused-{at}"))?;
            let refused = fetch_settings(&read, Duration::from_secs(60), &AtomicBool::new(false))
                .await
                .err()
                .context("a refusal")?;
            assert_eq!(refused.exit_code(), 78, "{refused}");
            let message = refused.to_string();
            assert!(message.contains(named), "{message} names {named}");
            assert!(message.contains("storage_1"), "{message}");
        }
        Ok(())
    })
}

/// An unreachable service is a lost dependency once the patience has passed, every name in the error.
#[test]
fn test_an_unreachable_service_ends_with_every_name() -> TestResult {
    Runtime::new()?.block_on(async {
        let port = free_port()?;
        let read = bootstrap(port, "unreachable")?;
        let refused = fetch_settings(&read, NO_PATIENCE, &AtomicBool::new(false))
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(refused.exit_code(), 69, "{refused}");
        let message = refused.to_string();
        for named in [port.to_string().as_str(), "ci", "storage_1"] {
            assert!(message.contains(named), "{message} names {named}");
        }
        Ok(())
    })
}

/// A signal while waiting ends the wait with no settings, and no refusal.
#[test]
fn test_a_stop_ends_the_wait() -> TestResult {
    Runtime::new()?.block_on(async {
        let read = bootstrap(free_port()?, "stopped")?;
        let stopped =
            fetch_settings(&read, Duration::from_secs(60), &AtomicBool::new(true)).await?;
        assert!(stopped.is_none());
        Ok(())
    })
}
