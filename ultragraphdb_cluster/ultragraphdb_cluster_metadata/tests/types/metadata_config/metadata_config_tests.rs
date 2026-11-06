/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_the_defaults() {
    // The values the settings document: 2^20 sequences, 4,096 sessions, 10,000 rounds.
    let config = MetadataConfig::new();
    assert_eq!(config.block_size_max(), 1_048_576);
    assert_eq!(config.sessions_max(), 4_096);
    assert_eq!(config.snapshot_rounds(), 10_000);
    assert_eq!(MetadataConfig::default(), config);
}

#[test]
fn test_the_limits() {
    assert_eq!(MetadataConfig::BLOCK_SIZE_MAX_MAX, 4_294_967_296);
    assert_eq!(MetadataConfig::SESSIONS_MAX_MAX, 1_048_576);
    assert_eq!(MetadataConfig::SNAPSHOT_ROUNDS_MAX, 1_048_576);
}

#[test]
fn test_settings_at_their_bounds_are_taken() -> TestResult {
    let config = MetadataConfig::new()
        .with_block_size_max(1)?
        .with_sessions_max(1)?
        .with_snapshot_rounds(1)?;
    assert_eq!(
        (
            config.block_size_max(),
            config.sessions_max(),
            config.snapshot_rounds()
        ),
        (1, 1, 1)
    );
    let config_2 = config
        .with_block_size_max(4_294_967_296)?
        .with_sessions_max(1_048_576)?
        .with_snapshot_rounds(1_048_576)?;
    assert_eq!(
        (
            config_2.block_size_max(),
            config_2.sessions_max(),
            config_2.snapshot_rounds()
        ),
        (4_294_967_296, 1_048_576, 1_048_576)
    );
    // Each setter changes its own setting alone.
    let config_3 = MetadataConfig::new().with_sessions_max(9)?;
    assert_eq!(
        (config_3.block_size_max(), config_3.snapshot_rounds()),
        (1_048_576, 10_000)
    );
    Ok(())
}

#[test]
fn test_zero_is_refused_naming_the_setting() -> TestResult {
    let config = MetadataConfig::new();
    let refusals = [
        config.with_block_size_max(0).err().context("refused")?,
        config.with_sessions_max(0).err().context("refused")?,
        config.with_snapshot_rounds(0).err().context("refused")?,
    ];
    let expected: Vec<ClusterErrorEnum> = ["block_size_max", "sessions_max", "snapshot_rounds"]
        .into_iter()
        .map(|setting| ClusterErrorEnum::SettingBelow {
            setting,
            value: 0,
            bound: "its minimum",
            minimum: 1,
        })
        .collect();
    let refused: Vec<ClusterErrorEnum> = refusals.into_iter().map(|e| e.0).collect();
    assert_eq!(refused, expected);
    Ok(())
}

#[test]
fn test_one_past_the_maximum_is_refused_naming_it() -> TestResult {
    let config = MetadataConfig::new();
    assert_eq!(
        config
            .with_block_size_max(4_294_967_297)
            .err()
            .context("refused")?
            .0,
        ClusterErrorEnum::SettingAbove {
            setting: "block_size_max",
            value: 4_294_967_297,
            maximum: 4_294_967_296
        }
    );
    assert_eq!(
        config
            .with_sessions_max(1_048_577)
            .err()
            .context("refused")?
            .0,
        ClusterErrorEnum::SettingAbove {
            setting: "sessions_max",
            value: 1_048_577,
            maximum: 1_048_576
        }
    );
    assert_eq!(
        config
            .with_snapshot_rounds(1_048_577)
            .err()
            .context("refused")?
            .0,
        ClusterErrorEnum::SettingAbove {
            setting: "snapshot_rounds",
            value: 1_048_577,
            maximum: 1_048_576
        }
    );
    Ok(())
}

#[test]
fn test_catalog_check_ms_holds_within_its_bounds() -> TestResult {
    let config = MetadataConfig::new();
    assert_eq!(config.catalog_check_ms(), MetadataConfig::CATALOG_CHECK_MS);
    let longest = config.with_catalog_check_ms(MetadataConfig::CATALOG_CHECK_MS_MAX)?;
    assert_eq!(
        longest.catalog_check_ms(),
        MetadataConfig::CATALOG_CHECK_MS_MAX
    );
    assert_eq!(config.with_catalog_check_ms(1)?.catalog_check_ms(), 1);
    assert_eq!(
        config.with_catalog_check_ms(0).err().context("refused")?.0,
        ClusterErrorEnum::SettingBelow {
            setting: "catalog_check_ms",
            value: 0,
            minimum: 1,
            bound: "its minimum",
        }
    );
    assert_eq!(
        config
            .with_catalog_check_ms(MetadataConfig::CATALOG_CHECK_MS_MAX + 1)
            .err()
            .context("refused")?
            .0,
        ClusterErrorEnum::SettingAbove {
            setting: "catalog_check_ms",
            value: MetadataConfig::CATALOG_CHECK_MS_MAX + 1,
            maximum: MetadataConfig::CATALOG_CHECK_MS_MAX,
        }
    );
    Ok(())
}
