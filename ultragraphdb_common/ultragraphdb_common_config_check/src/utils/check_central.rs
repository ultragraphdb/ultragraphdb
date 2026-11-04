/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ultragraphdb_tool_toml::{TomlDocument, TomlValue};

use crate::types::check_report::CheckReport;
use crate::types::defect::Defect;
use crate::types::target_files::TargetFiles;
use crate::utils::check_target::{check_files, check_target};
use crate::utils::merge::instances;

/// What the central file must hold.
const CENTRAL_SHAPE: &str = "it must declare at least one [target] table, each key a role or instance and each value a settings file";

/// The targets `central` declares, each with its files read.
///
/// # Errors
///
/// Every [`Defect`] of the central file: missing, unreadable or not TOML, declaring no target, declaring a target with no
/// file, or declaring a file that cannot be read, each absent file named by the value as written and the path it resolved to.
pub fn read_central(central: &Path) -> Result<Vec<TargetFiles>, Vec<Defect>> {
    let label = central.display().to_string();
    let bytes = std::fs::read(central).map_err(|error| {
        vec![
            Defect::new(
                format!("the central file cannot be read: {error}"),
                "config.targets_file must name a file that declares every settings file",
            )
            .in_file(label.clone()),
        ]
    })?;
    let document = TomlDocument::parse(&bytes).map_err(|error| {
        vec![
            Defect::new(
                format!("the central file is not valid: {error}"),
                CENTRAL_SHAPE,
            )
            .in_file(label.clone()),
        ]
    })?;
    let directory = central
        .parent()
        .map_or_else(PathBuf::new, Path::to_path_buf);
    let mut defects = Vec::new();
    let mut targets = Vec::new();
    for table in document.tables() {
        if table.name().is_empty() {
            if let Some(entry) = table.entries().first() {
                defects.push(
                    Defect::new(
                        format!("the key {} stands outside any target", entry.key()),
                        CENTRAL_SHAPE,
                    )
                    .in_file(label.clone())
                    .on_line(Some(entry.line())),
                );
            }
            continue;
        }
        let mut target = TargetFiles::new(table.name());
        if table.entries().is_empty() {
            defects.push(
                Defect::new("the target declares no file", CENTRAL_SHAPE)
                    .in_file(label.clone())
                    .of_target(table.name()),
            );
        }
        for entry in table.entries() {
            read_declared(
                &directory,
                &label,
                table.name(),
                entry,
                &mut target,
                &mut defects,
            );
        }
        targets.push(target);
    }
    if targets.is_empty() && defects.is_empty() {
        defects
            .push(Defect::new("the central file declares no target", CENTRAL_SHAPE).in_file(label));
    }
    if defects.is_empty() {
        Ok(targets)
    } else {
        // What was read is reported too, so one run tells the administrator everything to fix.
        defects.extend(targets.iter().flat_map(check_files));
        Err(defects)
    }
}

/// Reads the file `entry` of `target_name` declares into `target`, or records why not.
fn read_declared(
    directory: &Path,
    label: &str,
    target_name: &str,
    entry: &ultragraphdb_tool_toml::TomlEntry,
    target: &mut TargetFiles,
    defects: &mut Vec<Defect>,
) {
    let located = |defect: Defect| {
        defect
            .in_file(label)
            .on_line(Some(entry.line()))
            .of_target(target_name)
            .of_instance(entry.key())
    };
    let TomlValue::String(written) = entry.value() else {
        defects.push(located(Defect::new(
            "the value is not a string",
            "each key names a settings file, relative to the central file",
        )));
        return;
    };
    if Path::new(written).is_absolute() {
        defects.push(located(Defect::new(
            format!("the value {written} is an absolute path"),
            "each key names a settings file, relative to the central file",
        )));
        return;
    }
    let resolved = directory.join(written);
    match std::fs::read(&resolved) {
        Ok(bytes) => target.insert(entry.key(), written.clone(), bytes),
        Err(error) => defects.push(located(Defect::new(
            format!(
                "the declared file {written} cannot be read at {}: {error}",
                resolved.display()
            ),
            "every declared file must exist and be readable",
        ))),
    }
}

/// Checks everything `central` declares: every defect, each target with its instances, and a warning for a `.toml` file
/// beside it that nothing declares, which is usually a misspelled name.
#[must_use]
pub fn check_central(central: &Path) -> CheckReport {
    let mut report = CheckReport::default();
    match read_central(central) {
        Err(defects) => report.defects = defects,
        Ok(targets) => {
            for target in &targets {
                report.defects.extend(check_target(target, None));
                let names = instances(target)
                    .iter()
                    .map(|instance| instance.name().to_string())
                    .collect();
                report.targets.push((target.name().to_string(), names));
            }
            report.warnings = stray_files(central, &targets);
        }
    }
    report
}

/// The `.toml` files beside `central` that nothing declares.
fn stray_files(central: &Path, targets: &[TargetFiles]) -> Vec<String> {
    let Some(directory) = central.parent() else {
        return Vec::new();
    };
    let declared: BTreeSet<PathBuf> = targets
        .iter()
        .flat_map(|target| target.files().map(|(_, label, _)| directory.join(label)))
        .chain([central.to_path_buf()])
        .collect();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut strays: Vec<String> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .filter(|path| !declared.contains(path))
        .map(|path| format!("{} is declared by no target", path.display()))
        .collect();
    strays.sort();
    strays
}
