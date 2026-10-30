/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::Display;

/// A client's side of a SASL mechanism (RFC 4422): the messages it sends and its check of the server's, the
/// mechanism's own bytes. [`crate::ClientAuthentication`] carries them in Authenticate frames.
pub trait ClientMechanism {
    /// Why the mechanism refused a server's message, for the client's error.
    type Error: Display;

    /// The mechanism's SASL name, which the Welcome lists.
    fn name(&self) -> &str;

    /// The client's first message, step 0.
    fn start(&mut self) -> Result<Vec<u8>, Self::Error>;

    /// The client's answer to the server's message `challenge`, the step after it.
    fn answer(&mut self, challenge: &[u8]) -> Result<Vec<u8>, Self::Error>;

    /// The server's final message checked: the server proved itself, or the mechanism's error.
    fn finish(&mut self, outcome: &[u8]) -> Result<(), Self::Error>;
}
