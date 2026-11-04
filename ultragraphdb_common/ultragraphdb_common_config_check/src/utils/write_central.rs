/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use crate::types::defect::Defect;
use crate::types::target_files::TargetFiles;
use crate::utils::check_target::well_named;
use crate::utils::merge::role_of;

/// The central file's name in a folder this writes.
pub const CENTRAL_FILE: &str = "targets.toml";

/// Writes `targets` as a folder [`read_central`](crate::read_central) reads back byte for byte: `targets.toml`, written last,
/// and each target's files in a folder of its name, each named for its key. It adds to nothing: `folder` is absent or empty.
///
/// # Errors
///
/// A [`Defect`] for a folder holding anything, a target or key that cannot name a file, which comes from a peer and is checked
/// before anything is written, or a file that cannot be written.
pub fn write_central(folder: &Path, targets: &[TargetFiles]) -> Result<PathBuf, Box<Defect>> {
    let label = folder.display().to_string();
    let wrong = |found: String, required: &str| {
        Box::new(Defect::new(found, required).in_file(label.clone()))
    };
    for target in targets {
        if !well_named(target.name()) {
            return Err(wrong(
                format!("the target's name {} cannot name a folder", target.name()),
                "a target is named by A-Z, a-z, 0-9, _ and -",
            ));
        }
        if let Some((key, _, _)) = target.files().find(|(key, _, _)| role_of(key).is_none()) {
            return Err(wrong(
                format!(
                    "target {} holds the file {key}, which names no role",
                    target.name()
                ),
                "a key is frontend, compute or storage, alone or followed by _ and a label of a-z, 0-9 and _",
            ));
        }
    }
    let holds_something =
        std::fs::read_dir(folder).is_ok_and(|mut entries| entries.next().is_some());
    if holds_something {
        return Err(wrong(
            "the folder already holds files, which a restore would mix with its own".to_string(),
            "name a folder that is absent or empty",
        ));
    }
    let failed = |path: &Path, error: &std::io::Error| {
        wrong(
            format!("{} cannot be written: {error}", path.display()),
            "the folder must be writable",
        )
    };
    let mut central = String::new();
    for target in targets {
        let name = target.name();
        let directory = folder.join(name);
        std::fs::create_dir_all(&directory).map_err(|error| failed(&directory, &error))?;
        central.push_str(&format!("[{name}]\n"));
        for (key, _, bytes) in target.files() {
            let file = directory.join(format!("{key}.toml"));
            std::fs::write(&file, bytes).map_err(|error| failed(&file, &error))?;
            central.push_str(&format!("{key} = \"{name}/{key}.toml\"\n"));
        }
    }
    let file = folder.join(CENTRAL_FILE);
    std::fs::write(&file, central).map_err(|error| failed(&file, &error))?;
    Ok(file)
}
