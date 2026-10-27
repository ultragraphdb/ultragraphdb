/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_transactions_log::WalConfig;

fn nz(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).expect("nonzero")
}

#[test]
fn test_defaults_without_power_loss_protection() {
    // The table in WalConfig's documentation: the frame's 1 MiB, 1,024 transactions, 256 MiB between checkpoints.
    let config = WalConfig::new(7, false);
    assert_eq!(config.cluster(), 7);
    assert!(!config.power_loss_protection());
    assert_eq!(config.round_bytes(), 1_048_576);
    assert_eq!(config.round_transactions(), 1_024);
    assert_eq!(config.checkpoint_bytes(), 268_435_456);
}

#[test]
fn test_defaults_with_power_loss_protection() {
    // 256 KiB rounds, 256 transactions, 64 MiB between checkpoints.
    let config = WalConfig::new(7, true);
    assert!(config.power_loss_protection());
    assert_eq!(config.round_bytes(), 262_144);
    assert_eq!(config.round_transactions(), 256);
    assert_eq!(config.checkpoint_bytes(), 67_108_864);
}

#[test]
fn test_the_flag_sets_nothing_but_the_round_bounds_and_the_checkpoint_interval() {
    let without = WalConfig::new(7, false);
    let with = WalConfig::new(7, true);
    assert_eq!(with.cluster(), without.cluster());
    assert_eq!(with.segment_blocks(), without.segment_blocks());
    assert_eq!(with.segment_size(), without.segment_size());
    assert_eq!(with.capacity(), without.capacity());
    // 16,384 blocks of 4,096 bytes, 64 MiB; 64 segments.
    assert_eq!(without.segment_blocks(), nz(16_384));
    assert_eq!(without.segment_size(), 67_108_864);
    assert_eq!(without.capacity(), nz(64));
}

#[test]
fn test_setters_replace_one_knob_each() {
    let base = WalConfig::new(7, false);
    let config = base
        .with_segment_blocks(nz(4))
        .with_capacity(nz(3))
        .with_round_bytes(5_000)
        .with_round_transactions(9)
        .with_checkpoint_bytes(70_000);
    assert_eq!(config.segment_blocks(), nz(4));
    assert_eq!(config.segment_size(), 16_384);
    assert_eq!(config.capacity(), nz(3));
    assert_eq!(config.round_bytes(), 5_000);
    assert_eq!(config.round_transactions(), 9);
    assert_eq!(config.checkpoint_bytes(), 70_000);
    assert_eq!(config.cluster(), 7);
    assert!(!config.power_loss_protection());
    // A setter on a protected drive keeps the flag.
    assert!(
        WalConfig::new(7, true)
            .with_round_bytes(1)
            .power_loss_protection()
    );
}

#[test]
fn test_the_largest_segment_size_does_not_overflow() {
    let config = WalConfig::new(7, false).with_segment_blocks(nz(u32::MAX));
    assert_eq!(config.segment_size(), u64::from(u32::MAX) * 4_096);
}
