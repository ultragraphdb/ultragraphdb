/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::FaultConfig;

#[test]
fn test_none_injects_nothing() {
    let c = FaultConfig::none();
    let all = [
        c.batch_failure(),
        c.crash_during_batch(),
        c.crash_between_steps(),
        c.power_loss_between_steps(),
        c.crash_during_drop(),
        c.power_loss_during_drop(),
        c.read_failure(),
        c.scan_failure(),
        c.corruption(),
        c.slow(),
    ];
    assert!(all.iter().all(|&p| p == 0));
    assert_eq!(c.slow_max_nanos(), 0);
}

#[test]
fn test_mixed_injects_every_class() {
    let c = FaultConfig::mixed();
    assert!(c.batch_failure() > 0 && c.crash_during_batch() > 0 && c.crash_between_steps() > 0);
    assert!(
        c.power_loss_between_steps() > 0
            && c.crash_during_drop() > 0
            && c.power_loss_during_drop() > 0
    );
    assert!(c.read_failure() > 0 && c.scan_failure() > 0 && c.corruption() > 0 && c.slow() > 0);
    assert!(c.slow_max_nanos() > 0);
}

#[test]
fn test_each_builder_sets_its_class() {
    let c = FaultConfig::none()
        .with_batch_failure(1)
        .with_crash_during_batch(2)
        .with_crash_between_steps(3)
        .with_power_loss_between_steps(4)
        .with_crash_during_drop(5)
        .with_power_loss_during_drop(6)
        .with_read_failure(7)
        .with_scan_failure(8)
        .with_corruption(9)
        .with_slow(10, 11);
    let got = [
        c.batch_failure(),
        c.crash_during_batch(),
        c.crash_between_steps(),
        c.power_loss_between_steps(),
        c.crash_during_drop(),
        c.power_loss_during_drop(),
        c.read_failure(),
        c.scan_failure(),
        c.corruption(),
        c.slow(),
    ];
    assert_eq!(got, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(c.slow_max_nanos(), 11);
}

#[test]
fn test_rebuild_stops_are_configured_per_rebuild() {
    let c = FaultConfig::none();
    assert_eq!(
        (c.crash_during_rebuild(), c.power_loss_during_rebuild()),
        (0, 0)
    );
    let m = FaultConfig::mixed();
    assert!(m.crash_during_rebuild() > 0 && m.power_loss_during_rebuild() > 0);
    let c = FaultConfig::none()
        .with_crash_during_rebuild(12)
        .with_power_loss_during_rebuild(13);
    assert_eq!(
        (c.crash_during_rebuild(), c.power_loss_during_rebuild()),
        (12, 13)
    );
}
