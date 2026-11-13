/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::geo_error::GeoError;
use crate::types::geometry::Geometry;
use crate::types::position::Position;
use crate::types::shape::Shape;

/// What ISO well-known binary adds to a type's code for positions with an elevation.
const WITH_ELEVATION: u32 = 1_000;

impl Geometry {
    /// The geometry the well-known binary `bytes` hold (ISO 13249-3): for each geometry a byte order, a type code,
    /// 1 to 7 and 1001 to 1007 with an elevation, and its coordinates as doubles. Both byte orders are read. Each
    /// count is checked against the bytes that remain before anything is allocated for it, and collections nest to
    /// [`Geometry::MAX_NESTING`].
    ///
    /// # Errors
    ///
    /// [`GeoError::wkb`] for bytes that end early, run on past the geometry, name an unknown byte order or type, hold
    /// a count the bytes cannot fill, or nest too deep; [`GeoError::coordinate`] and [`GeoError::shape`] as
    /// [`Position::new`] and [`Geometry::new`] refuse.
    pub fn from_wkb(bytes: &[u8]) -> Result<Self, GeoError> {
        let mut reader = Reader { rest: bytes };
        let geometry = reader.geometry(0)?;
        if reader.rest.is_empty() {
            Ok(geometry)
        } else {
            Err(GeoError::wkb(format!(
                "{} bytes after the geometry",
                reader.rest.len()
            )))
        }
    }

    /// The geometry as little-endian well-known binary, which [`Geometry::from_wkb`] reads back to it.
    #[must_use]
    pub fn to_wkb(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_geometry(&mut out, self);
        out
    }
}

/// The bytes of a geometry still to read.
struct Reader<'a> {
    rest: &'a [u8],
}

