/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::Debug;

use compio::io::util::Splittable;
use compio::io::{AsyncRead, AsyncWrite};

/// A stream the client splits into its two halves once open, so a frame being read holds up no write: a socket's halves
/// share no lock, as [`ultragraphdb_protocol_transport_compio::Carrier`]'s and a TLS stream's over one do. The client reads
/// each frame on a task of its own, so the receiving half is `'static`. Every such stream is one; nothing implements it
/// by hand.
pub trait SplitStream:
    AsyncRead + AsyncWrite + Splittable<ReadHalf = Self::Read, WriteHalf = Self::Write>
{
    /// The receiving half.
    type Read: AsyncRead + Debug + 'static;
    /// The sending half.
    type Write: AsyncWrite + Debug;
}

impl<S> SplitStream for S
where
    S: AsyncRead + AsyncWrite + Splittable,
    S::ReadHalf: AsyncRead + Debug + 'static,
    S::WriteHalf: AsyncWrite + Debug,
{
    type Read = S::ReadHalf;
    type Write = S::WriteHalf;
}
