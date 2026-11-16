/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the property tests draw from: a target as a table of instances, each a table of settings, rendered to one file per
//! instance as a central file declares them; and the numbers and coins the laws' inputs are drawn as.

use std::collections::BTreeMap;

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_config_check::{Defect, TargetFiles, check_target};

/// The port the hosting instance's group listens at in a drawn target.
pub(crate) const SYSTEM_PORT: u64 = 7100;

/// One instance's settings: per table, each key and its value as TOML writes it, a string quoted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Settings(BTreeMap<String, BTreeMap<String, String>>);

impl Settings {
    /// Sets `table.key` to `value`, written as it stands: a number or a boolean.
    pub(crate) fn bare(&mut self, table: &str, key: &str, value: impl ToString) {
        let _previous = self
            .0
            .entry(table.to_string())
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    /// Sets `table.key` to the string `value`.
    pub(crate) fn text(&mut self, table: &str, key: &str, value: &str) {
        self.bare(table, key, format!("\"{value}\""));
    }

    /// Holds the table `table`, with no key when it holds none yet.
    pub(crate) fn header(&mut self, table: &str) {
        let _held = self.0.entry(table.to_string()).or_default();
    }

    /// Removes `table.key`.
    pub(crate) fn remove(&mut self, table: &str, key: &str) {
        if let Some(held) = self.0.get_mut(table) {
            let _removed = held.remove(key);
        }
    }

    /// Removes the table `table` and every key in it.
    pub(crate) fn drop_table(&mut self, table: &str) {
        let _removed = self.0.remove(table);
    }

    /// The file: each table's header and its keys, a table holding none written as its header alone.
    fn render(&self) -> String {
        self.0
            .iter()
            .map(|(table, keys)| {
                let lines: String = keys
                    .iter()
                    .map(|(key, value)| format!("{key} = {value}\n"))
                    .collect();
                format!("[{table}]\n{lines}")
            })
            .collect()
    }
}

/// A target's instances by name, each in a file of its own.
pub(crate) type Instances = BTreeMap<String, Settings>;

/// The target `ci` of `instances`, each file named for its instance.
pub(crate) fn target_of(instances: &Instances) -> TargetFiles {
    let mut target = TargetFiles::new("ci");
    for (name, settings) in instances {
        target.insert(
            name.as_str(),
            format!("{name}.toml"),
            settings.render().into_bytes(),
        );
    }
    target
}

/// Every defect of `instances` against `active`, as an administrator reads each.
pub(crate) fn defects(instances: &Instances, active: Option<&Instances>) -> Vec<String> {
    let active = active.map(target_of);
    check_target(&target_of(instances), active.as_ref())
        .iter()
        .map(Defect::to_string)
        .collect()
}

/// The settings every instance of a drawn target shares: its cluster, its secret and the system group's address.
fn joined(system_address: &str) -> Settings {
    let mut settings = Settings::default();
    settings.bare("cluster", "id", 7);
    settings.text("cluster", "role_secret_file", "/run/secret");
    settings.text("cluster", "system_address", system_address);
    settings
}

/// Makes `settings`, storage instance `index`'s of `storages`, the host of the system group: its `[system]` table, the two
/// moved tables and the partition count it forms the placement over.
pub(crate) fn host(settings: &mut Settings, index: u64, storages: u64) {
    settings.bare("cluster", "partition_count", storages);
    settings.text(
        "system",
        "role_address",
        &format!("tcp://10.0.2.{index}:{SYSTEM_PORT}"),
    );
    settings.text("system", "data_dir", "/data/system");
    settings.bare("metadata", "sessions_max", 16);
    settings.bare("sequencer", "epoch_ms_min", 2);
    settings.bare("sequencer", "epoch_ms_max", 10);
}

/// Undoes [`host`].
pub(crate) fn unhost(settings: &mut Settings) {
    settings.remove("cluster", "partition_count");
    for table in ["system", "metadata", "sequencer"] {
        settings.drop_table(table);
    }
}

/// A target that passes every check: one frontend, `computes` compute instances and `storages` storage instances of one
/// partition each, each on a host of its own, `storage_1` hosting the system group.
pub(crate) fn valid(computes: u64, storages: u64) -> Instances {
    let system_address = format!("tcp://10.0.2.1:{SYSTEM_PORT}");
    let mut instances = Instances::new();
    let mut frontend = joined(&system_address);
    frontend.text("observability", "address", "tcp://10.0.0.1:8080");
    let _none = instances.insert("frontend".to_string(), frontend);
    for index in 1..=computes {
        let mut compute = joined(&system_address);
        compute.text(
            "observability",
            "address",
            &format!("tcp://10.0.1.{index}:8080"),
        );
        compute.text(
            "compute",
            "role_address",
            &format!("tcp://10.0.1.{index}:7082"),
        );
        let _none = instances.insert(format!("compute_{index}"), compute);
    }
    for index in 1..=storages {
        let mut storage = joined(&system_address);
        storage.text(
            "observability",
            "address",
            &format!("tcp://10.0.2.{index}:8080"),
        );
        storage.bare("storage", "partition_first", index);
        storage.bare("storage", "partition_count", 1);
        storage.text(
            "storage",
            "role_address",
            &format!("tcp://10.0.2.{index}:7000"),
        );
        storage.text("storage", "data_dir", "/data/storage");
        if index == 1 {
            host(&mut storage, index, storages);
        }
        let _none = instances.insert(format!("storage_{index}"), storage);
    }
    instances
}

/// The settings of instance `name`, which a drawn target holds.
pub(crate) fn of<'a>(instances: &'a mut Instances, name: &str) -> &'a mut Settings {
    instances.entry(name.to_string()).or_default()
}

/// A number from `min` to `max`.
pub(crate) fn within(tc: &TestCase, min: u64, max: u64) -> u64 {
    tc.draw(gs::integers::<u64>().min_value(min).max_value(max))
}

/// Whether a drawn coin came up.
pub(crate) fn coin(tc: &TestCase) -> bool {
    tc.draw(gs::booleans())
}

/// The instance and the setting a defect's text names, as `Defect`'s display writes them.
pub(crate) fn named(defect: &str) -> (String, String) {
    let between = |start: &str, end: &str| {
        defect
            .split_once(start)
            .and_then(|(_, rest)| rest.split_once(end))
            .map(|(held, _)| held.to_string())
            .unwrap_or_default()
    };
    (between("instance ", ", "), between("setting ", ": "))
}