impl Reader<'_> {
    /// The next `N` bytes.
    fn take<const N: usize>(&mut self) -> Result<[u8; N], GeoError> {
        let (head, rest) = self
            .rest
            .split_first_chunk::<N>()
            .ok_or_else(|| GeoError::wkb("the bytes end inside a geometry"))?;
        self.rest = rest;
        Ok(*head)
    }

    /// A geometry's byte order: whether it is little-endian.
    fn order(&mut self) -> Result<bool, GeoError> {
        match self.take::<1>()? {
            [0] => Ok(false),
            [1] => Ok(true),
            [other] => Err(GeoError::wkb(format!("unknown byte order: {other}"))),
        }
    }

    fn u32(&mut self, little: bool) -> Result<u32, GeoError> {
        let bytes = self.take::<4>()?;
        Ok(if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    fn f64(&mut self, little: bool) -> Result<f64, GeoError> {
        let bytes = self.take::<8>()?;
        Ok(if little {
            f64::from_le_bytes(bytes)
        } else {
            f64::from_be_bytes(bytes)
        })
    }

    /// A count of items of at least `least` bytes each, refused when the bytes that remain cannot hold them.
    fn count(&mut self, little: bool, least: usize) -> Result<usize, GeoError> {
        let stated = self.u32(little)?;
        usize::try_from(stated)
            .ok()
            .filter(|count| {
                count
                    .checked_mul(least)
                    .is_some_and(|needed| needed <= self.rest.len())
            })
            .ok_or_else(|| GeoError::wkb(format!("a count of {stated} the bytes cannot hold")))
    }

    fn position(&mut self, little: bool, elevated: bool) -> Result<Position, GeoError> {
        let position = Position::new(self.f64(little)?, self.f64(little)?)?;
        if elevated {
            position.with_elevation(self.f64(little)?)
        } else {
            Ok(position)
        }
    }

    fn positions(&mut self, little: bool, elevated: bool) -> Result<Vec<Position>, GeoError> {
        let width = if elevated { 24 } else { 16 };
        let count = self.count(little, width)?;
        (0..count)
            .map(|_| self.position(little, elevated))
            .collect()
    }

    fn rings(&mut self, little: bool, elevated: bool) -> Result<Vec<Vec<Position>>, GeoError> {
        // A ring is at least its count.
        let count = self.count(little, 4)?;
        (0..count)
            .map(|_| self.positions(little, elevated))
            .collect()
    }

    /// `count` member geometries, each with a header of its own, read by `member` from the shape it holds.
    fn members<T>(
        &mut self,
        little: bool,
        depth: usize,
        member: impl Fn(Geometry) -> Result<T, GeoError>,
    ) -> Result<Vec<T>, GeoError> {
        // A member is at least its byte order and its type.
        let count = self.count(little, 5)?;
        (0..count)
            .map(|_| self.geometry(depth.saturating_add(1)).and_then(&member))
            .collect()
    }

    /// One geometry with its header, at `depth` collections down.
    fn geometry(&mut self, depth: usize) -> Result<Geometry, GeoError> {
        if depth > Geometry::MAX_NESTING {
            return Err(GeoError::wkb(format!(
                "geometries nested deeper than {}",
                Geometry::MAX_NESTING
            )));
        }
        let little = self.order()?;
        let code = self.u32(little)?;
        let elevated = code > WITH_ELEVATION;
        let kind = if elevated {
            code - WITH_ELEVATION
        } else {
            code
        };
        let unexpected = |g: &Geometry| {
            GeoError::wkb(format!(
                "a {} where a member of another type is needed",
                g.name()
            ))
        };
        let shape = match kind {
            1 => Shape::Point(self.position(little, elevated)?),
            2 => Shape::LineString(self.positions(little, elevated)?),
            3 => Shape::Polygon(self.rings(little, elevated)?),
            4 => Shape::MultiPoint(self.members(little, depth, |g| match g.shape {
                Shape::Point(p) => Ok(p),
                Shape::LineString(_)
                | Shape::Polygon(_)
                | Shape::MultiPoint(_)
                | Shape::MultiLineString(_)
                | Shape::MultiPolygon(_)
                | Shape::GeometryCollection(_) => Err(unexpected(&g)),
            })?),
            5 => Shape::MultiLineString(self.members(little, depth, |g| match g.shape {
                Shape::LineString(line) => Ok(line),
                Shape::Point(_)
                | Shape::Polygon(_)
                | Shape::MultiPoint(_)
                | Shape::MultiLineString(_)
                | Shape::MultiPolygon(_)
                | Shape::GeometryCollection(_) => Err(unexpected(&g)),
            })?),
            6 => Shape::MultiPolygon(self.members(little, depth, |g| match g.shape {
                Shape::Polygon(rings) => Ok(rings),
                Shape::Point(_)
                | Shape::LineString(_)
                | Shape::MultiPoint(_)
                | Shape::MultiLineString(_)
                | Shape::MultiPolygon(_)
                | Shape::GeometryCollection(_) => Err(unexpected(&g)),
            })?),
            7 => Shape::GeometryCollection(self.members(little, depth, Ok)?),
            _ => return Err(GeoError::wkb(format!("unknown geometry type: {code}"))),
        };
        Geometry::new(shape)
    }
}

/// Whether a shape's positions state an elevation: its first does, as all then do.
fn elevated(shape: &Shape) -> bool {
    let first = match shape {
        Shape::Point(p) => Some(p),
        Shape::LineString(line) | Shape::MultiPoint(line) => line.first(),
        Shape::Polygon(rings) | Shape::MultiLineString(rings) => rings.iter().flatten().next(),
        Shape::MultiPolygon(polygons) => polygons.iter().flatten().flatten().next(),
        Shape::GeometryCollection(_) => None,
    };
    first.is_some_and(|p| p.elevation().is_some())
}

/// Writes a header: little-endian, and the type's code with its elevation mark.
fn write_header(out: &mut Vec<u8>, kind: u32, elevated: bool) {
    out.push(1);
    let code = if elevated {
        kind + WITH_ELEVATION
    } else {
        kind
    };
    out.extend_from_slice(&code.to_le_bytes());
}

/// Writes a count. A geometry read by this crate holds no more than `u32::MAX` of anything; one built past that
/// writes the most a count states, which no reader then accepts.
fn write_count(out: &mut Vec<u8>, count: usize) {
    out.extend_from_slice(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes());
}

fn write_position(out: &mut Vec<u8>, p: &Position) {
    out.extend_from_slice(&p.lng().to_le_bytes());
    out.extend_from_slice(&p.lat().to_le_bytes());
    if let Some(elevation) = p.elevation() {
        out.extend_from_slice(&elevation.to_le_bytes());
    }
}

fn write_positions(out: &mut Vec<u8>, positions: &[Position]) {
    write_count(out, positions.len());
    positions.iter().for_each(|p| write_position(out, p));
}

fn write_rings(out: &mut Vec<u8>, rings: &[Vec<Position>]) {
    write_count(out, rings.len());
    rings.iter().for_each(|ring| write_positions(out, ring));
}

/// Writes `geometry` with its header, and each member of a multi-geometry or collection with one of its own.
fn write_geometry(out: &mut Vec<u8>, geometry: &Geometry) {
    let shape = geometry.shape();
    let z = elevated(shape);
    match shape {
        Shape::Point(p) => {
            write_header(out, 1, z);
            write_position(out, p);
        }
        Shape::LineString(line) => {
            write_header(out, 2, z);
            write_positions(out, line);
        }
        Shape::Polygon(rings) => {
            write_header(out, 3, z);
            write_rings(out, rings);
        }
        Shape::MultiPoint(points) => {
            write_header(out, 4, z);
            write_count(out, points.len());
            for p in points {
                write_header(out, 1, z);
                write_position(out, p);
            }
        }
        Shape::MultiLineString(lines) => {
            write_header(out, 5, z);
            write_count(out, lines.len());
            for line in lines {
                write_header(out, 2, z);
                write_positions(out, line);
            }
        }
        Shape::MultiPolygon(polygons) => {
            write_header(out, 6, z);
            write_count(out, polygons.len());
            for rings in polygons {
                write_header(out, 3, z);
                write_rings(out, rings);
            }
        }
        Shape::GeometryCollection(members) => {
            write_header(out, 7, false);
            write_count(out, members.len());
            members
                .iter()
                .for_each(|member| write_geometry(out, member));
        }
    }
}
