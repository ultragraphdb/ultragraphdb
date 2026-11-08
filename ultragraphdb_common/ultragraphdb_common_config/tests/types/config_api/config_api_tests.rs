/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The HTTP API over a loopback socket, its store owned by a task on the same runtime, over the write-ahead log on its
//! simulated storage.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use compio::BufResult;
use compio::io::{AsyncReadExt, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_config::{
    CONFIG_LOG_CLUSTER, ConfigApi, ConfigError, ConfigStore, store_owner,
};
use ultragraphdb_common_config_check::TargetFiles;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{FaultMode, SimStorage, Wal, WalConfig};
use ultragraphdb_transport_http_server::{BearerKey, HttpLimits, serve_http};

const READ_KEY: &str = "read-key-0123456789abcdef01234567";
const ADMIN_KEY: &str = "admin-key-0123456789abcdef0123456";

const FRONTEND: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\npartition_count = 1\n[observability]\naddress = \"tcp://127.0.0.1:18081\"\n[frontend]\nrole_address = \"tcp://10.0.0.1:7081\"\ndata_dir = \"/data\"\n[sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n";
const STORAGE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[storage]\npartition_count = 1\ndata_dir = \"/data\"\nrole_address = \"tcp://10.0.0.2:7000\"\n";

/// How long a client waits on an answer: far past what a loopback takes.
const WAIT: Duration = Duration::from_secs(5);

/// A key file holding `key`, in a folder `label` names, which no other test names.
fn key(label: &str, name: &str, key: &str) -> TestResult<BearerKey> {
    let folder = std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!("config-api-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&folder)?;
    let file = folder.join(name);
    std::fs::write(&file, key)?;
    Ok(BearerKey::read(&file.display().to_string())?)
}

fn bundle() -> Vec<u8> {
    let mut target = TargetFiles::new("ci");
    target.insert("frontend", "frontend.toml", FRONTEND.as_bytes().to_vec());
    target.insert("storage", "storage.toml", STORAGE.as_bytes().to_vec());
    target.to_bundle()
}

/// The API served at a loopback address on this runtime, its store empty.
async fn serving(label: &str) -> TestResult<SocketAddr> {
    serving_over(label, SimStorage::new(9), true).await
}

/// The API served at a loopback address on this runtime, its empty store over `storage`, its key files in a folder
/// `label` names; its owner runs when `owned` and has ended otherwise.
async fn serving_over(label: &str, storage: SimStorage, owned: bool) -> TestResult<SocketAddr> {
    let config = WalConfig::new(CONFIG_LOG_CLUSTER, false);
    let (log, _) = Wal::open(storage, PathBuf::from("/wal"), config).await?;
    let (handle, owner) = store_owner(ConfigStore::recover(log).await?);
    if owned {
        spawn(owner).detach();
    } else {
        drop(owner);
    }
    let api = ConfigApi::new(
        handle,
        key(label, "read", READ_KEY)?,
        key(label, "admin", ADMIN_KEY)?,
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    spawn(async move {
        let limits = HttpLimits::new(64 * 1024, WAIT);
        while let Ok((stream, _)) = listener.accept().await {
            let api = api.clone();
            spawn(async move { serve_http(stream, &limits, &api).await }).detach();
        }
    })
    .detach();
    Ok(address)
}

/// The status and body of `method path` presenting `key`, with `body`.
async fn ask(
    address: SocketAddr,
    method: &str,
    path: &str,
    key: Option<&str>,
    body: &[u8],
) -> TestResult<(u16, Vec<u8>)> {
    let mut stream = TcpStream::connect(address).await?;
    let bearer = key.map_or(String::new(), |key| {
        format!("Authorization: Bearer {key}\r\n")
    });
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\n{bearer}Content-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(body);
    let BufResult(written, _) = stream.write_all(request).await;
    written?;
    let BufResult(read, answer) = compio::time::timeout(WAIT, stream.read_to_end(Vec::new()))
        .await
        .map_err(|_| TestError::new(format!("an answer to {method} {path}")))?;
    let _count: usize = read?;
    let end = answer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .context("the end of the head")?;
    let status = String::from_utf8_lossy(answer.get(9..12).context("a status")?).parse()?;
    Ok((status, answer.get(end + 4..).unwrap_or_default().to_vec()))
}

/// `/health` and `/ready` take no key; every other request takes one, and a change takes the admin key.
async fn keys(at: SocketAddr) -> TestResult {
    for path in ["/health", "/ready"] {
        assert_eq!(ask(at, "GET", path, None, b"").await?.0, 200, "{path}");
    }
    assert_eq!(ask(at, "GET", "/v1/targets", None, b"").await?.0, 401);
    assert_eq!(
        ask(at, "GET", "/v1/targets", Some(READ_KEY), b"").await?.0,
        200
    );
    let add = "/v1/targets/ci/revisions?base=0";
    assert_eq!(
        ask(at, "POST", add, Some(READ_KEY), &bundle()).await?.0,
        403
    );
    assert_eq!(
        ask(at, "POST", add, Some(ADMIN_KEY), &bundle()).await?.0,
        201
    );
    Ok(())
}

/// Each request reaches its own answer.
async fn routes(at: SocketAddr) -> TestResult {
    let read = Some(READ_KEY);
    let admin = Some(ADMIN_KEY);
    assert_eq!(
        ask(at, "PUT", "/v1/targets/ci/active", admin, b"1")
            .await?
            .0,
        200
    );
    let listed = ask(at, "GET", "/v1/targets", read, b"").await?;
    assert_eq!(listed, (200, b"ci latest 1 active 1\n".to_vec()));
    for path in [
        "/v1/targets/ci/instances/storage",
        "/v1/targets/ci/revisions/1/instances/storage",
    ] {
        let (status, body) = ask(at, "GET", path, read, b"").await?;
        assert_eq!(status, 200, "{path}");
        assert!(
            String::from_utf8(body)?.contains("partition_count = 1"),
            "{path}"
        );
    }
    for path in ["/v1/targets/ci/revisions/1", "/v1/targets/ci/download"] {
        assert_eq!(
            ask(at, "GET", path, admin, b"").await?,
            (200, bundle()),
            "{path}"
        );
    }
    assert_eq!(
        ask(at, "DELETE", "/v1/targets/ci", admin, b"").await?.0,
        200
    );
    let retired = ask(at, "GET", "/v1/targets", read, b"").await?;
    assert_eq!(retired, (200, b"ci latest 1 active 1 retired\n".to_vec()));
    Ok(())
}

/// The API answers each request by its key and its route, the store behind it.
#[test]
fn test_the_api_answers_each_request() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = serving("answers").await?;
        keys(at).await?;
        routes(at).await
    })
}

