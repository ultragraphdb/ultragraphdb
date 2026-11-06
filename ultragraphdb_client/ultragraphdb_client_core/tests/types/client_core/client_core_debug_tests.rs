/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client_core::{ClientCore, TEST_PASSWORD, TestServer};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::harness::{CLUSTER, Harness, config, login, scram_welcome};

/// The core in brief: its configuration, principal, generation, and the counts of its streams, of the reads it remembers and of its requests, with its
/// sessions; never the login, which holds the password.
#[test]
fn test_debug_names_the_state_and_leaves_the_login_out() -> TestResult {
    let core = ClientCore::new(config(), login()?);
    let text = format!("{core:?}");
    assert_eq!(
        text,
        format!(
            "ClientCore {{ config: {:?}, principal: None, generation: 0, streams: 0, remembered_reads: 0, pending: 0, sessions: {{}} }}",
            config()
        )
    );
    let mut h = Harness::authenticated(TestServer::new(CLUSTER, scram_welcome()))?;
    let _p = h.core.register(h.now)?;
    let authenticated = format!("{:?}", h.core);
    assert!(
        authenticated
            .contains("principal: Some(7), generation: 1, streams: 1, remembered_reads: 0, pending: 1, sessions: {}"),
        "{authenticated}"
    );
    assert!(!authenticated.contains(TEST_PASSWORD), "{authenticated}");
    Ok(())
}
