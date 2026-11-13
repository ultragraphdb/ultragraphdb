/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod geometry_geojson;
mod geometry_wkb;

use crate::errors::geo_error::GeoError;
use crate::types::position::Position;
use crate::types::shape::Shape;

/// A geometry in CRS84, checked as it is built: a line holds two positions or more, a polygon's rings are closed with
/// four positions or more, and its positions all state an elevation or none does. A collection's members are each
/// whole in themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    shape: Shape,
}

impl Geometry {
    /// How deep collections nest in well-known binary: a collection of collections to this depth is read, and a
    /// deeper one refused.
    pub const MAX_NESTING: usize = 16;

    /// The geometry holding `shape`.
    ///
    /// # Errors
    ///
    /// [`GeoError::shape`] for a line of fewer than two positions, a ring that is open or holds fewer than four, and
    /// positions some with an elevation and some without.
    pub fn new(shape: Shape) -> Result<Self, GeoError> {
        match &shape {
            Shape::Point(_) | Shape::GeometryCollection(_) => {}
            Shape::LineString(line) => check_line(line)?,
            Shape::Polygon(rings) => check_rings(rings)?,
            Shape::MultiPoint(points) => check_dimension(points.iter())?,
            Shape::MultiLineString(lines) => {
                lines.iter().try_for_each(|line| check_line(line))?;
                check_dimension(lines.iter().flatten())?;
            }
            Shape::MultiPolygon(polygons) => {
                polygons.iter().try_for_each(|rings| check_rings(rings))?;
                check_dimension(polygons.iter().flatten().flatten())?;
            }
        }
        Ok(Self { shape })
    }

    /// The point at `position`.
    #[must_use]
    pub fn point(position: Position) -> Self {
        Self {
            shape: Shape::Point(position),
        }
    }

    /// The geometry collection holding nothing.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            shape: Shape::GeometryCollection(Vec::new()),
        }
    }

    /// What the geometry holds.
    #[must_use]
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// The geometry's GeoJSON type name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.shape.name()
    }
}

/// A line holds two positions or more, all of one dimension.
fn check_line(line: &[Position]) -> Result<(), GeoError> {
    if line.len() < 2 {
        return Err(GeoError::shape(format!(
            "a line of {} positions, where two or more are needed",
            line.len()
        )));
    }
    check_dimension(line.iter())
}

/// Each ring holds four positions or more and ends where it starts, and all are of one dimension.
fn check_rings(rings: &[Vec<Position>]) -> Result<(), GeoError> {
    for ring in rings {
        if ring.len() < 4 {
            return Err(GeoError::shape(format!(
                "a ring of {} positions, where four or more are needed",
                ring.len()
            )));
        }
        if ring.first() != ring.last() {
            return Err(GeoError::shape("a ring that does not end where it starts"));
        }
    }
    check_dimension(rings.iter().flatten())
}

/// Every position states an elevation, or none does.
fn check_dimension<'a>(mut positions: impl Iterator<Item = &'a Position>) -> Result<(), GeoError> {
    let Some(first) = positions.next() else {
        return Ok(());
    };
    let elevated = first.elevation().is_some();
    if positions.all(|p| p.elevation().is_some() == elevated) {
        Ok(())
    } else {
        Err(GeoError::shape(
            "positions with an elevation beside positions without one",
        ))
    }
}
