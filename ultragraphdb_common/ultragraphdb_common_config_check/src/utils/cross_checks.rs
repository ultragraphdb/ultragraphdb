/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_service_config::{ServiceConfig, partition_address};
use ultragraphdb_protocol_transport::Address;

use crate::types::defect::Defect;
use crate::types::instance::Instance;
use crate::types::target_files::TargetFiles;
use crate::utils::merge::instances;

/// One check's view of a target: its name and its parsed instances.
struct Checking<'a> {
    target: &'a TargetFiles,
    instances: Vec<Instance>,
    defects: Vec<Defect>,
}

impl Checking<'_> {
    fn report(
        &mut self,
        instance: &str,
        setting: &str,
        found: String,
        required: impl Into<String>,
    ) {
        self.defects
            .push(self.located(instance, setting, found, required));
    }

    /// A defect of `setting` of `instance`, standing in the file that sets it: the instance's own, else its role's.
    fn located(
        &self,
        instance: &str,
        setting: &str,
        found: String,
        required: impl Into<String>,
    ) -> Defect {
        let (table, key) = setting.split_once('.').unwrap_or(("", setting));
        let role = crate::utils::merge::role_of(instance).unwrap_or_default();
        let holder = [instance, role].into_iter().find(|name| {
            self.target
                .bytes(name)
                .and_then(|bytes| ultragraphdb_tool_toml::TomlDocument::parse(bytes).ok())
                .is_some_and(|document| document.line_of(table, key).is_some())
        });
        let defect = Defect::new(found, required)
            .of_target(self.target.name())
            .of_instance(instance)
            .of_setting(setting);
        let Some(name) = holder else {
            return match self
                .target
                .label(instance)
                .or_else(|| self.target.label(role))
            {
                Some(label) => defect.in_file(label),
                None => defect,
            };
        };
        let line = self
            .target
            .bytes(name)
            .and_then(|bytes| ultragraphdb_tool_toml::TomlDocument::parse(bytes).ok())
            .and_then(|document| document.line_of(table, key));
        defect
            .in_file(self.target.label(name).unwrap_or(name))
            .on_line(line)
    }
}

/// What only the merged settings, or the set of them, can show.
pub(crate) fn cross_checks(target: &TargetFiles, active: Option<&TargetFiles>) -> Vec<Defect> {
    let mut checking = Checking {
        target,
        instances: instances(target),
        defects: Vec::new(),
    };
    let held: Vec<Instance> = checking.instances.clone();
    roles(&mut checking, &held);
    for instance in &held {
        if let Some(config) = instance.config() {
            required(&mut checking, instance, config);
            bounds(&mut checking, instance, config);
            security(&mut checking, instance, config);
            secrets(&mut checking, instance, config);
        }
    }
    identity(&mut checking, &held);
    frontend_address(&mut checking, &held);
    partitions(&mut checking, &held);
    addresses(&mut checking, &held);
    data_dirs(&mut checking, &held);
    relations(&mut checking, &held);
    if let Some(active) = active {
        fixed(&mut checking, &held, active);
    }
    checking.defects
}

fn roles(checking: &mut Checking<'_>, held: &[Instance]) {
    let count = |role: &str| {
        held.iter()
            .filter(|instance| instance.role() == role)
            .count()
    };
    let name = checking.target.name().to_string();
    let frontends = count("frontend");
    if frontends != 1 {
        checking.defects.push(
            Defect::new(
                format!("the target has {frontends} frontends"),
                "a target has exactly one frontend in this release",
            )
            .of_target(&name),
        );
    }
    if count("storage") == 0 {
        checking.defects.push(
            Defect::new(
                "the target has no storage instance",
                "a target has at least one",
            )
            .of_target(&name),
        );
    }
}

