/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::{Answer, Session};
use ultragraphdb_protocol_scram::ScramServer;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};

use crate::types::asked_credentials::AskedCredentials;
use crate::types::owner_handle::OwnerHandle;

/// One client connection: its authentication against the principals the owner holds, which it asks for by message on
/// every authentication. A client's requests arrive here with the frontend's endpoint.
#[derive(Debug)]
pub struct ClientSession {
    owner: OwnerHandle,
    credentials: AskedCredentials,
}

impl ClientSession {
    /// A session asking `owner`, filling `credentials` for the connection's machine.
    pub fn new(owner: OwnerHandle, credentials: AskedCredentials) -> Self {
        Self { owner, credentials }
    }
}

impl Session for ClientSession {
    async fn authenticating(&mut self, first: &[u8]) {
        // A malformed message asks for nothing, and the machine refuses it.
        if let Ok(name) = ScramServer::name_in(first) {
            let held = self.owner.verifier(name.clone()).await;
            self.credentials.answered(name, held);
        }
    }

    async fn answer(&mut self, _stream: u64, frame: ReceivedFrame) -> Answer {
        Answer::Refuse(ErrorReply::new(
            ErrorCode::Unavailable,
            format!("command {} is not served here yet", frame.command()),
        ))
    }
}
