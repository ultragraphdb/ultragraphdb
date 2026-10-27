/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;
use std::io;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_bus::{BusError, BusErrorEnum};
use ultragraphdb_protocol_wire::ErrorCode;

#[test]
fn test_each_constructor_builds_its_variant() {
    assert_eq!(
        BusError::Protocol(ProtocolError::UnknownCommand(18)).0,
        BusErrorEnum::Protocol {
            error: ProtocolError::UnknownCommand(18)
        }
    );
    assert_eq!(
        BusError::Io(&io::Error::new(io::ErrorKind::BrokenPipe, "gone")).0,
        BusErrorEnum::Io {
            kind: io::ErrorKind::BrokenPipe,
            message: "gone".to_string()
        }
    );
    assert_eq!(BusError::Closed().0, BusErrorEnum::Closed);
    assert_eq!(
        BusError::Truncated(3, 256).0,
        BusErrorEnum::Truncated {
            received: 3,
            expected: 256
        }
    );
    assert_eq!(
        BusError::PoolExhausted(4).0,
        BusErrorEnum::PoolExhausted { buffers: 4 }
    );
    assert_eq!(BusError::StreamZero().0, BusErrorEnum::StreamZero);
    assert_eq!(
        BusError::StreamOpen(5).0,
        BusErrorEnum::StreamOpen { stream: 5 }
    );
    assert_eq!(
        BusError::StreamLimit(64).0,
        BusErrorEnum::StreamLimit { limit: 64 }
    );
    assert_eq!(
        BusError::UnknownStream(6).0,
        BusErrorEnum::UnknownStream { stream: 6 }
    );
    assert_eq!(
        BusError::Refused(ErrorCode::ProtocolVersion, "no protocol").0,
        BusErrorEnum::Refused {
            code: ErrorCode::ProtocolVersion,
            message: "no protocol".to_string()
        }
    );
    assert_eq!(
        BusError::UnexpectedCommand(2, 16).0,
        BusErrorEnum::UnexpectedCommand {
            expected: 2,
            found: 16
        }
    );
    assert_ne!(BusError::Truncated(3, 256), BusError::Truncated(256, 3));
    assert_ne!(
        BusError::UnexpectedCommand(2, 16),
        BusError::UnexpectedCommand(16, 2)
    );
}

#[test]
fn test_each_names_its_values() {
    assert_eq!(
        BusError::Protocol(ProtocolError::UnknownCommand(18)).to_string(),
        "frame refused: unknown command 18"
    );
    assert_eq!(
        BusError::Io(&io::Error::new(io::ErrorKind::BrokenPipe, "gone")).to_string(),
        "connection failed: gone"
    );
    assert_eq!(
        BusError::Closed().to_string(),
        "connection closed by the peer"
    );
    assert_eq!(
        BusError::Truncated(3, 256).to_string(),
        "connection closed 3 bytes into a frame of 256"
    );
    assert_eq!(
        BusError::PoolExhausted(4).to_string(),
        "all 4 frame buffers are in use"
    );
    assert_eq!(
        BusError::StreamZero().to_string(),
        "stream 0 carries no request"
    );
    assert_eq!(
        BusError::StreamOpen(5).to_string(),
        "stream 5 is already open"
    );
    assert_eq!(
        BusError::StreamLimit(64).to_string(),
        "64 streams are in flight, the limit"
    );
    assert_eq!(
        BusError::UnknownStream(6).to_string(),
        "stream 6 is not open"
    );
    assert_eq!(
        BusError::Refused(ErrorCode::ProtocolVersion, "no protocol").to_string(),
        "refused with ProtocolVersion: no protocol"
    );
    assert_eq!(
        BusError::UnexpectedCommand(2, 16).to_string(),
        "command 16 arrived where command 2 was due"
    );
    let error: Box<dyn Error> = Box::new(BusError::Closed());
    assert!(error.source().is_none());
}
