/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::io::util::Splittable;

use super::Carrier;

/// A socket is duplex, so its halves share no lock: a read pending on one half never holds up a write on the other.
impl Splittable for Carrier {
    type ReadHalf = Self;
    type WriteHalf = Self;

    fn split(self) -> (Self, Self) {
        match self {
            Self::Tcp(stream) => {
                let (read, write) = stream.into_split();
                (Self::Tcp(read), Self::Tcp(write))
            }
            Self::Unix(stream) => {
                let (read, write) = stream.into_split();
                (Self::Unix(read), Self::Unix(write))
            }
        }
    }
}
