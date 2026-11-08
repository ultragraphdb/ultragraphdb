/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a role client refuses before its peer could: a password SASLprep refuses, and a name whose escaped client-first
//! message no frame holds.

use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::{Endpoint, TransportErrorEnum};
use ultragraphdb_protocol_wire::{Command, MAX_FRAME_SIZE};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::serve_tests::{CLUSTER, PASSWORD, bounded, free_tcp, serving};

/// A password SASLprep refuses, holding a control character, fails the dial as an authentication, before anything is sent.
#[test]
fn test_a_password_saslprep_refuses_fails_the_authentication() -> TestResult {
    bounded(async {
        let address = serving(&free_tcp()?, Endpoint::Compute).await?;
        let refused = RoleClient::dial(&address, CLUSTER, "compute", "s3\u{7}cret")
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(refused.0, TransportErrorEnum::Authentication { .. }),
            "{refused}"
        );
        Ok(())
    })
}

/// A name the Hello holds whose client-first message, each `,` escaped as three bytes, passes the largest frame is refused
/// as an Authenticate that cannot be built.
#[test]
fn test_a_name_too_long_to_authenticate_is_refused_as_a_frame_not_built() -> TestResult {
    bounded(async {
        let address = serving(&free_tcp()?, Endpoint::Compute).await?;
        let name = ",".repeat(MAX_FRAME_SIZE / 2);
        let refused = RoleClient::dial(&address, CLUSTER, &name, PASSWORD)
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(
                refused.0,
                TransportErrorEnum::FrameNotBuilt { command, .. }
                    if command == Command::Authenticate.number()
            ),
            "{refused}"
        );
        Ok(())
    })
}
