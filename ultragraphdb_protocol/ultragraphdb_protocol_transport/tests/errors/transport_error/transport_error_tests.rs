/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;
use std::io;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{AddressRule, TransportError, TransportErrorEnum};
use ultragraphdb_protocol_wire::ErrorCode;

#[test]
fn test_each_constructor_builds_its_variant() {
    assert_eq!(
        TransportError::protocol(ProtocolError::unknown_command(18)).0,
        TransportErrorEnum::Protocol {
            error: ProtocolError::unknown_command(18)
        }
    );
    assert_eq!(
        TransportError::io(&io::Error::new(io::ErrorKind::BrokenPipe, "gone")).0,
        TransportErrorEnum::Io {
            kind: io::ErrorKind::BrokenPipe,
            message: "gone".to_string()
        }
    );
    assert_eq!(TransportError::closed().0, TransportErrorEnum::Closed);
    assert_eq!(
        TransportError::truncated(3, 256).0,
        TransportErrorEnum::Truncated {
            received: 3,
            expected: 256
        }
    );
    assert_eq!(
        TransportError::pool_exhausted(4).0,
        TransportErrorEnum::PoolExhausted { buffers: 4 }
    );
    assert_eq!(
        TransportError::stream_zero().0,
        TransportErrorEnum::StreamZero
    );
    assert_eq!(
        TransportError::stream_open(5).0,
        TransportErrorEnum::StreamOpen { stream: 5 }
    );
    assert_eq!(
        TransportError::stream_limit(64).0,
        TransportErrorEnum::StreamLimit { limit: 64 }
    );
    assert_eq!(
        TransportError::unknown_stream(6).0,
        TransportErrorEnum::UnknownStream { stream: 6 }
    );
    assert_eq!(
        TransportError::refused(ErrorCode::ProtocolVersion, "no protocol").0,
        TransportErrorEnum::Refused {
            code: ErrorCode::ProtocolVersion,
            message: "no protocol".to_string()
        }
    );
    assert_eq!(
        TransportError::unexpected_command(2, 16).0,
        TransportErrorEnum::UnexpectedCommand {
            expected: 2,
            found: 16
        }
    );
    assert_eq!(
        TransportError::authentication("the server did not prove itself").0,
        TransportErrorEnum::Authentication {
            reason: "the server did not prove itself".to_string()
        }
    );
    assert_ne!(
        TransportError::truncated(3, 256),
        TransportError::truncated(256, 3)
    );
    assert_ne!(
        TransportError::unexpected_command(2, 16),
        TransportError::unexpected_command(16, 2)
    );
}

#[test]
fn test_a_frame_not_built_names_its_command_and_why() {
    let too_large = ProtocolError::frame_too_large(1_048_600, 1_048_576);
    let error = TransportError::frame_not_built(18, too_large.clone());
    assert_eq!(
        error.0,
        TransportErrorEnum::FrameNotBuilt {
            command: 18,
            error: too_large.clone()
        }
    );
    // The protocol error's text names the size against the bound.
    assert_eq!(
        error.to_string(),
        format!("the frame of command 18 could not be built: {too_large}")
    );
    assert!(too_large.to_string().contains("1048600"), "{too_large}");
}

#[test]
fn test_an_address_refusal_holds_the_address_and_the_rule() {
    assert_eq!(
        TransportError::address("http://h:1", AddressRule::UnknownScheme).0,
        TransportErrorEnum::Address {
            address: "http://h:1".to_string(),
            rule: AddressRule::UnknownScheme
        }
    );
}

#[test]
fn test_an_address_refusal_names_the_address_and_the_rule() {
    assert_eq!(
        TransportError::address("http://h:1", AddressRule::UnknownScheme).to_string(),
        "the address \"http://h:1\" is refused: a scheme other than tcp:// and unix://"
    );
}

#[test]
fn test_each_names_its_values() {
    assert_eq!(
        TransportError::protocol(ProtocolError::unknown_command(18)).to_string(),
        "frame refused: unknown command 18"
    );
    assert_eq!(
        TransportError::io(&io::Error::new(io::ErrorKind::BrokenPipe, "gone")).to_string(),
        "connection failed: gone"
    );
    assert_eq!(
        TransportError::closed().to_string(),
        "connection closed by the peer"
    );
    assert_eq!(
        TransportError::truncated(3, 256).to_string(),
        "connection closed 3 bytes into a frame of 256"
    );
    assert_eq!(
        TransportError::pool_exhausted(4).to_string(),
        "all 4 frame buffers are in use"
    );
    assert_eq!(
        TransportError::stream_zero().to_string(),
        "stream 0 carries no request"
    );
    assert_eq!(
        TransportError::stream_open(5).to_string(),
        "stream 5 is already open"
    );
    assert_eq!(
        TransportError::stream_limit(64).to_string(),
        "64 streams are in flight, the limit"
    );
    assert_eq!(
        TransportError::unknown_stream(6).to_string(),
        "stream 6 is not open"
    );
    assert_eq!(
        TransportError::refused(ErrorCode::ProtocolVersion, "no protocol").to_string(),
        "refused with ProtocolVersion: no protocol"
    );
    assert_eq!(
        TransportError::unexpected_command(2, 16).to_string(),
        "command 16 arrived where command 2 was due"
    );
    assert_eq!(
        TransportError::authentication("the server did not prove itself").to_string(),
        "the server's authentication was refused: the server did not prove itself"
    );
    let error: Box<dyn Error> = Box::new(TransportError::closed());
    assert!(error.source().is_none());
}