/// A role's required settings: each `(setting, value)` whose value is absent is a defect.
fn required(checking: &mut Checking<'_>, instance: &Instance, config: &ServiceConfig) {
    let cluster = config.cluster();
    let mut needed: Vec<(&str, bool)> = vec![
        ("cluster.id", cluster.id().is_some()),
        (
            "cluster.role_secret_file",
            cluster.role_secret_file().is_some(),
        ),
        (
            "observability.address",
            config.observability().address().is_some(),
        ),
    ];
    match instance.role() {
        "frontend" => needed.extend([
            (
                "cluster.partition_count",
                cluster.partition_count().is_some(),
            ),
            (
                "frontend.role_address",
                config.frontend().role_address().is_some(),
            ),
            ("frontend.data_dir", config.frontend().data_dir().is_some()),
            (
                "sequencer.epoch_ms_min",
                config.sequencer().epoch_ms_min().is_some(),
            ),
            (
                "sequencer.epoch_ms_max",
                config.sequencer().epoch_ms_max().is_some(),
            ),
        ]),
        "storage" => needed.extend([
            (
                "cluster.frontend_address",
                cluster.frontend_address().is_some(),
            ),
            (
                "storage.role_address",
                config.storage().role_address().is_some(),
            ),
            ("storage.data_dir", config.storage().data_dir().is_some()),
            (
                "storage.partition_count",
                config.storage().partition_count().is_some(),
            ),
        ]),
        _ => needed.extend([
            (
                "cluster.frontend_address",
                cluster.frontend_address().is_some(),
            ),
            (
                "compute.role_address",
                config.compute().role_address().is_some(),
            ),
        ]),
    }
    for (setting, present) in needed {
        if !present {
            let role = instance.role();
            checking.report(
                instance.name(),
                setting,
                "it is absent".to_string(),
                format!("a {role} requires it"),
            );
        }
    }
}

/// Each bound, by the domain type that states it.
fn bounds(checking: &mut Checking<'_>, instance: &Instance, config: &ServiceConfig) {
    let name = instance.name();
    if instance.role() == "frontend" {
        let table = config.sequencer();
        if let (Some(min), Some(max)) = (table.epoch_ms_min(), table.epoch_ms_max())
            && let Err(error) = SequencerConfig::new(min, max)
        {
            checking.report(
                name,
                "sequencer.epoch_ms_max",
                format!("{min} to {max} ms is refused"),
                error.to_string(),
            );
        }
        metadata_bounds(checking, name, config);
        if let Some(count) = config.cluster().partition_count()
            && !(1..=PlacementMap::PARTITIONS_MAX).contains(&count)
        {
            checking.report(
                name,
                "cluster.partition_count",
                format!("it holds {count}"),
                format!("it must lie from 1 to {}", PlacementMap::PARTITIONS_MAX),
            );
        }
    }
    if instance.role() == "storage" && config.storage().partition_count() == Some(0) {
        checking.report(
            name,
            "storage.partition_count",
            "it holds 0".to_string(),
            "it must be at least 1",
        );
    }
    no_time_bounds(checking, instance, config);
    if let Some(address) = config.observability().address()
        && address
            .strip_prefix("tcp://")
            .and_then(|target| target.parse::<std::net::SocketAddr>().ok())
            .is_none()
    {
        checking.report(
            name,
            "observability.address",
            format!("it holds {address}"),
            "it must be tcp://<ip>:<port>",
        );
    }
    for (setting, address) in role_addresses(instance, config) {
        if let Err(error) = Address::parse(address) {
            checking.report(
                name,
                setting,
                format!("it holds {address}"),
                error.to_string(),
            );
        }
    }
}

/// The settings a value of 0 leaves with no time or room to act: a Scan's stall, a storage process's retention and the
/// bytes a frontend's reads hold back. A frontend bounds a hung compute process by its own `scan_stall_ms`.
fn no_time_bounds(checking: &mut Checking<'_>, instance: &Instance, config: &ServiceConfig) {
    let (storage, frontend) = (instance.role() == "storage", instance.role() == "frontend");
    let settings = [
        (
            storage || frontend,
            "storage.scan_stall_ms",
            config.storage().scan_stall_ms(),
        ),
        (
            storage,
            "storage.read_retain_ms",
            config.storage().read_retain_ms(),
        ),
        (
            frontend,
            "endpoint.held_bytes",
            config.endpoint().held_bytes(),
        ),
    ];
    for (_, setting, _) in settings
        .iter()
        .filter(|(applies, _, value)| *applies && *value == Some(0))
    {
        checking.report(
            instance.name(),
            setting,
            "it holds 0".to_string(),
            "it must be at least 1",
        );
    }
}

