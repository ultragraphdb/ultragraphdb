/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_transport::{
    AuthenticationStep, ClientAuthentication, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    Authenticate, BodyBuilder, Command, Credit, ErrorCode, ErrorReply, FrameView, Header,
    MAX_FRAME_SIZE,
};

use crate::utils::mechanism::{TEST, TestClient};

const CLUSTER: u64 = 3;

fn frame<B: BodyBuilder>(body: &B) -> Vec<u8> {
    body.frame(Header::new(CLUSTER, Command::Abort)).unwrap()
}

fn receive(
    auth: &mut ClientAuthentication<TestClient>,
    bytes: &[u8],
) -> Result<AuthenticationStep, TransportError> {
    auth.receive(&FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE).unwrap())
}

/// An exchange of `alice` past its start.
fn started() -> ClientAuthentication<TestClient> {
    let mut auth = ClientAuthentication::new(TestClient::new("alice", "pw"));
    auth.start().unwrap();
    auth
}

#[test]
fn test_an_exchange_numbers_its_messages_and_ends_with_the_servers_final() {
    let mut auth = ClientAuthentication::new(TestClient::new("alice", "pw"));
    assert_eq!(auth.mechanism().name_of(), TEST);
    assert_eq!(
        auth.start(),
        Ok(Authenticate::new(0, TEST, b"user=alice".to_vec()))
    );
    let challenge = frame(&Authenticate::new(1, "", b"challenge".to_vec()));
    assert_eq!(
        receive(&mut auth, &challenge),
        Ok(AuthenticationStep::Send(Authenticate::new(
            2,
            "",
            b"pass=pw".to_vec()
        )))
    );
    let last = frame(&Authenticate::new(3, "", b"welcome".to_vec()).with_final(7, 99));
    assert_eq!(
        receive(&mut auth, &last),
        Ok(AuthenticationStep::Done {
            principal: 7,
            expires: 99
        })
    );
}

#[test]
fn test_the_servers_error_is_a_refusal_naming_its_code() {
    let mut auth = started();
    let refused = frame(&ErrorReply::new(
        ErrorCode::AuthenticationFailed,
        "authentication failed",
    ));
    assert_eq!(
        receive(&mut auth, &refused),
        Err(TransportError::Refused(
            ErrorCode::AuthenticationFailed,
            "authentication failed"
        ))
    );
}

#[test]
fn test_a_message_out_of_step_or_another_command_is_a_protocol_error() {
    // Step 3 where 1 is due.
    let mut auth = started();
    let early = frame(&Authenticate::new(3, "", b"welcome".to_vec()).with_final(7, 0));
    assert_eq!(
        receive(&mut auth, &early),
        Err(TransportError::Protocol(ProtocolError::FieldValue(
            18, "step", 3
        )))
    );
    // A final message at step 1 is the server's at its step, and taken.
    let mut auth = started();
    let quick = frame(&Authenticate::new(1, "", b"welcome".to_vec()).with_final(9, 0));
    assert_eq!(
        receive(&mut auth, &quick),
        Ok(AuthenticationStep::Done {
            principal: 9,
            expires: 0
        })
    );
    // Before the exchange starts, any message is out of step.
    let mut fresh = ClientAuthentication::new(TestClient::new("alice", "pw"));
    let challenge = frame(&Authenticate::new(1, "", b"challenge".to_vec()));
    assert!(matches!(
        receive(&mut fresh, &challenge).unwrap_err().0,
        TransportErrorEnum::Protocol {
            error: ProtocolError(ProtocolErrorEnum::FieldValue { field: "step", .. })
        }
    ));
    // Another command on stream 0.
    let mut auth = started();
    let credit = frame(&Credit::new(1));
    assert_eq!(
        receive(&mut auth, &credit),
        Err(TransportError::UnexpectedCommand(18, 14))
    );
}

