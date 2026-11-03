/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod verify_body_builder;

/// The second half of verifying a principal's SCRAM-SHA-256 proof (`PROTOCOL.md` §13.9): the frontend sends the
/// principal's name, the AuthMessage and the ClientProof; the metadata group's host answers whether the proof holds,
/// the principal and, when it holds, the ServerSignature.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `verified` | u8 boolean; 0 in a request |
/// | 8 | `principal` | u64: the principal in a verified reply; 0 otherwise |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the principal's name, UTF-8, prepared with SASLprep |
/// | 1 | the AuthMessage, UTF-8 |
/// | 2 | the ClientProof, 32 bytes |
/// | 3 | the ServerSignature, 32 bytes in a verified reply; empty otherwise |
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Verify {
    name: String,
    auth_message: String,
    proof: [u8; 32],
    verified: Option<(u64, [u8; 32])>,
}

impl Verify {
    /// The request: the principal `name`, the SCRAM `auth_message` and the client's `proof`; also the reply of a proof
    /// that does not hold.
    pub fn request(
        name: impl Into<String>,
        auth_message: impl Into<String>,
        proof: [u8; 32],
    ) -> Self {
        Self {
            name: name.into(),
            auth_message: auth_message.into(),
            proof,
            verified: None,
        }
    }

    /// The reply of a proof that holds: the `principal` and the `signature` the frontend sends in its
    /// server-final-message.
    pub fn with_verified(mut self, principal: u64, signature: [u8; 32]) -> Self {
        self.verified = Some((principal, signature));
        self
    }

    /// The principal's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The SCRAM AuthMessage.
    pub fn auth_message(&self) -> &str {
        &self.auth_message
    }

    /// The ClientProof.
    pub fn proof(&self) -> &[u8; 32] {
        &self.proof
    }

    /// Whether the proof holds.
    pub fn verified(&self) -> bool {
        self.verified.is_some()
    }

    /// The principal of a verified reply; 0 otherwise.
    pub fn principal(&self) -> u64 {
        self.verified.map_or(0, |(principal, _)| principal)
    }

    /// The ServerSignature of a verified reply.
    pub fn signature(&self) -> Option<&[u8; 32]> {
        self.verified.as_ref().map(|(_, signature)| signature)
    }
}
