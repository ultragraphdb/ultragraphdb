/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/rect.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::cap::Cap;
use crate::s2::cellid::CellId;
use crate::s2::gomath;
use crate::s2::latlng::LatLng;
use crate::s2::point::Point;
use crate::s2::r1::R1Interval;
use crate::s2::r3::R3Vector;
use crate::s2::s1::{Angle, S1Interval};
use core::f64::consts::{FRAC_PI_2, PI};

const VALID_RECT_LAT_RANGE: R1Interval = R1Interval::new(-FRAC_PI_2, FRAC_PI_2);
const VALID_RECT_LNG_RANGE: S1Interval = S1Interval::full();

/// A closed latitude-longitude rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub(crate) lat: R1Interval,
    pub(crate) lng: S1Interval,
}

impl Rect {
    /// Returns the rectangle with the given latitude and longitude intervals, in radians.
    pub const fn new(lat: R1Interval, lng: S1Interval) -> Rect {
        Rect { lat, lng }
    }

    /// Returns the empty rectangle.
    pub const fn empty() -> Rect {
        Rect {
            lat: R1Interval::empty(),
            lng: S1Interval::empty(),
        }
    }

    /// Returns the full rectangle.
    pub const fn full() -> Rect {
        Rect {
            lat: VALID_RECT_LAT_RANGE,
            lng: VALID_RECT_LNG_RANGE,
        }
    }

    /// Constructs a rectangle containing a single point p.
    pub fn from_lat_lng(p: LatLng) -> Rect {
        Rect {
            lat: R1Interval::new(p.lat.radians(), p.lat.radians()),
            lng: S1Interval::new(p.lng.radians(), p.lng.radians()),
        }
    }

    /// The latitude interval, in radians.
    pub fn lat(&self) -> R1Interval {
        self.lat
    }

    /// The longitude interval, in radians.
    pub fn lng(&self) -> S1Interval {
        self.lng
    }

    /// Returns true iff the rectangle is valid. This requires Lat ⊆ [-π/2,π/2] and Lng ⊆
    /// [-π,π], and Lat = ∅ iff Lng = ∅
    pub fn is_valid(&self) -> bool {
        self.lat.lo.abs() <= FRAC_PI_2
            && self.lat.hi.abs() <= FRAC_PI_2
            && self.lng.is_valid()
            && self.lat.is_empty() == self.lng.is_empty()
    }

    /// Reports whether the rectangle is empty.
    pub fn is_empty(&self) -> bool {
        self.lat.is_empty()
    }

    /// Reports whether the rectangle is full.
    pub fn is_full(&self) -> bool {
        self.lat.equal(VALID_RECT_LAT_RANGE) && self.lng.is_full()
    }

    /// Returns one corner of the rectangle.
    pub fn lo(&self) -> LatLng {
        LatLng::new(Angle(self.lat.lo), Angle(self.lng.lo))
    }

    /// Returns the other corner of the rectangle.
    pub fn hi(&self) -> LatLng {
        LatLng::new(Angle(self.lat.hi), Angle(self.lng.hi))
    }

    /// Returns the center of the rectangle.
    pub fn center(&self) -> LatLng {
        LatLng::new(Angle(self.lat.center()), Angle(self.lng.center()))
    }

    /// Increases the size of the rectangle to include the given point.
    pub fn add_point(&self, ll: LatLng) -> Rect {
        if !ll.is_valid() {
            return *self;
        }
        Rect {
            lat: self.lat.add_point(ll.lat.radians()),
            lng: self.lng.add_point(ll.lng.radians()),
        }
    }

    /// Returns a rectangle that has been expanded by margin.Lat on each side in the latitude
    /// direction, and by margin.Lng on each side in the longitude direction. If either margin
    /// is negative, then it shrinks the rectangle on the corresponding sides instead. The
    /// resulting rectangle may be empty.
    ///
    /// The latitude-longitude space has the topology of a cylinder. Longitudes "wrap around"
    /// at +/-180 degrees, while latitudes are clamped to range [-90, 90]. This means that any
    /// expansion (positive or negative) of the full longitude range remains full (since the
    /// "rectangle" is actually a continuous band around the cylinder), while expansion of the
    /// full latitude range remains full only if the margin is positive.
    ///
    /// If either the latitude or longitude interval becomes empty after expansion by a
    /// negative margin, the result is empty.
    ///
    /// Note that if an expanded rectangle contains a pole, it may not contain all possible
    /// lat/lng representations of that pole, e.g., both points [π/2,0] and [π/2,1] represent
    /// the same pole, but they might not be contained by the same Rect.
    ///
    /// If you are trying to grow a rectangle by a certain distance on the sphere (e.g. 5km),
    /// refer to the ExpandedByDistance() C++ method implementation instead.
    pub(crate) fn expanded(&self, margin: LatLng) -> Rect {
        let lat = self.lat.expanded(margin.lat.radians());
        let lng = self.lng.expanded(margin.lng.radians());

        if lat.is_empty() || lng.is_empty() {
            return Rect::empty();
        }

        Rect {
            lat: lat.intersection(VALID_RECT_LAT_RANGE),
            lng,
        }
    }

