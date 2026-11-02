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
    Malformed {
        /// The part of the message that does not parse.
        part: &'static str,
    },
    /// The client asked for channel binding, which this mechanism without TLS does not offer.
    ChannelBinding,
    /// A password or name SASLprep (RFC 4013) refuses: a prohibited or unassigned character, or bidirectional text
    /// mixed.
    Prepare,
    /// The iteration count, `iterations`, is outside [`crate::MIN_ITERATIONS`] to [`crate::MAX_ITERATIONS`]: a
    /// server's, which the client refuses to run, or a verifier's, which is never derived.
    Iterations {
        /// The iteration count refused.
        iterations: u32,
    },
    /// The server's nonce does not extend the client's.
    Nonce,
    /// The client's proof does not verify: a wrong password, or a name no principal holds.
    Proof,
    /// The server's signature does not verify: the server does not hold the password's verifier.
    ServerSignature,
    /// The server answered with its error `e=`, `reason`.
    Server {
        /// The server's error, as its `e=` attribute states it.
        reason: String,
    },
    /// A message arrived where the exchange expected none, or another.
    OutOfStep,
    /// The operating system's random source failed, `reason` its error: no nonce was drawn.
    Random {
        /// The random source's error.
        reason: String,
    },
}

/// Why a SCRAM message or password was refused; `.0` classifies it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScramError(pub ScramErrorEnum);

impl ScramError {
    /// A [`ScramErrorEnum::Malformed`] naming `part`.
    pub fn malformed(part: &'static str) -> Self {
        Self(ScramErrorEnum::Malformed { part })
    }

    /// A [`ScramErrorEnum::ChannelBinding`].
    pub fn channel_binding() -> Self {
        Self(ScramErrorEnum::ChannelBinding)
    }

    /// A [`ScramErrorEnum::Prepare`].
    pub fn prepare() -> Self {
        Self(ScramErrorEnum::Prepare)
    }

    /// A [`ScramErrorEnum::Iterations`] of `iterations`.
    pub fn iterations(iterations: u32) -> Self {
        Self(ScramErrorEnum::Iterations { iterations })
    }

    /// A [`ScramErrorEnum::Nonce`].
    pub fn nonce() -> Self {
        Self(ScramErrorEnum::Nonce)
    }

    /// A [`ScramErrorEnum::Proof`].
    pub fn proof() -> Self {
        Self(ScramErrorEnum::Proof)
    }

    /// A [`ScramErrorEnum::ServerSignature`].
    pub fn server_signature() -> Self {
        Self(ScramErrorEnum::ServerSignature)
    }

    /// A [`ScramErrorEnum::Server`] for the server's `reason`.
    pub fn server(reason: impl Into<String>) -> Self {
        Self(ScramErrorEnum::Server {
            reason: reason.into(),
        })
    }

    /// A [`ScramErrorEnum::OutOfStep`].
    pub fn out_of_step() -> Self {
        Self(ScramErrorEnum::OutOfStep)
    }

    /// A [`ScramErrorEnum::Random`] for the random source's `reason`.
    pub fn random(reason: impl Into<String>) -> Self {
        Self(ScramErrorEnum::Random {
            reason: reason.into(),
        })
    }
}
