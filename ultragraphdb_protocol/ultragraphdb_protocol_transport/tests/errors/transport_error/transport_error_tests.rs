/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;
use std::io;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{TransportError, TransportErrorEnum};
use ultragraphdb_protocol_wire::ErrorCode;

#[test]
fn test_each_constructor_builds_its_variant() {
    assert_eq!(
        TransportError::Protocol(ProtocolError::UnknownCommand(18)).0,
        TransportErrorEnum::Protocol {
            error: ProtocolError::UnknownCommand(18)
        }
    );
    assert_eq!(
        TransportError::Io(&io::Error::new(io::ErrorKind::BrokenPipe, "gone")).0,
        TransportErrorEnum::Io {
            kind: io::ErrorKind::BrokenPipe,
            message: "gone".to_string()
        }
    );
    assert_eq!(TransportError::Closed().0, TransportErrorEnum::Closed);
    assert_eq!(
        TransportError::Truncated(3, 256).0,
        TransportErrorEnum::Truncated {
            received: 3,
            expected: 256
        }
    );
    assert_eq!(
        TransportError::PoolExhausted(4).0,
        TransportErrorEnum::PoolExhausted { buffers: 4 }
    );
    assert_eq!(
        TransportError::StreamZero().0,
        TransportErrorEnum::StreamZero
    );
    assert_eq!(
        TransportError::StreamOpen(5).0,
        TransportErrorEnum::StreamOpen { stream: 5 }
    );
    assert_eq!(
        TransportError::StreamLimit(64).0,
        TransportErrorEnum::StreamLimit { limit: 64 }
    );
    assert_eq!(
        TransportError::UnknownStream(6).0,
        TransportErrorEnum::UnknownStream { stream: 6 }
    );
    assert_eq!(
        TransportError::Refused(ErrorCode::ProtocolVersion, "no protocol").0,
        TransportErrorEnum::Refused {
            code: ErrorCode::ProtocolVersion,
            message: "no protocol".to_string()
        }
    );
    assert_eq!(
        TransportError::UnexpectedCommand(2, 16).0,
        TransportErrorEnum::UnexpectedCommand {
            expected: 2,
            found: 16
        }
    );
    assert_eq!(
        TransportError::Authentication("the server did not prove itself").0,
        TransportErrorEnum::Authentication {
            reason: "the server did not prove itself".to_string()
        }
    );
    assert_ne!(
        TransportError::Truncated(3, 256),
        TransportError::Truncated(256, 3)
    );
    assert_ne!(
        TransportError::UnexpectedCommand(2, 16),
        TransportError::UnexpectedCommand(16, 2)
    );
}

#[test]
fn test_each_names_its_values() {
    assert_eq!(
        TransportError::Protocol(ProtocolError::UnknownCommand(18)).to_string(),
        "frame refused: unknown command 18"
    );
    assert_eq!(
        TransportError::Io(&io::Error::new(io::ErrorKind::BrokenPipe, "gone")).to_string(),
        "connection failed: gone"
    );
    assert_eq!(
        TransportError::Closed().to_string(),
        "connection closed by the peer"
    );
    assert_eq!(
        TransportError::Truncated(3, 256).to_string(),
        "connection closed 3 bytes into a frame of 256"
    );
    assert_eq!(
        TransportError::PoolExhausted(4).to_string(),
        "all 4 frame buffers are in use"
    );
    assert_eq!(
        TransportError::StreamZero().to_string(),
        "stream 0 carries no request"
    );
    assert_eq!(
        TransportError::StreamOpen(5).to_string(),
        "stream 5 is already open"
    );
    assert_eq!(
        TransportError::StreamLimit(64).to_string(),
        "64 streams are in flight, the limit"
    );
    assert_eq!(
        TransportError::UnknownStream(6).to_string(),
        "stream 6 is not open"
    );
    assert_eq!(
        TransportError::Refused(ErrorCode::ProtocolVersion, "no protocol").to_string(),
        "refused with ProtocolVersion: no protocol"
    );
    assert_eq!(
        TransportError::UnexpectedCommand(2, 16).to_string(),
        "command 16 arrived where command 2 was due"
    );
    assert_eq!(
        TransportError::Authentication("the server did not prove itself").to_string(),
        "the server's authentication was refused: the server did not prove itself"
    );
    let error: Box<dyn Error> = Box::new(TransportError::Closed());
    assert!(error.source().is_none());
}