    /// Returns the rectangle unmodified if it does not include either pole. If it includes
    /// either pole, PolarClosure returns an expansion of the rectangle along the longitudinal
    /// range to include all possible representations of the contained poles.
    pub fn polar_closure(&self) -> Rect {
        if self.lat.lo == -FRAC_PI_2 || self.lat.hi == FRAC_PI_2 {
            return Rect {
                lat: self.lat,
                lng: S1Interval::full(),
            };
        }
        *self
    }

    /// Returns the smallest Rect containing the union of this rectangle and the given
    /// rectangle.
    pub fn union(&self, other: Rect) -> Rect {
        Rect {
            lat: self.lat.union(other.lat),
            lng: self.lng.union(other.lng),
        }
    }

    /// Returns the smallest rectangle containing the intersection of this rectangle and the
    /// given rectangle. Note that the region of intersection may consist of two disjoint
    /// rectangles, in which case a single rectangle spanning both of them is returned.
    pub fn intersection(&self, other: Rect) -> Rect {
        let lat = self.lat.intersection(other.lat);
        let lng = self.lng.intersection(other.lng);

        if lat.is_empty() || lng.is_empty() {
            return Rect::empty();
        }
        Rect { lat, lng }
    }

    /// Reports whether this rectangle and the other have any points in common.
    pub fn intersects(&self, other: Rect) -> bool {
        self.lat.intersects(other.lat) && self.lng.intersects(other.lng)
    }

    /// Returns a cap that contains Rect.
    pub fn cap_bound(&self) -> Cap {
        // We consider two possible bounding caps, one whose axis passes
        // through the center of the lat-long rectangle and one whose axis
        // is the north or south pole.  We return the smaller of the two caps.

        if self.is_empty() {
            return Cap::empty();
        }

        let (pole_z, pole_angle) = if self.lat.hi + self.lat.lo < 0.0 {
            // South pole axis yields smaller cap.
            (-1.0, FRAC_PI_2 + self.lat.hi)
        } else {
            (1.0, FRAC_PI_2 - self.lat.lo)
        };
        let pole_cap =
            Cap::from_center_angle(Point(R3Vector::new(0.0, 0.0, pole_z)), Angle(pole_angle));

        // For bounding rectangles that span 180 degrees or less in longitude, the
        // maximum cap size is achieved at one of the rectangle vertices.  For
        // rectangles that are larger than 180 degrees, we punt and always return a
        // bounding cap centered at one of the two poles.
        if gomath::remainder(self.lng.hi - self.lng.lo, 2.0 * PI) >= 0.0
            && self.lng.hi - self.lng.lo <= PI
        {
            let mid_cap = Cap::from_point(Point::from_lat_lng(self.center()))
                .add_point(Point::from_lat_lng(self.lo()))
                .add_point(Point::from_lat_lng(self.hi()));
            if mid_cap.height() < pole_cap.height() {
                return mid_cap;
            }
        }
        pole_cap
    }

    /// Returns itself.
    pub fn rect_bound(&self) -> Rect {
        *self
    }

    /// Reports whether this Rect contains the other Rect.
    pub fn contains(&self, other: Rect) -> bool {
        self.lat.contains_interval(other.lat) && self.lng.contains_interval(other.lng)
    }

    /// Returns true if the given LatLng is contained by this Rect.
    pub fn contains_lat_lng(&self, ll: LatLng) -> bool {
        if !ll.is_valid() {
            return false;
        }
        self.lat.contains(ll.lat.radians()) && self.lng.contains(ll.lng.radians())
    }

    /// Reports whether the given Point is within this Rect.
    pub fn contains_point(&self, p: Point) -> bool {
        self.contains_lat_lng(LatLng::from_point(p))
    }

    /// Computes a covering of the Rect.
    pub fn cell_union_bound(&self) -> Vec<CellId> {
        self.cap_bound().cell_union_bound()
    }
}
