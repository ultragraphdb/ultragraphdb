/*
 * Copyright (c) 2013, Tom Payne. All rights reserved.
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Ported from go-geom v1.6.1, geom.go, flat.go, point.go, linestring.go, polygon.go, multipoint.go,
 * multilinestring.go, multipolygon.go, geometrycollection.go. Changed for ultragraphdb: translated to Rust; one
 * type over go-geom's flat representations, with the operations Dgraph uses.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod geometry_clone;
mod geometry_drop;
pub(crate) mod geometry_geojson;
mod geometry_wkb;

use crate::errors::types_error::TypesError;
use crate::types::geometry_kind::GeometryKind;
use crate::types::layout::Layout;

/// A go-geom geometry: its kind and layout, and go-geom's flat representation (all coordinates in one vector,
/// `ends` closing each ring or line, `endss` each polygon of a multipolygon), or the members of a collection.
///
/// Collections nest without a limit in WKB, so a geometry is cloned, dropped, encoded and walked from a stack
/// rather than by recursion; `PartialEq` and `Debug` recurse into the members.
#[derive(Debug, PartialEq)]
pub struct Geometry {
    kind: GeometryKind,
    layout: Layout,
    flat_coords: Vec<f64>,
    ends: Vec<usize>,
    endss: Vec<Vec<usize>>,
    geoms: Vec<Geometry>,
}

fn stride_error(got: usize, want: usize) -> TypesError {
    TypesError::geo(format!("geom: stride mismatch, got {got}, want {want}"))
}

fn layout_error(got: Layout, want: Layout) -> TypesError {
    TypesError::geo(format!(
        "geom: layout mismatch, got {}, want {}",
        got.name(),
        want.name()
    ))
}

/// go-geom's `deflate0`.
fn deflate0(flat: &mut Vec<f64>, c: &[f64], stride: usize) -> Result<(), TypesError> {
    if c.len() != stride {
        return Err(stride_error(c.len(), stride));
    }
    flat.extend_from_slice(c);
    Ok(())
}

/// go-geom's `deflate1`.
fn deflate1(flat: &mut Vec<f64>, coords: &[Vec<f64>], stride: usize) -> Result<(), TypesError> {
    coords.iter().try_for_each(|c| deflate0(flat, c, stride))
}

/// go-geom's `deflate2`.
fn deflate2(
    flat: &mut Vec<f64>,
    ends: &mut Vec<usize>,
    coords: &[Vec<Vec<f64>>],
    stride: usize,
) -> Result<(), TypesError> {
    for c in coords {
        deflate1(flat, c, stride)?;
        ends.push(flat.len());
    }
    Ok(())
}

impl Geometry {
    fn flat(
        kind: GeometryKind,
        layout: Layout,
        flat_coords: Vec<f64>,
        ends: Vec<usize>,
        endss: Vec<Vec<usize>>,
    ) -> Self {
        Self {
            kind,
            layout,
            flat_coords,
            ends,
            endss,
            geoms: Vec::new(),
        }
    }

    /// go-geom's `NewPoint(layout).SetCoords(coords)`.
    pub fn point(layout: Layout, coords: &[f64]) -> Result<Self, TypesError> {
        let mut flat = Vec::new();
        deflate0(&mut flat, coords, layout.stride())?;
        Ok(Self::flat(
            GeometryKind::Point,
            layout,
            flat,
            Vec::new(),
            Vec::new(),
        ))
    }

    /// go-geom's `NewPointEmpty(layout)`.
    pub fn point_empty(layout: Layout) -> Self {
        Self::flat(
            GeometryKind::Point,
            layout,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    /// go-geom's `NewLineString(layout).SetCoords(coords)`.
    pub fn line_string(layout: Layout, coords: &[Vec<f64>]) -> Result<Self, TypesError> {
        let mut flat = Vec::new();
        deflate1(&mut flat, coords, layout.stride())?;
        Ok(Self::flat(
            GeometryKind::LineString,
            layout,
            flat,
            Vec::new(),
            Vec::new(),
        ))
    }

    /// go-geom's `NewPolygon(layout).SetCoords(rings)`.
    pub fn polygon(layout: Layout, rings: &[Vec<Vec<f64>>]) -> Result<Self, TypesError> {
        let (mut flat, mut ends) = (Vec::new(), Vec::new());
        deflate2(&mut flat, &mut ends, rings, layout.stride())?;
        Ok(Self::flat(
            GeometryKind::Polygon,
            layout,
            flat,
            ends,
            Vec::new(),
        ))
    }

    /// go-geom's `NewMultiPoint(layout).SetCoords(points)`; `None` is an empty point.
    pub fn multi_point(layout: Layout, points: &[Option<Vec<f64>>]) -> Result<Self, TypesError> {
        let (mut flat, mut ends) = (Vec::new(), Vec::new());
        for p in points {
            if let Some(c) = p {
                deflate0(&mut flat, c, layout.stride())?;
            }
            ends.push(flat.len());
        }
        Ok(Self::flat(
            GeometryKind::MultiPoint,
            layout,
            flat,
            ends,
            Vec::new(),
        ))
    }

    /// go-geom's `NewMultiLineString(layout).SetCoords(lines)`.
    pub fn multi_line_string(layout: Layout, lines: &[Vec<Vec<f64>>]) -> Result<Self, TypesError> {
        let (mut flat, mut ends) = (Vec::new(), Vec::new());
        deflate2(&mut flat, &mut ends, lines, layout.stride())?;
        Ok(Self::flat(
            GeometryKind::MultiLineString,
            layout,
            flat,
            ends,
            Vec::new(),
        ))
    }

    /// go-geom's `NewMultiPolygon(layout).SetCoords(polygons)`.
    pub fn multi_polygon(
        layout: Layout,
        polygons: &[Vec<Vec<Vec<f64>>>],
    ) -> Result<Self, TypesError> {
        let (mut flat, mut endss) = (Vec::new(), Vec::new());
        for p in polygons {
            let mut ends = Vec::new();
            deflate2(&mut flat, &mut ends, p, layout.stride())?;
            endss.push(ends);
        }
        Ok(Self::flat(
            GeometryKind::MultiPolygon,
            layout,
            flat,
            Vec::new(),
            endss,
        ))
    }

    /// go-geom's `NewGeometryCollection().Push(geoms...)`: a collection without a layout of its own.
    pub fn collection(geoms: Vec<Geometry>) -> Self {
        Self {
            kind: GeometryKind::GeometryCollection,
            layout: Layout::NO_LAYOUT,
            flat_coords: Vec::new(),
            ends: Vec::new(),
            endss: Vec::new(),
            geoms,
        }
    }

    pub(crate) fn empty_collection(layout: Layout) -> Self {
        let mut g = Self::collection(Vec::new());
        g.layout = layout;
        g
    }

    pub(crate) fn from_flat(
        kind: GeometryKind,
        layout: Layout,
        flat_coords: Vec<f64>,
        ends: Vec<usize>,
    ) -> Self {
        Self::flat(kind, layout, flat_coords, ends, Vec::new())
    }

    /// The empty multi-geometry of a kind, for `Push`.
    pub(crate) fn empty_multi(kind: GeometryKind, layout: Layout) -> Self {
        Self::flat(kind, layout, Vec::new(), Vec::new(), Vec::new())
    }

    /// go-geom's `Push` of a member into a multi-geometry or collection, with its layout check.
    pub(crate) fn push(&mut self, g: Geometry) -> Result<(), TypesError> {
        match self.kind {
            GeometryKind::GeometryCollection => {
                if self.layout != Layout::NO_LAYOUT && g.layout() != self.layout {
                    return Err(layout_error(g.layout(), self.layout));
                }
                self.geoms.push(g);
            }
            GeometryKind::MultiPolygon => {
                if g.layout != self.layout {
                    return Err(layout_error(g.layout, self.layout));
                }
                let offset = self.flat_coords.len();
                self.endss.push(g.ends.iter().map(|e| e + offset).collect());
                self.flat_coords.extend_from_slice(&g.flat_coords);
            }
            _ => {
                if g.layout != self.layout {
                    return Err(layout_error(g.layout, self.layout));
                }
                self.flat_coords.extend_from_slice(&g.flat_coords);
                self.ends.push(self.flat_coords.len());
            }
        }
        Ok(())
    }

    /// The geometry type.
    pub fn kind(&self) -> GeometryKind {
        self.kind
    }

    /// go-geom's `Layout`; a collection without one takes the widest of its members'. go-geom recurses into the
    /// members; widest is a join (XYZ and XYM join to XYZM), so the members of nested collections without a layout
    /// are joined from a stack, in any order, to the same result.
    pub fn layout(&self) -> Layout {
        if self.kind != GeometryKind::GeometryCollection || self.layout != Layout::NO_LAYOUT {
            return self.layout;
        }
        let mut max = Layout::NO_LAYOUT;
        let mut stack: Vec<&Geometry> = self.geoms.iter().collect();
        while let Some(g) = stack.pop() {
            if g.kind == GeometryKind::GeometryCollection && g.layout == Layout::NO_LAYOUT {
                stack.extend(&g.geoms);
                continue;
            }
            max = max.widest(g.layout);
        }
        max
    }

    /// The number of coordinates per point.
    pub fn stride(&self) -> usize {
        self.layout().stride()
    }

    /// All coordinates, go-geom's `FlatCoords`.
    pub fn flat_coords(&self) -> &[f64] {
        &self.flat_coords
    }

    /// The end of each ring, line or point, go-geom's `Ends`.
    pub fn ends(&self) -> &[usize] {
        &self.ends
    }

    /// The ends of each polygon of a multipolygon, go-geom's `Endss`.
    pub fn endss(&self) -> &[Vec<usize>] {
        &self.endss
    }

    /// The members of a collection.
    pub fn geoms(&self) -> &[Geometry] {
        &self.geoms
    }

    /// go-geom's `Empty`.
    pub fn is_empty(&self) -> bool {
        // A collection is empty when every geometry in it is, nested collections walked from a stack.
        let mut stack = vec![self];
        while let Some(g) = stack.pop() {
            match g.kind {
                GeometryKind::GeometryCollection => stack.extend(&g.geoms),
                _ if !g.flat_coords.is_empty() => return false,
                _ => {}
            }
        }
        true
    }

    /// go-geom's `NumPolygons` of a multipolygon.
    pub fn num_polygons(&self) -> usize {
        self.endss.len()
    }

    /// go-geom's `MultiPolygon.Polygon(i)`; the empty polygon past the last one, where go-geom indexes past them and
    /// panics.
    pub fn polygon_at(&self, i: usize) -> Geometry {
        let ends = self.endss.get(i).map_or(&[][..], Vec::as_slice);
        if ends.is_empty() {
            return Self::flat(
                GeometryKind::Polygon,
                self.layout,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
        }
        let offset = self
            .endss
            .iter()
            .take(i)
            .rev()
            .find_map(|e| e.last().copied())
            .unwrap_or(0);
        let end = ends.last().copied().unwrap_or(offset);
        Self::flat(
            GeometryKind::Polygon,
            self.layout,
            self.flat_coords
                .get(offset..end)
                .unwrap_or_default()
                .to_vec(),
            ends.iter().map(|e| e.saturating_sub(offset)).collect(),
            Vec::new(),
        )
    }

    /// go-geom's `NumLinearRings` of a polygon.
    pub fn num_linear_rings(&self) -> usize {
        self.ends.len()
    }

    /// go-geom's `Polygon.LinearRing(i)`: the ring's flat coordinates; none past the last ring, where go-geom
    /// indexes past them and panics.
    pub fn linear_ring(&self, i: usize) -> &[f64] {
        let offset = i
            .checked_sub(1)
            .and_then(|prev| self.ends.get(prev))
            .copied()
            .unwrap_or(0);
        self.ends
            .get(i)
            .and_then(|&end| self.flat_coords.get(offset..end))
            .unwrap_or_default()
    }

    /// go-geom's `Coords` of a point: its coordinates.
    pub fn coords(&self) -> &[f64] {
        &self.flat_coords
    }

    /// The point coordinates of each member of a multipoint, `None` for an empty point.
    pub(crate) fn multi_point_coords(&self) -> Vec<Option<&[f64]>> {
        let stride = self.layout.stride();
        let mut offset = 0;
        let mut prev = 0;
        self.ends
            .iter()
            .map(|&end| {
                let c = (end != prev).then(|| {
                    let c = &self.flat_coords[offset..offset + stride];
                    offset += stride;
                    c
                });
                prev = end;
                c
            })
            .collect()
    }
}
