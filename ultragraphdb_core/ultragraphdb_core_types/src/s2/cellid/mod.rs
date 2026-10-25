/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/cellid.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_strconv;

use crate::s2::gomath;
use crate::s2::latlng::LatLng;
use crate::s2::point::Point;
use crate::s2::r1::R1Interval;
use crate::s2::r2::R2Rect;
use crate::s2::r3::R3Vector;
use crate::s2::stuv::{face_uv_to_xyz, ij_to_st_min, st_to_ij, st_to_uv, uv_to_st, xyz_to_face_uv};
use crate::s2::util::clamp;

mod lookup;

pub(crate) use lookup::{IJ_TO_POS, INVERT_MASK, POS_TO_IJ, POS_TO_ORIENTATION, SWAP_MASK};
use lookup::{LOOKUP_BITS, LOOKUP_IJ, LOOKUP_POS};

/// The number of bits used to encode the face number.
pub const FACE_BITS: u32 = 3;
/// The number of faces.
pub const NUM_FACES: usize = 6;
/// The number of levels needed to specify a leaf cell.
pub const MAX_LEVEL: i64 = 30;
/// The total number of position bits.
pub const POS_BITS: u32 = 2 * MAX_LEVEL as u32 + 1;
/// The size of the IJ-plane in leaf cells: MaxSize = 1 << MaxLevel.
pub(crate) const MAX_SIZE: i64 = 1 << MAX_LEVEL;

/// Uniquely identifies a cell in the S2 cell decomposition. The most significant 3 bits encode
/// the face number (0-5). The remaining 61 bits encode the position of the center of this cell
/// along the Hilbert curve on that face. The zero value and the value (1<<64)-1 are invalid
/// cell IDs. The first compares less than any valid cell ID, the second as greater than any
/// valid cell ID.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellId(pub(crate) u64);

impl CellId {
    /// An invalid cell ID guaranteed to be larger than any valid cell ID. It is used primarily
    /// by ShapeIndex. The value is also used by some S2 types when encoding data. Note that the
    /// sentinel's RangeMin == RangeMax == itself.
    pub const SENTINEL: CellId = CellId(!0u64);

    /// Wraps the given 64-bit value.
    pub const fn new(id: u64) -> CellId {
        CellId(id)
    }

    /// The 64-bit value.
    pub fn id(&self) -> u64 {
        self.0
    }

    /// Returns a cell given its face in the range \[0,5\], the 61-bit Hilbert curve position pos
    /// within that face, and the level in the range \[0,MaxLevel\]. The position in the cell ID
    /// will be truncated to correspond to the Hilbert curve position at the center of the
    /// returned cell.
    pub fn from_face_pos_level(face: usize, pos: u64, level: i64) -> CellId {
        CellId(((face as u64) << POS_BITS).wrapping_add(pos) | 1).parent(level)
    }

    /// Returns a leaf cell given its cube face (range 0..5) and IJ coordinates.
    pub fn from_face(face: usize) -> CellId {
        CellId(((face as u64) << POS_BITS) + lsb_for_level(0))
    }

    /// Returns the leaf cell containing ll.
    pub fn from_lat_lng(ll: LatLng) -> CellId {
        cell_id_from_point(Point::from_lat_lng(ll))
    }

    /// Returns a cell given a hex-encoded string of its uint64 ID, or the zero id (invalid)
    /// for any string Go's `CellIDFromToken` refuses.
    pub fn from_token(s: &str) -> CellId {
        if s.len() > 16 {
            return CellId(0);
        }
        let Ok(mut n) = go_strconv::parse_uint(s, 16) else {
            return CellId(0);
        };
        if s.len() < 16 {
            n <<= 4 * (16 - s.len()) as u32;
        }
        CellId(n)
    }

    /// Returns a hex-encoded string of the uint64 cell id, with leading zeros included but
    /// trailing zeros stripped.
    pub fn to_token(&self) -> String {
        let s = format!("{:016x}", self.0);
        let s = s.trim_end_matches('0');
        if s.is_empty() {
            return "X".to_string();
        }
        s.to_string()
    }

    /// Reports whether ci represents a valid cell.
    pub fn is_valid(&self) -> bool {
        self.face() < NUM_FACES && (self.lsb() & 0x1555555555555555 != 0)
    }

    /// Returns the cube face for this cell ID, in the range \[0,5\].
    pub fn face(&self) -> usize {
        (self.0 >> POS_BITS) as usize
    }

    /// Returns the position along the Hilbert curve of this cell ID, in the range [0,2^PosBits-1].
    pub fn pos(&self) -> u64 {
        self.0 & (!0u64 >> FACE_BITS)
    }

