/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::cell_id::CellId;
use crate::utils::s2::{
    face_ij, leaf_id, level_for_min_width, vertex_neighbours, xyz_from_degrees,
};

/// The radius S2 takes for the Earth, in metres, which turns a distance on the ground into the angle the cell levels
/// are measured in.
pub const EARTH_RADIUS_METRES: f64 = 6_371_010.0;

/// The cells a point at `lat` and `lng` degrees is filed under when it is known to within `radius_metres`: its leaf
/// cell for a radius of zero or below; otherwise the cells, at the finest level at least twice the radius wide, that
/// meet at the cell vertex nearest the point — four, or three at a cube corner — which is where S2's own coverer
/// starts for a small circle. A radius wider than a face gives all six faces. `None` when a coordinate or the radius
/// is NaN or infinite.
#[must_use]
pub fn cells_around(lat: f64, lng: f64, radius_metres: f64) -> Option<Vec<CellId>> {
    if !(lat.is_finite() && lng.is_finite() && radius_metres.is_finite()) {
        return None;
    }
    let (face, i, j) = face_ij(xyz_from_degrees(lat, lng));
    if radius_metres <= 0.0 {
        return Some(vec![CellId::new(leaf_id(face, i, j))]);
    }
    // The finest level at least twice the radius wide is one coarser than the finest at least the radius wide.
    let level = level_for_min_width(radius_metres / EARTH_RADIUS_METRES) - 1;
    let cells = match u8::try_from(level) {
        Ok(level) => vertex_neighbours(face, i, j, level),
        Err(_) => (0..6).map(|every: u64| (every << 61) | (1 << 60)).collect(),
    };
    Some(cells.into_iter().map(CellId::new).collect())
}
