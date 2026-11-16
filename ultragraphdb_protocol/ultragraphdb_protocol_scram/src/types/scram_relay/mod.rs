/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod scram_relay_debug;

use crate::errors::scram_error::ScramError;
use crate::utils::crypto::prepare;
use crate::utils::message::{attribute, decode, encode, unescape, valid_nonce};

/// What a relay has read and built of an exchange once its server-first-message is built: the channel binding the
/// client-final-message must carry, the two messages the AuthMessage opens with, and the exchange's nonce. Its `Debug`
/// holds the length of each, never its text.
#[derive(Clone, PartialEq, Eq)]
struct Exchange {
    binding: String,
    client_first_bare: String,
    nonce: String,
    server_first: String,
}

/// Where a relay's exchange stands. Its `Debug` holds the length of each message and nonce it keeps, never its text.
#[derive(Clone, PartialEq, Eq)]
enum RelayState {
    Initial,
    Read {
        binding: String,
        client_first_bare: String,
        nonce: String,
    },
    /// The server-first-message is built; `answered` once a client-final-message was read, which a
    /// server-final-message answers.
    Challenged {
        exchange: Exchange,
        answered: bool,
    },
    Done,
}

/// The endpoint's half of a server's SCRAM-SHA-256 exchange (RFC 5802 §5), which holds no key: it reads the
/// client-first-message for the name to ask for, builds the server-first-message from the salt and iteration count it
/// is handed, reads the client-final-message into the AuthMessage and the ClientProof to have checked, and builds the
/// server-final-message from the ServerSignature it is handed. The keys stay with whoever checks the proof:
/// [`crate::ScramVerifier::sign`], in the system group (`PROTOCOL.md` §13.8, §13.9). Its `Debug` holds where the
/// exchange stands and the length of each message and nonce it keeps, never their text.
#[derive(Clone, PartialEq, Eq)]
pub struct ScramRelay {
    nonce: String,
    state: RelayState,
}

impl ScramRelay {
    /// An exchange whose server nonce part is `nonce`, printable ASCII without a comma.
    #[must_use]
    pub fn new(nonce: &str) -> Self {
        Self {
            nonce: nonce.to_string(),
            state: RelayState::Initial,
        }
    }

    /// The SASLprep-prepared name `client_first` asks for, the name a Challenge carries; the message is read as
    /// [`crate::ScramServer::first`] reads it.
    ///
    /// # Errors
    ///
    /// [`ScramError::malformed`] for a header, name or nonce the message does not hold in its form,
    /// [`ScramError::channel_binding`] for a client asking for channel binding, [`ScramError::prepare`] for a name
    /// SASLprep refuses, or [`ScramError::out_of_step`] after a first message was read.
    pub fn first(&mut self, client_first: &[u8]) -> Result<String, ScramError> {
        if self.state != RelayState::Initial {
            return Err(ScramError::out_of_step());
        }
        let (flag, bare, name) = read_name(client_first)?;
        let client_nonce = attribute(bare.split(',').nth(1), 'r')
            .filter(|nonce| valid_nonce(nonce))
            .ok_or(ScramError::malformed("nonce"))?;
        self.state = RelayState::Read {
            binding: encode(format!("{flag},,").as_bytes()),
            client_first_bare: bare.to_string(),
            nonce: format!("{client_nonce}{}", self.nonce),
        };
        Ok(name)
    }

    /// The server-first-message of the name's `salt` and `iterations`, as a Challenge's reply hands them:
    /// `r=<client nonce + server nonce>,s=<salt>,i=<iterations>`.
    ///
    /// # Errors
    ///
    /// [`ScramError::out_of_step`] before the client-first-message is read, or once this message was built.
    pub fn challenge(&mut self, salt: &[u8], iterations: u32) -> Result<Vec<u8>, ScramError> {
        let RelayState::Read {
            binding,
            client_first_bare,
            nonce,
        } = &self.state
        else {
            return Err(ScramError::out_of_step());
        };
        let server_first = format!("r={nonce},s={},i={iterations}", encode(salt));
        self.state = RelayState::Challenged {
            exchange: Exchange {
                binding: binding.clone(),
                client_first_bare: client_first_bare.clone(),
                nonce: nonce.clone(),
                server_first: server_first.clone(),
            },
            answered: false,
        };
        Ok(server_first.into_bytes())
    }

