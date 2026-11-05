/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_config::{CONFIG_LOG_CLUSTER, ConfigApi, ConfigStore, store_owner};
use ultragraphdb_common_config_check::{Defect, TargetFiles, check_central, check_config_service};
use ultragraphdb_common_service::stop_flag;
use ultragraphdb_common_service_config::{EndpointSecurity, ServiceConfig};
use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, init};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};
use ultragraphdb_transport_http_server::{BearerKey, HttpLimits, serve_listener};
use ultragraphdb_transport_tls::{CertificateChainFile, PrivateKeyFile, TlsAcceptor};

/// The most a request holds: a revision fits one round of the log.
const REQUEST_BYTES: usize = 512 * 1024;

/// How long a request has.
const REQUEST_TIME: Duration = Duration::from_secs(5);

/// How often the service looks at whether a signal arrived.
const SIGNAL_POLL: Duration = Duration::from_millis(20);

/// What the service starts from.
struct ServiceStart {
    address: Address,
    security: EndpointSecurity,
    data_dir: PathBuf,
    read_key: BearerKey,
    admin_key: BearerKey,
    targets: Vec<TargetFiles>,
}

/// Runs the config service named by `args`, the program's name left out: `--config <file>`. Everything is read and
/// validated before anything is stored or served, and it runs until SIGTERM or SIGINT.
///
/// # Errors
///
/// [`RoleError::configuration`] listing every defect of its settings, its central file or an active revision;
/// [`RoleError::unavailable`] when the data directory is held by another process or the address cannot be listened at;
/// [`RoleError::io`] when the log cannot be opened or read.
pub(crate) fn run_config_service(args: &[String]) -> Result<(), RoleError> {
    let start = read_start(args)?;
    let _telemetry = init(&TelemetryConfig::new("info", LogFormat::Plain))
        .map_err(|error| RoleError::unavailable(format!("telemetry: {error}")))?;
    let stop = stop_flag()?;
    let runtime =
        Runtime::new().map_err(|error| RoleError::unavailable(format!("the runtime: {error}")))?;
    runtime.block_on(serve(start, &stop))
}

fn listing(defects: &[Defect]) -> String {
    defects
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_start(args: &[String]) -> Result<ServiceStart, RoleError> {
    let [flag, file] = args else {
        return Err(RoleError::configuration(
            "the arguments are --config <file>",
        ));
    };
    if flag != "--config" {
        return Err(RoleError::configuration(
            "the arguments are --config <file>",
        ));
    }
    let bytes = std::fs::read(file)
        .map_err(|error| RoleError::configuration(format!("{file} cannot be read: {error}")))?;
    let defects = check_config_service(file, &bytes);
    if !defects.is_empty() {
        return Err(RoleError::configuration(listing(&defects)));
    }
    let config = ServiceConfig::parse(&bytes)
        .map_err(|error| RoleError::configuration(format!("{file}: {error}")))?;
    let table = config.config();
    let setting = |value: Option<&str>, name: &str| {
        value.map(str::to_string).ok_or_else(|| {
            RoleError::configuration(format!("{file}: setting {name}: it is absent"))
        })
    };
    let address = setting(table.address(), "config.address")?;
    let address = Address::parse(&address).map_err(|error| {
        RoleError::configuration(format!("{file}: setting config.address: {error}"))
    })?;
    let security = table
        .security()
        .map_err(|error| RoleError::configuration(format!("{file}: {error}")))?;
    let key = |name: &str, path: String| {
        BearerKey::read(&path)
            .map_err(|error| RoleError::configuration(format!("setting {name}: {error}")))
    };
    let read_key = key(
        "config.read_key_file",
        setting(table.read_key_file(), "config.read_key_file")?,
    )?;
    let admin_key = key(
        "config.admin_key_file",
        setting(table.admin_key_file(), "config.admin_key_file")?,
    )?;
    let central = PathBuf::from(setting(table.targets_file(), "config.targets_file")?);
    let targets = read_targets(&central)?;
    Ok(ServiceStart {
        address,
        security,
        data_dir: PathBuf::from(setting(table.data_dir(), "config.data_dir")?),
        read_key,
        admin_key,
        targets,
    })
}

/// The central file's targets, whole and passing, or every defect at once.
fn read_targets(central: &Path) -> Result<Vec<TargetFiles>, RoleError> {
    let report = check_central(central);
    if !report.passes() {
        return Err(RoleError::configuration(listing(report.defects())));
    }
    for warning in report.warnings() {
        eprintln!("warning: {warning}");
    }
    ultragraphdb_common_config_check::read_central(central)
        .map_err(|defects| RoleError::configuration(listing(&defects)))
}

/// Holds `directory` for this process's life.
fn lock(directory: &Path) -> Result<File, RoleError> {
    let unavailable = |error: &dyn std::fmt::Display| {
        RoleError::io(format!(
            "the data directory {}: {error}",
            directory.display()
        ))
    };
    std::fs::create_dir_all(directory).map_err(|error| unavailable(&error))?;
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("lock"))
        .map_err(|error| unavailable(&error))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(RoleError::unavailable(format!(
            "another process holds the data directory {}: stop it, or name another data_dir. A supervisor killed outright leaves its config service running",
            directory.display()
        ))),
        Err(TryLockError::Error(error)) => Err(unavailable(&error)),
    }
}

async fn serve(start: ServiceStart, stop: &AtomicBool) -> Result<(), RoleError> {
    let _lock = lock(&start.data_dir)?;
    let failed = |what: &str, error: &dyn std::fmt::Display| {
        RoleError::io(format!(
            "{what} under {}: {error}",
            start.data_dir.display()
        ))
    };
    // No drive here is assumed to keep what it acknowledged through a power loss.
    let (log, _) = Wal::open(
        CompioStorage,
        start.data_dir.join("wal"),
        WalConfig::new(CONFIG_LOG_CLUSTER, false),
    )
    .await
    .map_err(|error| failed("the log", &error))?;
    let mut store = ConfigStore::recover(log)
        .await
        .map_err(|error| failed("the log", &error))?;
    let defects = store.active_defects();
    if !defects.is_empty() {
        return Err(RoleError::configuration(listing(&defects)));
    }
    let differing = store
        .seed(&start.targets)
        .await
        .map_err(|error| failed("the log", &error))?;
    for (target, active) in differing {
        tracing::info!(
            %target,
            ?active,
            "the declared files differ from the active revision, which stays: the API is the authority, and GET /v1/targets/{target}/download returns what it holds"
        );
    }
    let tls = match &start.security {
        EndpointSecurity::Plaintext => None,
        EndpointSecurity::Tls {
            cert_file,
            key_file,
        } => Some(
            TlsAcceptor::from_pem_files(
                &CertificateChainFile::new(cert_file.clone()),
                &PrivateKeyFile::new(key_file.clone()),
            )
            .map_err(|error| {
                RoleError::configuration(format!("the setting config.tls_cert_file: {error}"))
            })?,
        ),
    };
    let listener = Listener::bind(&start.address).await.map_err(|error| {
        RoleError::unavailable(format!("listening at {}: {error}", start.address))
    })?;
    let (handle, owner) = store_owner(store);
    spawn(owner).detach();
    let api = ConfigApi::new(handle, start.read_key, start.admin_key);
    spawn(serve_listener(
        listener,
        tls,
        HttpLimits::new(REQUEST_BYTES, REQUEST_TIME),
        api,
    ))
    .detach();
    tracing::info!(address = %start.address, "serving");
    while !stop.load(Ordering::Acquire) {
        compio::time::sleep(SIGNAL_POLL).await;
    }
    tracing::info!("stopping");
    Ok(())
}
