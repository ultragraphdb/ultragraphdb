/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod held_body_builder;

/// A partition holds a Write or an Operation sent on a Bind stream (`PROTOCOL.md` §13.20): its references registered and
/// its changes in the transaction. The frontend takes it as the request's answer and relays nothing; the stream stays
/// open. The command header is all zero, with no sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Held;