fn metadata_bounds(checking: &mut Checking<'_>, name: &str, config: &ServiceConfig) {
    let table = config.metadata();
    let base = MetadataConfig::new();
    if let Some(size) = table.block_size_max()
        && let Err(error) = base.with_block_size_max(size)
    {
        checking.report(
            name,
            "metadata.block_size_max",
            format!("it holds {size}"),
            error.to_string(),
        );
    }
    if let Some(sessions) = table.sessions_max()
        && let Err(error) = base.with_sessions_max(sessions)
    {
        checking.report(
            name,
            "metadata.sessions_max",
            format!("it holds {sessions}"),
            error.to_string(),
        );
    }
    if let Some(rounds) = table.snapshot_rounds()
        && let Err(error) = base.with_snapshot_rounds(rounds)
    {
        checking.report(
            name,
            "metadata.snapshot_rounds",
            format!("it holds {rounds}"),
            error.to_string(),
        );
    }
}

/// The addresses others reach `instance` at, by the setting that names each.
fn role_addresses<'a>(
    instance: &Instance,
    config: &'a ServiceConfig,
) -> Vec<(&'static str, &'a str)> {
    let mut held = Vec::new();
    if let Some(address) = config.cluster().frontend_address() {
        held.push(("cluster.frontend_address", address));
    }
    let own = match instance.role() {
        "frontend" => config
            .frontend()
            .role_address()
            .map(|a| ("frontend.role_address", a)),
        "storage" => config
            .storage()
            .role_address()
            .map(|a| ("storage.role_address", a)),
        _ => config
            .compute()
            .role_address()
            .map(|a| ("compute.role_address", a)),
    };
    held.extend(own);
    held
}

fn security(checking: &mut Checking<'_>, instance: &Instance, config: &ServiceConfig) {
    let name = instance.name();
    let required_text = "name tls_cert_file and tls_key_file, or set plaintext = true";
    if config.endpoint().address().is_some()
        && let Err(error) = config.endpoint().security()
    {
        checking.report(name, "endpoint.plaintext", error.to_string(), required_text);
    }
    if config.admin().address().is_some() {
        if let Err(error) = config.admin().security() {
            checking.report(name, "admin.plaintext", error.to_string(), required_text);
        }
        if config.admin().key_sha256().is_none() {
            checking.report(
                name,
                "admin.key_sha256",
                "it is absent".to_string(),
                "an admin listener requires the admin key's digest",
            );
        }
    }
}

fn secrets(checking: &mut Checking<'_>, instance: &Instance, config: &ServiceConfig) {
    let name = instance.name();
    let paths = [
        (
            "cluster.role_secret_file",
            config.cluster().role_secret_file(),
        ),
        ("admin.tls_cert_file", config.admin().tls_cert_file()),
        ("admin.tls_key_file", config.admin().tls_key_file()),
    ];
    for (setting, path) in paths {
        if let Some(path) = path
            && !path.starts_with('/')
        {
            checking.report(
                name,
                setting,
                format!("it holds {path}"),
                "it must be an absolute path",
            );
        }
    }
    if let Some(digest) = config.admin().key_sha256()
        && !(digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        checking.report(
            name,
            "admin.key_sha256",
            "it is not a SHA-256 digest".to_string(),
            "it must be 64 hexadecimal digits",
        );
    }
}

fn identity(checking: &mut Checking<'_>, held: &[Instance]) {
    let mut first: Option<(&str, u64)> = None;
    for instance in held {
        let Some(id) = instance.config().and_then(|config| config.cluster().id()) else {
            continue;
        };
        match first {
            None => first = Some((instance.name(), id)),
            Some((holder, expected)) if id != expected => checking.report(
                instance.name(),
                "cluster.id",
                format!("it holds {id}, and {holder} holds {expected}"),
                "every instance of a target names one cluster",
            ),
            Some(_) => {}
        }
    }
}

/// The scheme and port, or the Unix path, a role address reaches.
fn reach(address: &str) -> String {
    match address.strip_prefix("tcp://") {
        Some(target) => format!("tcp port {}", target.rsplit(':').next().unwrap_or_default()),
        None => address.to_string(),
    }
}