#[test]
fn test_the_mechanisms_refusal_is_an_authentication_error() {
    let mut auth = started();
    let odd = frame(&Authenticate::new(1, "", b"something else".to_vec()));
    assert_eq!(
        receive(&mut auth, &odd),
        Err(TransportError::Authentication("an unexpected challenge"))
    );
    // A final message the mechanism does not take: the server did not prove itself.
    let mut auth = started();
    let challenge = frame(&Authenticate::new(1, "", b"challenge".to_vec()));
    receive(&mut auth, &challenge).unwrap();
    let forged = frame(&Authenticate::new(3, "", b"forged".to_vec()).with_final(7, 0));
    assert_eq!(
        receive(&mut auth, &forged),
        Err(TransportError::Authentication(
            "the server did not prove itself"
        ))
    );
}

/// A mechanism that cannot start, as a login whose secret is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Unstartable;

impl ultragraphdb_protocol_transport::ClientMechanism for Unstartable {
    type Error = &'static str;

    fn name(&self) -> &str {
        TEST
    }

    fn start(&mut self) -> Result<Vec<u8>, &'static str> {
        Err("no secret to start with")
    }

    fn answer(&mut self, _challenge: &[u8]) -> Result<Vec<u8>, &'static str> {
        Ok(Vec::new())
    }

    fn finish(&mut self, _outcome: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }
}

#[test]
fn test_a_mechanism_that_cannot_start_is_an_authentication_error() {
    let mut auth = ClientAuthentication::new(Unstartable);
    assert_eq!(
        auth.start(),
        Err(TransportError::Authentication("no secret to start with"))
    );
    // Not started, so the server's step 1 is out of step.
    let challenge = Authenticate::new(1, "", b"challenge".to_vec())
        .frame(Header::new(CLUSTER, Command::Abort))
        .unwrap();
    assert_eq!(
        auth.receive(&FrameView::validate(&challenge, CLUSTER, MAX_FRAME_SIZE).unwrap()),
        Err(TransportError::Protocol(ProtocolError::FieldValue(
            18, "step", 1
        )))
    );
}

#[test]
fn test_each_challenge_is_answered_at_the_step_after_it() {
    // Two challenges before the final message: the client's messages take steps 0, 2, 4 and the server's 1, 3, 5
    // (PROTOCOL.md section 5.3).
    let mut auth = started();
    for (challenge, answer) in [(1, 2), (3, 4)] {
        let message = frame(&Authenticate::new(challenge, "", b"challenge".to_vec()));
        assert_eq!(
            receive(&mut auth, &message),
            Ok(AuthenticationStep::Send(Authenticate::new(
                answer,
                "",
                b"pass=pw".to_vec()
            )))
        );
    }
    // Step 3 again, where 5 is due.
    let repeated = frame(&Authenticate::new(3, "", b"challenge".to_vec()));
    let mut again = started();
    receive(
        &mut again,
        &frame(&Authenticate::new(1, "", b"challenge".to_vec())),
    )
    .unwrap();
    receive(
        &mut again,
        &frame(&Authenticate::new(3, "", b"challenge".to_vec())),
    )
    .unwrap();
    assert_eq!(
        receive(&mut again, &repeated),
        Err(TransportError::Protocol(ProtocolError::FieldValue(
            18, "step", 3
        )))
    );
    let last = frame(&Authenticate::new(5, "", b"welcome".to_vec()).with_final(7, 0));
    assert_eq!(
        receive(&mut auth, &last),
        Ok(AuthenticationStep::Done {
            principal: 7,
            expires: 0
        })
    );
    // The exchange is over: any later message is out of step, the final message repeated among them.
    assert_eq!(
        receive(&mut auth, &last),
        Err(TransportError::Protocol(ProtocolError::FieldValue(
            18, "step", 5
        )))
    );
}

#[test]
fn test_a_server_message_breaking_its_bodys_rules_is_a_protocol_error() {
    // A message after step 0 names no mechanism (PROTOCOL.md section 5.3): one naming `TEST`, 4 bytes, is refused.
    let mut auth = started();
    let named = frame(&Authenticate::new(1, TEST, b"challenge".to_vec()));
    assert_eq!(
        receive(&mut auth, &named),
        Err(TransportError::Protocol(ProtocolError::FieldValue(
            18,
            "mechanism",
            4
        )))
    );
}