/// The answer to a refusal: its status and the error's text on a line.
fn refused(status: u16, error: &ConfigError) -> (u16, Vec<u8>) {
    (status, format!("{error}\n").into_bytes())
}

/// Each refusal of the store is answered with its status and the error's text.
async fn refusals(at: SocketAddr) -> TestResult {
    let admin = Some(ADMIN_KEY);
    let storage = "/v1/targets/ci/instances/storage";
    let unknown = ConfigError::unknown_target("ci");
    assert_eq!(
        ask(at, "GET", storage, admin, b"").await?,
        refused(404, &unknown)
    );
    let add = "/v1/targets/ci/revisions?base=0";
    assert_eq!(ask(at, "POST", add, admin, &bundle()).await?.0, 201);
    let inactive = ConfigError::no_active_revision("ci");
    for path in [storage, "/v1/targets/ci/download"] {
        assert_eq!(
            ask(at, "GET", path, admin, b"").await?,
            refused(404, &inactive),
            "{path}"
        );
    }
    let absent = ConfigError::unknown_revision("ci", 9);
    for path in [
        "/v1/targets/ci/revisions/9/instances/storage",
        "/v1/targets/ci/revisions/9",
    ] {
        assert_eq!(
            ask(at, "GET", path, admin, b"").await?,
            refused(404, &absent),
            "{path}"
        );
    }
    assert_eq!(
        ask(at, "PUT", "/v1/targets/ci/active", admin, b"9").await?,
        refused(404, &absent)
    );
    let nobody = "/v1/targets/ci/revisions/1/instances/nobody";
    assert_eq!(
        ask(at, "GET", nobody, admin, b"").await?,
        refused(404, &ConfigError::unknown_instance("ci", 1, "nobody"))
    );
    assert_eq!(
        ask(at, "POST", add, admin, &bundle()).await?,
        refused(409, &ConfigError::stale("ci", 0, 1))
    );
    let broken = "/v1/targets/ci/revisions?base=1";
    assert_eq!(
        ask(at, "POST", broken, admin, b"not a bundle").await?.0,
        422
    );
    assert_eq!(
        ask(at, "DELETE", "/v1/targets/nobody", admin, b"").await?,
        refused(404, &ConfigError::unknown_target("nobody"))
    );
    assert_eq!(
        ask(at, "DELETE", "/v1/targets/ci", admin, b"").await?.0,
        200
    );
    assert_eq!(
        ask(at, "GET", storage, admin, b"").await?,
        refused(404, &ConfigError::retired("ci"))
    );
    Ok(())
}

