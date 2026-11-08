/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A central file's own refusals, each naming the file, and the listing of the folder it stands in.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use ultragraphdb_common_config_check::{Defect, check_central, read_central};
use ultragraphdb_tool_bdd::{Context, TestResult};

static FOLDERS: AtomicU32 = AtomicU32::new(0);

/// The settings of a storage instance, which its file holds whole.
const STORAGE: &str = "[storage]\npartition_count = 1\n";

/// A folder no other test names, made empty.
fn folder() -> TestResult<PathBuf> {
    let at = std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!(
            "check-central-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&at)?;
    Ok(at)
}

/// The defects `read_central` gives for a central file holding `text`, one per line, and the central file's path.
fn refused(text: &str) -> TestResult<(String, String)> {
    let at = folder()?;
    let central = at.join("targets.toml");
    std::fs::write(&central, text)?;
    let defects = read_central(&central).err().context("a refusal")?;
    let listing = defects
        .iter()
        .map(Defect::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    Ok((listing, central.display().to_string()))
}

/// A central file that is not TOML is refused whole, naming the file and the parser's reason.
#[test]
fn test_a_central_file_that_is_not_toml_is_refused() -> TestResult {
    let (listing, central) = refused("[ci\n")?;
    assert!(
        listing.starts_with(&format!("{central}: the central file is not valid: ")),
        "{listing}"
    );
    Ok(())
}

/// A key before any target belongs to none, and is named by its line.
#[test]
fn test_a_key_outside_any_target_is_refused_by_its_line() -> TestResult {
    let (listing, central) = refused("storage = \"storage.toml\"\n[ci]\n")?;
    assert!(
        listing.contains(&format!(
            "{central}:1: the key storage stands outside any target;"
        )),
        "{listing}"
    );
    Ok(())
}

/// A declared file is a string naming a path relative to the central file: another value, or an absolute path, is
/// refused naming the target and the key.
#[test]
fn test_a_declared_file_is_a_relative_path() -> TestResult {
    let (number, central) = refused("[ci]\nstorage = 1\n")?;
    assert!(
        number.contains(&format!(
            "{central}:2: target ci, instance storage, the value is not a string;"
        )),
        "{number}"
    );
    let absolute = "/etc/storage.toml";
    let (absolute_listing, absolute_central) =
        refused(&format!("[ci]\nstorage = \"{absolute}\"\n"))?;
    assert!(
        absolute_listing.contains(&format!(
            "{absolute_central}:2: target ci, instance storage, the value {absolute} is an absolute path;"
        )),
        "{absolute_listing}"
    );
    Ok(())
}

/// A folder whose files can be read but not listed is checked whole, its stray files unnamed rather than guessed at.
#[test]
fn test_a_folder_that_cannot_be_listed_names_no_stray_file() -> TestResult {
    let at = folder()?;
    std::fs::write(at.join("storage.toml"), STORAGE)?;
    std::fs::write(at.join("stray.toml"), STORAGE)?;
    let central = at.join("targets.toml");
    std::fs::write(&central, "[ci]\nstorage = \"storage.toml\"\n")?;
    let listed = check_central(&central);
    assert!(
        listed
            .warnings()
            .iter()
            .any(|warning| warning.contains("stray.toml")),
        "{:?}",
        listed.warnings()
    );
    // Search without read: each file opens by its name, and the folder lists nothing.
    std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o300))?;
    let unlisted = check_central(&central);
    std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o700))?;
    assert_eq!(unlisted.defects(), listed.defects());
    assert_eq!(unlisted.targets(), listed.targets());
    assert!(unlisted.warnings().is_empty(), "{:?}", unlisted.warnings());
    Ok(())
}
