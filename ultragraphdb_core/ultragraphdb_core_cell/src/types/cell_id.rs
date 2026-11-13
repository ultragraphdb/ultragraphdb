/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::s2::{ancestor, face_ij, leaf_id, xyz_from_degrees};

/// The deepest level: a leaf cell, about a centimetre across.
pub const MAX_LEVEL: u8 = 30;

/// An S2 cell's identifier: the cube face in the top three bits, the cell's position along the face's Hilbert curve
/// below them, and one marker bit after the position, whose place gives the level. Identifiers order as the curve
/// runs, so a cell's descendants are one range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellId(u64);

impl CellId {
    /// The cell `id` names; [`CellId::level`] tells whether it names one.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// The identifier.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// The leaf cell holding the point at `lat` and `lng` degrees; `None` when either is NaN or infinite.
    #[must_use]
    pub fn at(lat: f64, lng: f64) -> Option<Self> {
        if !(lat.is_finite() && lng.is_finite()) {
            return None;
        }
        let (face, i, j) = face_ij(xyz_from_degrees(lat, lng));
        Some(Self(leaf_id(face, i, j)))
    }

    /// The cell's level, 0 for a face to 30 for a leaf; `None` for an identifier that is no cell, whose face is past
    /// 5 or whose level marker is missing or misplaced.
    #[must_use]
    pub fn level(self) -> Option<u8> {
        let marker = self.0.isolate_lowest_one();
        // The marker is the lowest set bit, at an even position from 0 (a leaf) to 60 (a face).
        if self.0 >> 61 > 5 || marker & 0x1555_5555_5555_5555 == 0 {
            return None;
        }
        u8::try_from(marker.trailing_zeros() / 2)
            .ok()
            .and_then(|below| MAX_LEVEL.checked_sub(below))
    }

    /// The cube face the cell lies on, 0 to 5; `None` for an identifier that is no cell.
    #[must_use]
    pub fn face(self) -> Option<u8> {
        self.level().and_then(|_| u8::try_from(self.0 >> 61).ok())
    }

    /// The cell's ancestor at `level`, the cell itself at its own level; `None` above its level or for an identifier
    /// that is no cell.
    #[must_use]
    pub fn parent(self, level: u8) -> Option<Self> {
        let own = self.level()?;
        (level <= own).then(|| Self(ancestor(self.0, level)))
    }

    /// Every ancestor of the cell, its face first; none for an identifier that is no cell.
    #[must_use]
    pub fn ancestors(self) -> Vec<Self> {
        self.level().map_or_else(Vec::new, |own| {
            (0..own).map(|at| Self(ancestor(self.0, at))).collect()
        })
    }

    /// The first and the last leaf beneath the cell: every descendant at every level lies between them, and no cell
    /// outside it does. `None` for an identifier that is no cell.
    #[must_use]
    pub fn descendant_range(self) -> Option<(Self, Self)> {
        let _own: u8 = self.level()?;
        let span = self.0.isolate_lowest_one().checked_sub(1)?;
        Some((
            Self(self.0.checked_sub(span)?),
            Self(self.0.checked_add(span)?),
        ))
    }

    /// The cell's token: its identifier in sixteen hexadecimal digits without the trailing zeros, so a coarser cell
    /// takes a shorter text, and `X` for the identifier 0.
    #[must_use]
    pub fn token(self) -> String {
        let hex = format!("{:016x}", self.0);
        let token = hex.trim_end_matches('0');
        if token.is_empty() {
            "X".to_string()
        } else {
            token.to_string()
        }
    }
}