/// A request the API cannot read is answered 400, one it does not serve 404.
async fn malformed(at: SocketAddr) -> TestResult {
    let admin = Some(ADMIN_KEY);
    for (method, path, body) in [
        (
            "GET",
            "/v1/targets/ci/revisions/x/instances/storage",
            b"".as_slice(),
        ),
        ("GET", "/v1/targets/ci/revisions/x", b""),
        ("POST", "/v1/targets/ci/revisions", b""),
        ("PUT", "/v1/targets/ci/active", b"x"),
    ] {
        assert_eq!(
            ask(at, method, path, admin, body).await?.0,
            400,
            "{method} {path}"
        );
    }
    assert_eq!(ask(at, "GET", "/v1/nothing", admin, b"").await?.0, 404);
    Ok(())
}

/// The API refuses each request it cannot serve with that request's status and text.
#[test]
fn test_the_api_refuses_each_request_for_its_reason() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = serving("refuses").await?;
        refusals(at).await?;
        malformed(at).await
    })
}

/// A change the log fails to make durable is answered 503, the store unavailable.
#[test]
fn test_a_failed_log_is_answered_unavailable() -> TestResult {
    Runtime::new()?.block_on(async {
        let storage = SimStorage::new(10);
        let at = serving_over("failed", storage.clone(), true).await?;
        let admin = Some(ADMIN_KEY);
        let add = "/v1/targets/ci/revisions?base=0";
        assert_eq!(ask(at, "POST", add, admin, &bundle()).await?.0, 201);
        storage.fail_at(storage.trace().len(), FaultMode::Before);
        let retire = ask(at, "DELETE", "/v1/targets/ci", admin, b"").await?;
        assert_eq!(retire.0, 503, "{}", String::from_utf8_lossy(&retire.1));
        Ok(())
    })
}

/// Every request reaching the store is answered 503 once the store's owner has ended.
#[test]
fn test_a_stopped_store_answers_unavailable() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = serving_over("stopped", SimStorage::new(11), false).await?;
        let admin = Some(ADMIN_KEY);
        let bundle = bundle();
        for (method, path, body) in [
            ("GET", "/v1/targets", b"".as_slice()),
            ("GET", "/v1/targets/ci/instances/storage", b""),
            ("GET", "/v1/targets/ci/revisions/1", b""),
            ("GET", "/v1/targets/ci/download", b""),
            ("POST", "/v1/targets/ci/revisions?base=0", &bundle),
            ("PUT", "/v1/targets/ci/active", b"1"),
            ("DELETE", "/v1/targets/ci", b""),
        ] {
            assert_eq!(
                ask(at, method, path, admin, body).await?.0,
                503,
                "{method} {path}"
            );
        }
        Ok(())
    })
}