    /// Returns the subdivision level of this cell ID, in the range [0, MaxLevel].
    pub fn level(&self) -> i64 {
        MAX_LEVEL - (self.0.trailing_zeros() >> 1) as i64
    }

    /// Returns whether this cell ID is at the deepest level; that is, the level at which the
    /// cells are smallest.
    pub fn is_leaf(&self) -> bool {
        self.0 & 1 != 0
    }

    /// Returns the cell at the given level, which must be no greater than the current level.
    pub fn parent(&self, level: i64) -> CellId {
        let lsb = lsb_for_level(level);
        CellId((self.0 & lsb.wrapping_neg()) | lsb)
    }

    /// Returns the cell at the level immediately above ci.
    /// If ci is already a face cell, the result is undefined.
    pub(crate) fn immediate_parent(&self) -> CellId {
        let nlsb = self.lsb() << 2;
        CellId((self.0 & nlsb.wrapping_neg()) | nlsb)
    }

    /// Returns whether this is a top-level (face) cell.
    pub(crate) fn is_face(&self) -> bool {
        self.0 & (lsb_for_level(0) - 1) == 0
    }

    /// Returns the least significant bit that is set.
    pub(crate) fn lsb(&self) -> u64 {
        self.0 & self.0.wrapping_neg()
    }

    /// Returns the four immediate children of this cell. If ci is a leaf cell, it returns
    /// four identical cells that are not the children.
    pub fn children(&self) -> [CellId; 4] {
        let mut lsb = self.lsb();
        let c0 = self.0.wrapping_sub(lsb).wrapping_add(lsb >> 2);
        lsb >>= 1;
        let c1 = c0.wrapping_add(lsb);
        let c2 = c1.wrapping_add(lsb);
        let c3 = c2.wrapping_add(lsb);
        [CellId(c0), CellId(c1), CellId(c2), CellId(c3)]
    }

    /// Returns a slice of the neighboring cellIDs with vertex closest to this cell at the given
    /// level. (Normally there are four neighbors, but the closest vertex may only have three
    /// neighbors if it is one of the 8 cube vertices.)
    ///
    /// A level outside 0 to 29 gives Go's result: the offsets wrap as Go's int arithmetic does, where the cell
    /// size at level -33 is 2^63.
    pub fn vertex_neighbors(&self, level: i64) -> Vec<CellId> {
        let half_size = size_ij(level.wrapping_add(1));
        let size = half_size << 1;
        let (f, i, j, _) = self.face_ij_orientation();

        let isame: bool;
        let jsame: bool;
        let ioffset: i64;
        let joffset: i64;
        if i & half_size != 0 {
            ioffset = size;
            isame = i.wrapping_add(size) < MAX_SIZE;
        } else {
            ioffset = size.wrapping_neg();
            isame = i.wrapping_sub(size) >= 0;
        }
        if j & half_size != 0 {
            joffset = size;
            jsame = j.wrapping_add(size) < MAX_SIZE;
        } else {
            joffset = size.wrapping_neg();
            jsame = j.wrapping_sub(size) >= 0;
        }

        let mut results = vec![
            self.parent(level),
            cell_id_from_face_ij_same(f, i.wrapping_add(ioffset), j, isame).parent(level),
            cell_id_from_face_ij_same(f, i, j.wrapping_add(joffset), jsame).parent(level),
        ];

        if isame || jsame {
            results.push(
                cell_id_from_face_ij_same(
                    f,
                    i.wrapping_add(ioffset),
                    j.wrapping_add(joffset),
                    isame && jsame,
                )
                .parent(level),
            );
        }

        results
    }

    /// Returns the minimum CellID that is contained within this cell.
    pub fn range_min(&self) -> CellId {
        CellId(self.0.wrapping_sub(self.lsb().wrapping_sub(1)))
    }

    /// Returns the maximum CellID that is contained within this cell.
    pub fn range_max(&self) -> CellId {
        CellId(self.0.wrapping_add(self.lsb().wrapping_sub(1)))
    }

    /// Returns true iff the CellID contains oci.
    pub fn contains(&self, oci: CellId) -> bool {
        self.range_min().0 <= oci.0 && oci.0 <= self.range_max().0
    }

    /// Returns true iff the CellID intersects oci.
    pub fn intersects(&self, oci: CellId) -> bool {
        oci.range_min().0 <= self.range_max().0 && oci.range_max().0 >= self.range_min().0
    }