    /// The AuthMessage and the ClientProof of `client_final`, the two a Verify carries; the message is read as
    /// [`crate::ScramServer::last`] reads it.
    ///
    /// # Errors
    ///
    /// [`ScramError::malformed`] for a message or proof not in its form, [`ScramError::channel_binding`] for another
    /// channel binding than the first message's, [`ScramError::nonce`] for another nonce, [`ScramError::proof`] for a
    /// proof of other than 32 bytes, or [`ScramError::out_of_step`] before the server-first-message is built or once
    /// the server-final-message is.
    pub fn last(&mut self, client_final: &[u8]) -> Result<(String, [u8; 32]), ScramError> {
        let RelayState::Challenged { exchange, answered } = &mut self.state else {
            return Err(ScramError::out_of_step());
        };
        let text = std::str::from_utf8(client_final).map_err(|_| ScramError::malformed("proof"))?;
        let (without_proof, proof) = text
            .rsplit_once(",p=")
            .ok_or(ScramError::malformed("proof"))?;
        let proof = decode(proof, "proof")?;
        let mut parts = without_proof.split(',');
        let channel =
            attribute(parts.next(), 'c').ok_or(ScramError::malformed("channel binding"))?;
        if channel != exchange.binding {
            return Err(ScramError::channel_binding());
        }
        if attribute(parts.next(), 'r') != Some(exchange.nonce.as_str()) {
            return Err(ScramError::nonce());
        }
        // A proof of another length holds for no key: refused as a wrong proof, the server's answer to one.
        let proof: [u8; 32] = proof.try_into().map_err(|_| ScramError::proof())?;
        let auth_message = format!(
            "{},{},{without_proof}",
            exchange.client_first_bare, exchange.server_first
        );
        *answered = true;
        Ok((auth_message, proof))
    }

    /// The server-final-message of `signature`, the ServerSignature a verified Verify's reply hands: `v=<signature>`.
    ///
    /// # Errors
    ///
    /// [`ScramError::out_of_step`] before a client-final-message is read, or once this message was built.
    pub fn finish(&mut self, signature: &[u8; 32]) -> Result<Vec<u8>, ScramError> {
        let RelayState::Challenged { answered: true, .. } = self.state else {
            return Err(ScramError::out_of_step());
        };
        self.state = RelayState::Done;
        Ok(format!("v={}", encode(signature)).into_bytes())
    }
}

/// The GS2 flag, the bare message and the SASLprep-prepared name of `client_first` (RFC 5802 §7), read in that order:
/// the header refused as [`ScramError::malformed`], or [`ScramError::channel_binding`] for a client asking for channel
/// binding; the name as [`ScramError::malformed`] or [`ScramError::prepare`].
pub(crate) fn read_name(client_first: &[u8]) -> Result<(&str, &str, String), ScramError> {
    let text =
        std::str::from_utf8(client_first).map_err(|_| ScramError::malformed("channel binding"))?;
    let mut header = text.splitn(3, ',');
    let (flag, authorization, bare) = match (header.next(), header.next(), header.next()) {
        (Some(flag), Some(authorization), Some(bare)) => (flag, authorization, bare),
        _ => return Err(ScramError::malformed("channel binding")),
    };
    match flag {
        "n" | "y" => {}
        f if f.starts_with("p=") => return Err(ScramError::channel_binding()),
        _ => return Err(ScramError::malformed("channel binding")),
    }
    if !authorization.is_empty() {
        return Err(ScramError::malformed("authorization identity"));
    }
    let name = attribute(bare.split(',').next(), 'n').ok_or(ScramError::malformed("name"))?;
    Ok((flag, bare, prepare(&unescape(name)?)?))
}
