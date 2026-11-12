/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_client_core::{ClientConfig, ClientCore};
use ultragraphdb_protocol_transport::Anonymous;
use ultragraphdb_protocol_wire::{Hello, MAX_FRAME_SIZE, PROTOCOL_VERSION};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_the_defaults_are_as_documented() -> TestResult {
    let config = ClientConfig::new(7, "c");
    assert_eq!(
        (config.cluster(), config.name(), config.release()),
        (7, "c", 0)
    );
    assert_eq!(config.timeout(), Duration::from_secs(10));
    assert_eq!(config.commit_attempts(), 8);
    assert_eq!(config.retry_delay(), Duration::from_millis(50));
    assert_eq!(config.credit(), 4);
    assert_eq!(config.renewal_margin(), Duration::from_secs(60));
    assert_eq!(config.buffers(), 64);
    assert_eq!(
        config.hello(),
        Hello::new(1, PROTOCOL_VERSION, 0, u32::try_from(MAX_FRAME_SIZE)?, "c")
    );
    // The core keeps the configuration it was made with.
    assert_eq!(ClientCore::new(config.clone(), Anonymous).config(), &config);
    Ok(())
}

#[test]
fn test_each_setting_is_kept() -> TestResult {
    let config = ClientConfig::new(u64::MAX, "")
        .with_release(u32::MAX)
        .with_protocols(2, 3)
        .with_timeout(Duration::ZERO)
        .with_commit_attempts(u32::MAX)
        .with_retry_delay(Duration::from_micros(1))
        .with_credit(u32::MAX)
        .with_renewal_margin(Duration::ZERO)
        .with_buffers(usize::MAX);
    assert_eq!(
        (config.cluster(), config.name(), config.release()),
        (u64::MAX, "", u32::MAX)
    );
    assert_eq!(config.timeout(), Duration::ZERO);
    assert_eq!(config.commit_attempts(), u32::MAX);
    assert_eq!(config.retry_delay(), Duration::from_micros(1));
    assert_eq!(config.credit(), u32::MAX);
    assert_eq!(config.renewal_margin(), Duration::ZERO);
    assert_eq!(config.buffers(), usize::MAX);
    // The Hello names the protocols and release set, the largest frame, and the name.
    assert_eq!(
        config.hello(),
        Hello::new(2, 3, u32::MAX, u32::try_from(MAX_FRAME_SIZE)?, "")
    );
    Ok(())
}

/// A commit is sent at least once, and a stream granted at least one frame at a time: 0 is raised to 1, 1 kept.
#[test]
fn test_attempts_and_credit_are_at_least_one() -> TestResult {
    for (asked, kept) in [(0, 1), (1, 1), (2, 2)] {
        let config = ClientConfig::new(1, "c")
            .with_commit_attempts(asked)
            .with_credit(asked);
        assert_eq!(
            (config.commit_attempts(), config.credit()),
            (kept, kept),
            "{asked}"
        );
    }
    Ok(())
}

/// A client reads into at least two buffers, one for the frame being read and one for a read's: fewer are raised to 2.
#[test]
fn test_buffers_are_at_least_two() -> TestResult {
    for (asked, kept) in [(0, 2), (1, 2), (2, 2), (3, 3)] {
        assert_eq!(
            ClientConfig::new(1, "c").with_buffers(asked).buffers(),
            kept,
            "{asked}"
        );
    }
    Ok(())
}
