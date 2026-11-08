/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_config_check::{TargetFiles, instances};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// An instance is a role's name, `_` and a label of a-z, 0-9 and `_`; any other key names none.
#[test]
fn test_an_instance_label_is_lowercase_digits_and_underscores() {
    let mut target = TargetFiles::new("ci");
    for key in [
        "frontend",
        "storage_a_1",
        "storage_A",
        "storage_",
        "storages",
    ] {
        target.insert(key, key, b"".to_vec());
    }
    let names: Vec<String> = instances(&target)
        .iter()
        .map(|instance| instance.name().to_string())
        .collect();
    assert_eq!(names, ["frontend", "storage_a_1"]);
}

/// Merged settings render as canonical TOML, a basic string escaping what TOML 1.0 requires: `"`, `\`, and the control
/// characters, by their short escape where one exists and as `\uXXXX` otherwise.
#[test]
fn test_merged_settings_render_canonically() -> TestResult {
    let mut target = TargetFiles::new("ci");
    target.insert(
        "frontend",
        "frontend.toml",
        b"[frontend]\ndata_dir = \"a\\u0001b\\tc\\\"d\\\\e\"\n".to_vec(),
    );
    let all = instances(&target);
    let frontend = all.first().context("the frontend")?;
    assert_eq!(
        frontend.rendered(),
        "[frontend]\ndata_dir = \"a\\u0001b\\tc\\\"d\\\\e\"\n"
    );
    assert_eq!(
        frontend
            .config()
            .context("its settings")?
            .frontend()
            .data_dir(),
        Some("a\u{1}b\tc\"d\\e")
    );
    Ok(())
}

/// A line feed and a carriage return render by their short escapes, and read back as themselves.
#[test]
fn test_line_breaks_render_by_their_short_escapes() -> TestResult {
    let mut target = TargetFiles::new("ci");
    let written = "[frontend]\ndata_dir = \"a\\nb\\rc\"\n";
    target.insert("frontend", "frontend.toml", written.as_bytes().to_vec());
    let all = instances(&target);
    let frontend = all.first().context("the frontend")?;
    assert_eq!(frontend.rendered(), written);
    assert_eq!(
        frontend
            .config()
            .context("its settings")?
            .frontend()
            .data_dir(),
        Some("a\nb\rc")
    );
    Ok(())
}

/// An instance one of whose layers is not TOML has no merged settings, though the other layer renders.
#[test]
fn test_a_layer_that_is_not_toml_leaves_no_settings() -> TestResult {
    let mut target = TargetFiles::new("ci");
    target.insert("storage", "storage.toml", b"[storage\n".to_vec());
    target.insert(
        "storage_1",
        "storage_1.toml",
        b"[storage]\npartition_first = 1\n".to_vec(),
    );
    let all = instances(&target);
    let storage = all.first().context("storage_1")?;
    assert_eq!(storage.name(), "storage_1");
    assert_eq!(storage.rendered(), "[storage]\npartition_first = 1\n");
    assert!(storage.config().is_none());
    Ok(())
}
