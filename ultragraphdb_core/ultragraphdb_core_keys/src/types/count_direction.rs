/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Which end a count index counts: the objects of each subject, or the subjects of each object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CountDirection {
    Forward,
    Reverse,
}

impl CountDirection {
    pub fn byte(self) -> u8 {
        match self {
            CountDirection::Forward => 0,
            CountDirection::Reverse => 1,
        }
    }

    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(CountDirection::Forward),
            1 => Some(CountDirection::Reverse),
            _ => None,
        }
    }
}
