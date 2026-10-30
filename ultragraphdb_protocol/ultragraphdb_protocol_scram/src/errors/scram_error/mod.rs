/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod scram_error_display;
mod scram_error_error;

/// Why a SCRAM message or password was refused. A server's refusal of an exchange says nothing of which rule failed
/// to the client (code 19); the variant is the server's, for its audit, and the client's, for its error.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScramErrorEnum {
    /// A message breaks RFC 5802's grammar: `part` names the part that does not parse.
    Malformed { part: &'static str },
    /// The client asked for channel binding, which this mechanism without TLS does not offer.
    ChannelBinding,
    /// A password or name SASLprep (RFC 4013) refuses: a prohibited or unassigned character, or bidirectional text
    /// mixed.
    Prepare,
    /// The server's iteration count, `iterations`, is below [`crate::MIN_ITERATIONS`].
    Iterations { iterations: u32 },
    /// The server's nonce does not extend the client's.
    Nonce,
    /// The client's proof does not verify: a wrong password, or a name no principal holds.
    Proof,
    /// The server's signature does not verify: the server does not hold the password's verifier.
    ServerSignature,
    /// The server answered with its error `e=`, `reason`.
    Server { reason: String },
    /// A message arrived where the exchange expected none, or another.
    OutOfStep,
}

/// Why a SCRAM message or password was refused; `.0` classifies it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScramError(pub ScramErrorEnum);

impl ScramError {
    #[allow(non_snake_case)]
    pub fn Malformed(part: &'static str) -> Self {
        Self(ScramErrorEnum::Malformed { part })
    }

    #[allow(non_snake_case)]
    pub fn ChannelBinding() -> Self {
        Self(ScramErrorEnum::ChannelBinding)
    }

    #[allow(non_snake_case)]
    pub fn Prepare() -> Self {
        Self(ScramErrorEnum::Prepare)
    }

    #[allow(non_snake_case)]
    pub fn Iterations(iterations: u32) -> Self {
        Self(ScramErrorEnum::Iterations { iterations })
    }

    #[allow(non_snake_case)]
    pub fn Nonce() -> Self {
        Self(ScramErrorEnum::Nonce)
    }

    #[allow(non_snake_case)]
    pub fn Proof() -> Self {
        Self(ScramErrorEnum::Proof)
    }

    #[allow(non_snake_case)]
    pub fn ServerSignature() -> Self {
        Self(ScramErrorEnum::ServerSignature)
    }

    #[allow(non_snake_case)]
    pub fn Server(reason: impl Into<String>) -> Self {
        Self(ScramErrorEnum::Server {
            reason: reason.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn OutOfStep() -> Self {
        Self(ScramErrorEnum::OutOfStep)
    }
}