    /// Returns the center of the s2 cell on the sphere as a Point. The maximum directional
    /// error in Point (compared to the exact mathematical result) is 1.5 * dblEpsilon radians,
    /// and the maximum length error is 2 * dblEpsilon (the same as Normalize).
    pub fn point(&self) -> Point {
        Point(self.raw_point().normalize())
    }

    /// Returns the center of the s2 cell on the sphere as a LatLng.
    pub fn lat_lng(&self) -> LatLng {
        LatLng::from_point(Point(self.raw_point()))
    }

    /// Returns the first child in a traversal of the children of this cell, in Hilbert curve
    /// order.
    pub fn child_begin(&self) -> CellId {
        let ol = self.lsb();
        CellId(self.0.wrapping_sub(ol).wrapping_add(ol >> 2))
    }

    /// Returns the first cell in a traversal of children a given level deeper than this cell,
    /// in Hilbert curve order. The given level must be no smaller than the cell's level.
    pub fn child_begin_at_level(&self, level: i64) -> CellId {
        CellId(
            self.0
                .wrapping_sub(self.lsb())
                .wrapping_add(lsb_for_level(level)),
        )
    }

    /// Returns the first cell after a traversal of the children of this cell in Hilbert curve
    /// order. The returned cell may be invalid.
    pub fn child_end(&self) -> CellId {
        let ol = self.lsb();
        CellId(self.0.wrapping_add(ol).wrapping_add(ol >> 2))
    }

    /// Returns the first cell after the last child in a traversal of children a given level
    /// deeper than this cell, in Hilbert curve order. The given level must be no smaller than
    /// the cell's level. The returned cell may be invalid.
    pub fn child_end_at_level(&self, level: i64) -> CellId {
        CellId(
            self.0
                .wrapping_add(self.lsb())
                .wrapping_add(lsb_for_level(level)),
        )
    }

    /// Returns the next cell along the Hilbert curve. This is expected to be used with
    /// ChildBegin and ChildEnd, or ChildBeginAtLevel and ChildEndAtLevel.
    pub fn next(&self) -> CellId {
        CellId(self.0.wrapping_add(self.lsb() << 1))
    }

    /// Returns the previous cell along the Hilbert curve.
    pub fn prev(&self) -> CellId {
        CellId(self.0.wrapping_sub(self.lsb() << 1))
    }

    /// Returns the level of the common ancestor of the two S2 CellIDs, or None if they have no
    /// common ancestor (they are on different faces).
    pub fn common_ancestor_level(&self, other: CellId) -> Option<i64> {
        let diff_bits = (self.0 ^ other.0).max(self.lsb()).max(other.lsb());
        let msb_pos = 64 - diff_bits.leading_zeros() as i64 - 1;
        if msb_pos > 60 {
            return None;
        }
        Some((60 - msb_pos) >> 1)
    }

    /// Returns the largest cell with the same RangeMin such that RangeMax < limit.RangeMin.
    /// It returns limit if no such cell exists. This method can be used to generate a small set
    /// of CellIDs that covers a given range (a tiling). This method works with any valid
    /// CellIDs, the sentinel and the zero id.
    pub(crate) fn max_tile(&self, limit: CellId) -> CellId {
        let mut ci = *self;
        let start = ci.range_min();
        if start >= limit.range_min() {
            return limit;
        }

        if ci.range_max() >= limit {
            // The cell is too large, shrink it. Note that when generating coverings
            // of CellID ranges, this loop usually executes only once. Also because
            // ci.RangeMin() < limit.RangeMin(), we will always exit the loop by the
            // time we reach a leaf cell.
            loop {
                ci = ci.children()[0];
                if ci.range_max() < limit {
                    break;
                }
            }
            return ci;
        }

        // The cell may be too small. Grow it if necessary. Note that generally
        // this loop only iterates once.
        while !ci.is_face() {
            let parent = ci.immediate_parent();
            if parent.range_min() != start || parent.range_max() >= limit {
                break;
            }
            ci = parent;
        }
        ci
    }

