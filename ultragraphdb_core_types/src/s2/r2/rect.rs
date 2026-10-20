/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, r2/rect.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::point::R2Point;
use crate::s2::r1::R1Interval;

/// A closed axis-aligned rectangle in the (x,y) plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct R2Rect {
    pub(crate) x: R1Interval,
    pub(crate) y: R1Interval,
}

impl Default for R2Rect {
    /// Go's zero `r2.Rect{}`, the degenerate rectangle at the origin (not the empty one).
    fn default() -> Self {
        R2Rect {
            x: R1Interval::new(0.0, 0.0),
            y: R1Interval::new(0.0, 0.0),
        }
    }
}

impl R2Rect {
    /// Returns the rectangle with the given x- and y-intervals.
    pub const fn new(x: R1Interval, y: R1Interval) -> Self {
        R2Rect { x, y }
    }

    /// The x-interval.
    pub fn x(&self) -> R1Interval {
        self.x
    }

    /// The y-interval.
    pub fn y(&self) -> R1Interval {
        self.y
    }

    /// Constructs a rect that contains the given points. Like Go, no points give the zero rect.
    pub fn from_points(pts: &[R2Point]) -> R2Rect {
        // Because the default value on interval is 0,0, we need to manually
        // define the interval from the first point passed in as our starting
        // interval, otherwise we end up with the case of passing in
        // Point{0.2, 0.3} and getting the starting Rect of {0, 0.2}, {0, 0.3}
        // instead of the Rect {0.2, 0.2}, {0.3, 0.3} which is not correct.
        let Some(first) = pts.first() else {
            return R2Rect::default();
        };
        let mut r = R2Rect {
            x: R1Interval::from_point(first.x),
            y: R1Interval::from_point(first.y),
        };
        for p in &pts[1..] {
            r = r.add_point(*p);
        }
        r
    }

    /// Constructs the canonical empty rectangle.
    pub const fn empty() -> R2Rect {
        R2Rect {
            x: R1Interval::empty(),
            y: R1Interval::empty(),
        }
    }

    /// Reports whether the rectangle is empty.
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    /// Returns all four vertices of the rectangle, CCW starting with the lower left corner.
    pub fn vertices(&self) -> [R2Point; 4] {
        [
            R2Point::new(self.x.lo, self.y.lo),
            R2Point::new(self.x.hi, self.y.lo),
            R2Point::new(self.x.hi, self.y.hi),
            R2Point::new(self.x.lo, self.y.hi),
        ]
    }

    /// Returns the vertex in direction i along the X-axis (0=left, 1=right) and
    /// direction j along the Y-axis (0=down, 1=up).
    pub fn vertex_ij(&self, i: usize, j: usize) -> R2Point {
        let x = if i == 1 { self.x.hi } else { self.x.lo };
        let y = if j == 1 { self.y.hi } else { self.y.lo };
        R2Point::new(x, y)
    }

    /// Returns the center of the rectangle in (x,y)-space.
    pub fn center(&self) -> R2Point {
        R2Point::new(self.x.center(), self.y.center())
    }

    /// Reports whether the rectangle contains the given point.
    pub fn contains_point(&self, p: R2Point) -> bool {
        self.x.contains(p.x) && self.y.contains(p.y)
    }

    /// Reports whether this rectangle and the other rectangle have any points in common.
    pub fn intersects(&self, other: R2Rect) -> bool {
        self.x.intersects(other.x) && self.y.intersects(other.y)
    }

    /// Expands the rectangle to include the given point.
    pub fn add_point(&self, p: R2Point) -> R2Rect {
        R2Rect {
            x: self.x.add_point(p.x),
            y: self.y.add_point(p.y),
        }
    }

    /// Expands the rectangle to include the given rectangle.
    pub fn add_rect(&self, other: R2Rect) -> R2Rect {
        R2Rect {
            x: self.x.union(other.x),
            y: self.y.union(other.y),
        }
    }

    /// Returns a rectangle that has been expanded in the x-direction by margin.x, and in
    /// y-direction by margin.y. If either margin is empty, then shrink the interval on the
    /// corresponding sides instead. The resulting rectangle may be empty.
    pub fn expanded(&self, margin: R2Point) -> R2Rect {
        let xx = self.x.expanded(margin.x);
        let yy = self.y.expanded(margin.y);
        if xx.is_empty() || yy.is_empty() {
            return R2Rect::empty();
        }
        R2Rect { x: xx, y: yy }
    }

    /// Returns a rect that has been expanded by the amount on all sides.
    pub fn expanded_by_margin(&self, margin: f64) -> R2Rect {
        self.expanded(R2Point::new(margin, margin))
    }

    /// Returns the smallest rectangle containing the union of this rectangle and the other.
    pub fn union(&self, other: R2Rect) -> R2Rect {
        R2Rect {
            x: self.x.union(other.x),
            y: self.y.union(other.y),
        }
    }
}
