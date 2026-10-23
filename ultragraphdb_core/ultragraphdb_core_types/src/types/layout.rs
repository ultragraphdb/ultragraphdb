/*
 * Copyright (c) 2013, Tom Payne. All rights reserved.
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Ported from go-geom v1.6.1, geom.go (Layout). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// go-geom's `Layout`: which coordinates each point carries. Values above `XYZM` are go-geom's `Layout(n)`, a
/// layout of `n` coordinates that GeoJSON decoding guesses for five or more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Layout(u32);

impl Layout {
    pub const NO_LAYOUT: Layout = Layout(0);
    pub const XY: Layout = Layout(1);
    pub const XYZ: Layout = Layout(2);
    pub const XYM: Layout = Layout(3);
    pub const XYZM: Layout = Layout(4);

    /// go-geom's `Layout(n)`.
    pub const fn new(n: u32) -> Layout {
        Layout(n)
    }

    /// go-geom's integer value of the layout.
    pub fn value(self) -> u32 {
        self.0
    }

    /// The number of coordinates per point.
    pub fn stride(self) -> usize {
        match self.0 {
            0 => 0,
            1 => 2,
            2 | 3 => 3,
            4 => 4,
            n => n as usize,
        }
    }

    /// The wider of two layouts as go-geom's collection `Layout` chooses it: XYZ and XYM are each other's widening
    /// to XYZM, and otherwise the larger value wins. It is a join, so a collection's layout is the join of its
    /// members' in any order.
    pub(crate) fn widest(self, other: Layout) -> Layout {
        if (self == Layout::XYZ && other == Layout::XYM)
            || (self == Layout::XYM && other == Layout::XYZ)
        {
            Layout::XYZM
        } else {
            self.max(other)
        }
    }

    /// go-geom's `String`.
    pub fn name(self) -> String {
        match self.0 {
            0 => "NoLayout".into(),
            1 => "XY".into(),
            2 => "XYZ".into(),
            3 => "XYM".into(),
            4 => "XYZM".into(),
            n => format!("Layout({n})"),
        }
    }
}