    /// Returns the (face, i, j) coordinates for the leaf cell corresponding to this cell id and
    /// the orientation the i- and j-axes follow at that level. Since cells are represented by
    /// the Hilbert curve position corresponding at their center, it turns out that the cell
    /// returned by FaceIJ is always one of two leaf cells closest to the center of the cell
    /// (unless the given cell is a leaf cell itself).
    pub(crate) fn face_ij_orientation(&self) -> (usize, i64, i64, usize) {
        let f = self.face();
        let mut orientation = f & SWAP_MASK;
        let mut nbits = MAX_LEVEL as u32 - 7 * LOOKUP_BITS; // first iteration
        let mut i: i64 = 0;
        let mut j: i64 = 0;

        // Each iteration maps 8 bits of the Hilbert curve position into
        // 4 bits of "i" and "j". The lookup table transforms a key of the
        // form "ppppppppoo" to a value of the form "iiiijjjjoo", where the
        // letters [ijpo] represents bits of "i", "j", the Hilbert curve
        // position, and the Hilbert curve orientation respectively.
        //
        // On the first iteration we need to be careful to clear out the bits
        // representing the cube face.
        for k in (0..8u32).rev() {
            orientation +=
                (((self.0 >> (k * 2 * LOOKUP_BITS + 1)) as usize) & ((1 << (2 * nbits)) - 1)) << 2;
            orientation = LOOKUP_IJ[orientation];
            i += ((orientation >> (LOOKUP_BITS + 2)) as i64) << (k * LOOKUP_BITS);
            j += (((orientation >> 2) & ((1 << LOOKUP_BITS) - 1)) as i64) << (k * LOOKUP_BITS);
            orientation &= SWAP_MASK | INVERT_MASK;
            nbits = LOOKUP_BITS; // following iterations
        }

        // The position of a non-leaf cell at level "n" consists of a prefix of
        // 2*n bits that identifies the cell, followed by a suffix of
        // 2*(MaxLevel-n)+1 bits of the form 10*. If n==MaxLevel, the suffix is
        // just "1" and has no effect. Otherwise, it consists of "10", followed
        // by (MaxLevel-n-1) repetitions of "00", followed by "0". The "10" has
        // no effect, while each occurrence of "00" has the effect of reversing
        // the swapMask bit.
        if self.lsb() & 0x1111111111111110 != 0 {
            orientation ^= SWAP_MASK;
        }

        (f, i, j, orientation)
    }

    /// Returns the Face/Si/Ti coordinates of the center of the cell.
    pub(crate) fn face_si_ti(&self) -> (usize, u32, u32) {
        let (face, i, j, _) = self.face_ij_orientation();
        let delta: i64 = if self.is_leaf() {
            1
        } else if (i ^ ((self.0 >> 2) as i64)) & 1 != 0 {
            2
        } else {
            0
        };
        (face, (2 * i + delta) as u32, (2 * j + delta) as u32)
    }

    /// Returns an unnormalized r3 vector from the origin through the center of the s2 cell on
    /// the sphere.
    pub(crate) fn raw_point(&self) -> R3Vector {
        let (face, si, ti) = self.face_si_ti();
        // Go's constant 0.5 / MaxSize is 2^-31.
        const HALF_OVER_MAX_SIZE: f64 = 0.5 / MAX_SIZE as f64;
        face_uv_to_xyz(
            face,
            st_to_uv(HALF_OVER_MAX_SIZE * si as f64),
            st_to_uv(HALF_OVER_MAX_SIZE * ti as f64),
        )
    }
}

/// Returns the lowest-numbered bit that is on for cells at the given level. A level outside 0 to 30 gives Go's
/// result: Go computes the count `2*(MaxLevel-level)` with wrapping int arithmetic and shifts by it as a uint64,
/// and a shift of 64 bits or more is 0 — so level -1 gives 1<<62, and `i64::MIN` and `i64::MAX` wrap to 1<<60
/// and 1<<62.
pub(crate) fn lsb_for_level(level: i64) -> u64 {
    go_shl(MAX_LEVEL.wrapping_sub(level).wrapping_mul(2))
}

/// Returns the edge length of this CellID in (i,j)-space, Go's 0 for a level below -33 or above 30.
pub(crate) fn size_ij(level: i64) -> i64 {
    go_shl(MAX_LEVEL.wrapping_sub(level)) as i64
}

/// Go's `1 << uint64(n)`: 0 for a count of 64 or more, a negative count among them.
fn go_shl(n: i64) -> u64 {
    u32::try_from(n)
        .ok()
        .and_then(|n| 1u64.checked_shl(n))
        .unwrap_or(0)
}

