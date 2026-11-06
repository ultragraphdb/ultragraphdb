/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::util::Splittable;

use crate::types::compio_sim_stream::CompioSimStream;

/// A simulated stream is duplex, as a socket is: its two halves are the same end, a read pending on one holding up no
/// write on the other.
impl Splittable for CompioSimStream {
    type ReadHalf = Self;
    type WriteHalf = Self;

    fn split(self) -> (Self, Self) {
        (self.clone(), self)
    }
}