fn frontend_address(checking: &mut Checking<'_>, held: &[Instance]) {
    let Some(role_address) = held
        .iter()
        .find(|instance| instance.role() == "frontend")
        .and_then(Instance::config)
        .and_then(|config| config.frontend().role_address())
        .map(str::to_string)
    else {
        return;
    };
    for instance in held.iter().filter(|instance| instance.role() != "frontend") {
        if let Some(address) = instance
            .config()
            .and_then(|config| config.cluster().frontend_address())
            && reach(address) != reach(&role_address)
        {
            checking.report(
                instance.name(),
                "cluster.frontend_address",
                format!("it holds {address}, and the frontend listens at {role_address}"),
                "it must reach the frontend's role address: the same scheme and port, or the same Unix path",
            );
        }
    }
}

/// Each storage instance's partitions.
fn partitions_of(config: &ServiceConfig) -> Vec<u32> {
    let first = config.storage().partition_first().unwrap_or(1);
    let count = config.storage().partition_count().unwrap_or(0);
    (0..count).filter_map(|at| first.checked_add(at)).collect()
}

fn partitions(checking: &mut Checking<'_>, held: &[Instance]) {
    let Some(total) = held
        .iter()
        .find(|instance| instance.role() == "frontend")
        .and_then(Instance::config)
        .and_then(|config| config.cluster().partition_count())
    else {
        return;
    };
    // An instance whose file is refused holds partitions no one can read, so coverage would accuse the rest.
    if held
        .iter()
        .any(|instance| instance.role() == "storage" && instance.config().is_none())
    {
        return;
    }
    let mut holders: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    for instance in held.iter().filter(|instance| instance.role() == "storage") {
        if let Some(config) = instance.config() {
            for partition in partitions_of(config) {
                holders.entry(partition).or_default().push(instance.name());
            }
        }
    }
    for (partition, names) in &holders {
        if let [first, second, ..] = names.as_slice() {
            checking.report(
                second,
                "storage.partition_first",
                format!("partition {partition} is held by both {first} and {second}"),
                "each partition is held by one storage instance",
            );
        }
        if *partition > total {
            checking.report(
                names.first().copied().unwrap_or_default(),
                "storage.partition_count",
                format!("partition {partition} lies outside the cluster's {total}"),
                "the storage instances hold partitions 1 to cluster.partition_count",
            );
        }
    }
    let name = checking.target.name().to_string();
    for partition in 1..=total.min(PlacementMap::PARTITIONS_MAX) {
        if !holders.contains_key(&partition) {
            checking.defects.push(
                Defect::new(
                    format!("no storage instance holds partition {partition}"),
                    "the storage instances hold partitions 1 to cluster.partition_count, each once",
                )
                .of_target(&name),
            );
        }
    }
}

/// Whether `address` listens on every interface.
fn is_wildcard(address: &str) -> bool {
    address.starts_with("tcp://0.0.0.0:") || address.starts_with("tcp://[::]:")
}

/// Every listener of `instance`, a partition's derived address included, with the setting behind it.
fn listeners(instance: &Instance, config: &ServiceConfig) -> Vec<(&'static str, String)> {
    let mut held: Vec<(&'static str, String)> = Vec::new();
    let mut add = |setting: &'static str, address: Option<&str>| {
        if let Some(address) = address {
            held.push((setting, address.to_string()));
        }
    };
    add("observability.address", config.observability().address());
    add("admin.address", config.admin().address());
    match instance.role() {
        "frontend" => {
            add("frontend.role_address", config.frontend().role_address());
            add("endpoint.address", config.endpoint().address());
        }
        "compute" => add("compute.role_address", config.compute().role_address()),
        _ => {
            if let Some(base) = config.storage().role_address() {
                let first = config.storage().partition_first().unwrap_or(1);
                for partition in partitions_of(config) {
                    if let Ok(derived) = partition_address(base, first, partition) {
                        held.push(("storage.role_address", derived));
                    }
                }
            }
        }
    }
    held
}

