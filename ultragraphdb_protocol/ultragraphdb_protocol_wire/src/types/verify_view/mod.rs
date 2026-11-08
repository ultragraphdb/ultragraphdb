/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod verify_view_body_view;

/// The body of a received Verify frame, read in place; see [`crate::Verify`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyView<'a> {
    name: &'a str,
    auth_message: &'a str,
    proof: &'a [u8; 32],
    verified: bool,
    principal: u64,
    signature: Option<&'a [u8; 32]>,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<VerifyView<'static>>() <= 64);

impl<'a> VerifyView<'a> {
    /// The principal's name.
    #[must_use]
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// The SCRAM AuthMessage.
    #[must_use]
    pub fn auth_message(&self) -> &'a str {
        self.auth_message
    }

    /// The ClientProof.
    #[must_use]
    pub fn proof(&self) -> &'a [u8; 32] {
        self.proof
    }

    /// Whether the proof holds.
    #[must_use]
    pub fn verified(&self) -> bool {
        self.verified
    }

    /// The principal of a verified reply; 0 otherwise.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The ServerSignature of a verified reply.
    #[must_use]
    pub fn signature(&self) -> Option<&'a [u8; 32]> {
        self.signature
    }
}