/// Returns a leaf cell given its cube face (range 0..5) and IJ coordinates.
pub(crate) fn cell_id_from_face_ij(f: usize, i: i64, j: i64) -> CellId {
    // Note that this value gets shifted one bit to the left at the end
    // of the function.
    let mut n = (f as u64) << (POS_BITS - 1);
    // Alternating faces have opposite Hilbert curve orientations; this
    // is necessary in order for all faces to have a right-handed
    // coordinate system.
    let mut bits = f & SWAP_MASK;
    // Each iteration maps 4 bits of "i" and "j" into 8 bits of the Hilbert
    // curve position. The lookup table transforms a 10-bit key of the form
    // "iiiijjjjoo" to a 10-bit value of the form "ppppppppoo", where the
    // letters [ijpo] denote bits of "i", "j", Hilbert curve position, and
    // Hilbert curve orientation respectively.
    for k in (0..8u32).rev() {
        let mask: i64 = (1 << LOOKUP_BITS) - 1;
        bits += (((i >> (k * LOOKUP_BITS)) & mask) as usize) << (LOOKUP_BITS + 2);
        bits += (((j >> (k * LOOKUP_BITS)) & mask) as usize) << 2;
        bits = LOOKUP_POS[bits];
        n |= ((bits >> 2) as u64) << (k * 2 * LOOKUP_BITS);
        bits &= SWAP_MASK | INVERT_MASK;
    }
    CellId(n.wrapping_mul(2).wrapping_add(1))
}

fn cell_id_from_face_ij_wrap(f: usize, i: i64, j: i64) -> CellId {
    // Convert i and j to the coordinates of a leaf cell just beyond the
    // boundary of this face. This prevents 32-bit overflow in the case
    // of finding the neighbors of a face cell.
    let i = clamp(i, -1, MAX_SIZE);
    let j = clamp(j, -1, MAX_SIZE);

    // We want to wrap these coordinates onto the appropriate adjacent face.
    // The easiest way to do this is to convert the (i,j) coordinates to (x,y,z)
    // (which yields a point outside the normal face boundary), and then call
    // xyzToFaceUV to project back onto the correct face.
    //
    // The code below converts (i,j) to (si,ti), and then (si,ti) to (u,v) using
    // the linear projection (u=2*s-1 and v=2*t-1). (The code further below
    // converts back using the inverse projection, s=0.5*(u+1) and t=0.5*(v+1).
    // Any projection would work here, so we use the simplest.) We also clamp
    // the (u,v) coordinates so that the point is barely outside the
    // [-1,1]x[-1,1] face rectangle, since otherwise the reprojection step
    // (which divides by the new z coordinate) might change the other
    // coordinates enough so that we end up in the wrong leaf cell.
    const SCALE: f64 = 1.0 / MAX_SIZE as f64;
    let limit = gomath::nextafter(1.0, 2.0);
    let u = gomath::max(
        -limit,
        gomath::min(limit, SCALE * ((i << 1) + 1 - MAX_SIZE) as f64),
    );
    let v = gomath::max(
        -limit,
        gomath::min(limit, SCALE * ((j << 1) + 1 - MAX_SIZE) as f64),
    );

    // Find the leaf cell coordinates on the adjacent face, and convert
    // them to a cell id at the appropriate level.
    let (f, u, v) = xyz_to_face_uv(face_uv_to_xyz(f, u, v));
    cell_id_from_face_ij(f, st_to_ij(0.5 * (u + 1.0)), st_to_ij(0.5 * (v + 1.0)))
}

fn cell_id_from_face_ij_same(f: usize, i: i64, j: i64, same_face: bool) -> CellId {
    if same_face {
        return cell_id_from_face_ij(f, i, j);
    }
    cell_id_from_face_ij_wrap(f, i, j)
}

/// Returns a leaf cell containing point p. Usually there is exactly one such cell, but for
/// points along the edge of a cell, any adjacent cell may be (deterministically) chosen. This
/// is because s2.CellIDs are considered to be closed sets. The returned cell will always
/// contain the given point, i.e.
///
/// ```text
/// CellFromPoint(p).ContainsPoint(p)
/// ```
///
/// is always true.
pub(crate) fn cell_id_from_point(p: Point) -> CellId {
    let (f, u, v) = xyz_to_face_uv(R3Vector::new(p.0.x, p.0.y, p.0.z));
    let i = st_to_ij(uv_to_st(u));
    let j = st_to_ij(uv_to_st(v));
    cell_id_from_face_ij(f, i, j)
}

/// Returns the bounds in (u,v)-space for the cell at the given level containing the leaf cell
/// with the given (i,j)-coordinates.
pub(crate) fn ij_level_to_bound_uv(i: i64, j: i64, level: i64) -> R2Rect {
    let cell_size = size_ij(level);
    let x_lo = i & -cell_size;
    let y_lo = j & -cell_size;

    R2Rect::new(
        R1Interval::new(
            st_to_uv(ij_to_st_min(x_lo)),
            st_to_uv(ij_to_st_min(x_lo + cell_size)),
        ),
        R1Interval::new(
            st_to_uv(ij_to_st_min(y_lo)),
            st_to_uv(ij_to_st_min(y_lo + cell_size)),
        ),
    )
}