fn addresses(checking: &mut Checking<'_>, held: &[Instance]) {
    let mut taken: BTreeMap<String, (String, &'static str)> = BTreeMap::new();
    for instance in held {
        let Some(config) = instance.config() else {
            continue;
        };
        for (setting, address) in listeners(instance, config) {
            if is_wildcard(&address) && setting.ends_with("role_address") {
                checking.report(
                    instance.name(),
                    setting,
                    format!("it holds {address}, which no peer can dial"),
                    "a role address names a host its peers can reach",
                );
            }
            // A wildcard is compared within one instance, since each pod holds its own interfaces.
            let scope = if is_wildcard(&address) {
                format!("{}/{address}", instance.name())
            } else {
                address.clone()
            };
            match taken.get(&scope) {
                Some((holder, other)) => checking.report(
                    instance.name(),
                    setting,
                    format!("{address} is also {holder}'s {other}"),
                    "two listeners cannot hold one address",
                ),
                None => {
                    let _none = taken.insert(scope, (instance.name().to_string(), setting));
                }
            }
        }
    }
}

/// The host `instance` names, or `unix` for one on Unix sockets, which share a machine.
fn host_of(instance: &Instance, config: &ServiceConfig) -> Option<String> {
    let (_, address) = listeners(instance, config)
        .into_iter()
        .find(|(setting, _)| setting.ends_with("role_address"))?;
    match address.strip_prefix("tcp://") {
        Some(target) => target.rsplit_once(':').map(|(host, _)| host.to_string()),
        None => Some("unix".to_string()),
    }
}

/// The instance's `data_dir`.
fn data_dir_of<'a>(instance: &Instance, config: &'a ServiceConfig) -> Option<&'a str> {
    match instance.role() {
        "frontend" => config.frontend().data_dir(),
        "storage" => config.storage().data_dir(),
        _ => None,
    }
}

fn data_dirs(checking: &mut Checking<'_>, held: &[Instance]) {
    let mut taken: BTreeMap<(String, String), String> = BTreeMap::new();
    for instance in held {
        let Some(config) = instance.config() else {
            continue;
        };
        let (Some(host), Some(directory)) =
            (host_of(instance, config), data_dir_of(instance, config))
        else {
            continue;
        };
        let key = (host, directory.to_string());
        match taken.get(&key) {
            Some(holder) => checking.report(
                instance.name(),
                &format!("{}.data_dir", instance.role()),
                format!("{directory} is also {holder}'s, on the same host"),
                "two instances on one host hold their own data directories",
            ),
            None => {
                let _none = taken.insert(key, instance.name().to_string());
            }
        }
    }
}

fn relations(checking: &mut Checking<'_>, held: &[Instance]) {
    let Some(epoch_ms_max) = held
        .iter()
        .find(|instance| instance.role() == "frontend")
        .and_then(Instance::config)
        .and_then(|config| config.sequencer().epoch_ms_max())
    else {
        return;
    };
    for instance in held.iter().filter(|instance| instance.role() == "storage") {
        if let Some(close_ms) = instance
            .config()
            .and_then(|config| config.storage().close_request_ms())
            && close_ms < epoch_ms_max.saturating_mul(2)
        {
            checking.report(
                instance.name(),
                "storage.close_request_ms",
                format!("it holds {close_ms}"),
                format!(
                    "it must be at least twice sequencer.epoch_ms_max, {}",
                    epoch_ms_max.saturating_mul(2)
                ),
            );
        }
    }
}

fn fixed(checking: &mut Checking<'_>, held: &[Instance], active: &TargetFiles) {
    let before = instances(active);
    for instance in held {
        let Some(config) = instance.config() else {
            continue;
        };
        let was = before
            .iter()
            .find(|old| old.name() == instance.name())
            .and_then(Instance::config);
        let id = before.iter().find_map(|old| {
            old.config()
                .and_then(|held_config| held_config.cluster().id())
        });
        if let (Some(id), Some(now)) = (id, config.cluster().id())
            && id != now
        {
            checking.report(
                instance.name(),
                "cluster.id",
                format!("it holds {now}, and the active revision holds {id}"),
                "a cluster's identity does not change",
            );
        }
        let Some(was) = was else {
            continue;
        };
        if was.cluster().partition_count() != config.cluster().partition_count() {
            checking.report(
                instance.name(),
                "cluster.partition_count",
                "it differs from the active revision".to_string(),
                "the placement is fixed in this release",
            );
        }
        if partitions_of(was) != partitions_of(config) && instance.role() == "storage" {
            checking.report(
                instance.name(),
                "storage.partition_first",
                "the instance's partitions differ from the active revision".to_string(),
                "the placement is fixed in this release",
            );
        }
        if data_dir_of(instance, was) != data_dir_of(instance, config) {
            checking.report(
                instance.name(),
                &format!("{}.data_dir", instance.role()),
                "it differs from the active revision".to_string(),
                "an instance's data stays where it is",
            );
        }
    }
}
