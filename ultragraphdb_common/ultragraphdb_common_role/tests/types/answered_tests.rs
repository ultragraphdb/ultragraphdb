/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! [`Answered::frame`] for the peer's Error: the request's failure wherever the Error came, on the request's stream or on
//! stream 0. Its frame is read from a connection in `role_client/role_client_answer_tests.rs`, since only a connection
//! makes one.

use ultragraphdb_common_role::Answered;
use ultragraphdb_protocol_transport::TransportErrorEnum;
use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The peer's Error on the request's stream is the request's refusal, its code and message kept.
#[test]
fn test_a_refusal_on_the_requests_stream_is_the_requests_failure() -> TestResult {
    let refused = Answered::Refused {
        code: ErrorCode::Refused,
        message: "no such version".to_string(),
    };
    let failure = refused.frame().err().context("a failure")?;
    assert_eq!(
        failure.0,
        TransportErrorEnum::Refused {
            code: ErrorCode::Refused,
            message: "no such version".to_string(),
        }
    );
    Ok(())
}

/// The peer's Error on stream 0 is the request's refusal too, its code and message kept: another code and another message
/// than the test above, so neither is a constant.
#[test]
fn test_a_refusal_on_stream_0_is_the_requests_failure() -> TestResult {
    let refused = Answered::ConnectionRefused {
        code: ErrorCode::Unavailable,
        message: "this process is stopping".to_string(),
    };
    let failure = refused.frame().err().context("a failure")?;
    assert_eq!(
        failure.0,
        TransportErrorEnum::Refused {
            code: ErrorCode::Unavailable,
            message: "this process is stopping".to_string(),
        }
    );
    Ok(())
}
