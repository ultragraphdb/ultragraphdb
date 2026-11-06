/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a read's task hands its connection's session.
#[derive(Debug)]
pub(crate) enum Pushed {
    /// A frame of the read's stream.
    Frame(u64, Vec<u8>),
    /// The read ended.
    Ended(u64),
}
