/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! S2's projection of the sphere onto the cube and its numbering of the cells, as the reference library computes
//! them: a point to its face and face coordinates, the quadratic projection to leaf coordinates, and the Hilbert
//! curve from leaf coordinates to a cell's position.

/// Leaf coordinates run from 0 to `MAX_SIZE - 1` along each axis of a face.
pub(crate) const MAX_SIZE: i64 = 1 << 30;

/// The Hilbert curve's quadrant order by orientation: for each orientation, the position of the quadrant at
/// `(i_bit << 1) | j_bit`.
const IJ_TO_POS: [[u8; 4]; 4] = [[0, 1, 3, 2], [0, 3, 1, 2], [2, 3, 1, 0], [2, 1, 3, 0]];

/// How the orientation changes on entering the quadrant at each position: swap the axes, invert the bits, or both.
const POS_TO_ORIENTATION: [usize; 4] = [1, 0, 0, 3];

/// The derivative of S2's minimum cell width under the quadratic projection: a level-k cell is at least
/// `MIN_WIDTH_DERIV / 2^k` radians wide.
const MIN_WIDTH_DERIV: f64 = 0.942_809_041_582_063_4;

/// A point on the unit sphere from latitude and longitude in degrees.
pub(crate) fn xyz_from_degrees(lat: f64, lng: f64) -> [f64; 3] {
    let (phi, theta) = (lat.to_radians(), lng.to_radians());
    let cos_phi = phi.cos();
    [theta.cos() * cos_phi, theta.sin() * cos_phi, phi.sin()]
}

/// A point's face, the axis of its largest component plus three where that component is negative, and its
/// coordinates on that face.
fn face_uv([x, y, z]: [f64; 3]) -> (u8, f64, f64) {
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    if ax > ay && ax > az {
        if x < 0.0 {
            (3, z / x, y / x)
        } else {
            (0, y / x, z / x)
        }
    } else if ay > az && ax <= ay {
        if y < 0.0 {
            (4, z / y, -x / y)
        } else {
            (1, -x / y, z / y)
        }
    } else if z < 0.0 {
        (5, -y / z, -x / z)
    } else {
        (2, -x / z, -y / z)
    }
}

/// The point at face coordinates `u`, `v` of `face`, not normalised.
fn xyz_from_face_uv(face: u8, u: f64, v: f64) -> [f64; 3] {
    match face {
        0 => [1.0, u, v],
        1 => [-u, 1.0, v],
        2 => [-u, -v, 1.0],
        3 => [-1.0, -v, -u],
        4 => [v, -1.0, -u],
        _ => [v, u, -1.0],
    }
}

/// The quadratic projection from a face coordinate to its cell-space coordinate.
fn uv_to_st(u: f64) -> f64 {
    if u >= 0.0 {
        0.5 * (1.0 + 3.0 * u).sqrt()
    } else {
        1.0 - 0.5 * (1.0 - 3.0 * u).sqrt()
    }
}

/// A cell-space coordinate's leaf coordinate. The float is clamped to a face's leaves before it is converted, so
/// the conversion is exact.
fn st_to_ij(s: f64) -> i64 {
    let last = (MAX_SIZE - 1) as f64; // lossless: 2^30 - 1 is exact in a double
    (MAX_SIZE as f64 * s).floor().clamp(0.0, last) as i64
}

/// A point's face and leaf coordinates.
pub(crate) fn face_ij(p: [f64; 3]) -> (u8, i64, i64) {
    let (face, u, v) = face_uv(p);
    (face, st_to_ij(uv_to_st(u)), st_to_ij(uv_to_st(v)))
}

/// The leaf cell at leaf coordinates `i`, `j` of `face`: the face in the top three bits, the Hilbert curve's position
/// below them, and the leaf's marker bit.
pub(crate) fn leaf_id(face: u8, i: i64, j: i64) -> u64 {
    let mut orientation = usize::from(face & 1);
    let mut position: u64 = 0;
    for k in (0..30).rev() {
        let bit = |v: i64| usize::from((v >> k) & 1 == 1);
        let quadrant = (bit(i) << 1) | bit(j);
        let at = IJ_TO_POS
            .get(orientation)
            .and_then(|order| order.get(quadrant))
            .copied()
            .unwrap_or(0);
        position = (position << 2) | u64::from(at);
        orientation ^= POS_TO_ORIENTATION
            .get(usize::from(at))
            .copied()
            .unwrap_or(0);
    }
    (u64::from(face) << 61) | (position << 1) | 1
}

/// The leaf cell at leaf coordinates that may lie just beyond the face, wrapped onto the face they fall on, as the
/// reference library's `FromFaceIJWrap` does.
fn wrapped_leaf_id(face: u8, i: i64, j: i64) -> u64 {
    let scale = 1.0 / MAX_SIZE as f64;
    let limit = 1.0 + f64::EPSILON;
    let centre = |at: i64| {
        let at = at.clamp(-1, MAX_SIZE);
        (scale * ((at << 1) + 1 - MAX_SIZE) as f64).clamp(-limit, limit)
    };
    let (face, u, v) = face_uv(xyz_from_face_uv(face, centre(i), centre(j)));
    leaf_id(face, st_to_ij(0.5 * (u + 1.0)), st_to_ij(0.5 * (v + 1.0)))
}

/// The leaf cell at `i`, `j`, on `face` where `same` holds and wrapped onto its neighbour otherwise.
fn leaf_id_same(face: u8, i: i64, j: i64, same: bool) -> u64 {
    if same {
        leaf_id(face, i, j)
    } else {
        wrapped_leaf_id(face, i, j)
    }
}

/// The ancestor of a valid cell at `level`, at most 30.
pub(crate) const fn ancestor(id: u64, level: u8) -> u64 {
    let lsb = 1u64 << (2 * (30 - level as u32));
    (id & lsb.wrapping_neg()) | lsb
}

/// The cells at `level`, below 30, that meet at the vertex nearest the leaf at `i`, `j` of `face`: four, or three at
/// a cube corner (the reference library's `VertexNeighbors`).
pub(crate) fn vertex_neighbours(face: u8, i: i64, j: i64, level: u8) -> Vec<u64> {
    let half = 1i64 << (30 - (u32::from(level) + 1));
    let size = half << 1;
    let (i_offset, i_same) = if i & half != 0 {
        (size, i + size < MAX_SIZE)
    } else {
        (-size, i - size >= 0)
    };
    let (j_offset, j_same) = if j & half != 0 {
        (size, j + size < MAX_SIZE)
    } else {
        (-size, j - size >= 0)
    };
    let mut cells = vec![
        ancestor(leaf_id(face, i, j), level),
        ancestor(leaf_id_same(face, i + i_offset, j, i_same), level),
        ancestor(leaf_id_same(face, i, j + j_offset, j_same), level),
    ];
    if i_same || j_same {
        cells.push(ancestor(
            leaf_id_same(face, i + i_offset, j + j_offset, i_same && j_same),
            level,
        ));
    }
    cells
}

/// The finest level whose cells are at least `angle` radians wide; 30 for no width at all, and 0 where even a face is
/// narrower.
pub(crate) fn level_for_min_width(angle: f64) -> i64 {
    // The binary exponent of how many times the angle fits in a face's width, which is the level rounded down: the
    // reference library's `GetLevelForMinValue`.
    let ratio = MIN_WIDTH_DERIV / angle;
    let exponent = i64::try_from((ratio.to_bits() >> 52) & 0x7ff).unwrap_or(0) - 1023;
    exponent.clamp(0, 30)
}
