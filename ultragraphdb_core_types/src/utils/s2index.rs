/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/s2index.go. Changed for ultragraphdb: translated to Rust over this crate's
 * port of golang/geo's S2.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeSet;

use crate::errors::types_error::TypesError;
use crate::s2::{CellId, LatLng, Loop, Point, RegionCoverer};
use crate::types::geometry::Geometry;
use crate::types::geometry_kind::GeometryKind;
use crate::utils::strconv;

/// Dgraph's `MinCellLevel`: the largest cells the index uses (about 250 km by 380 km).
pub const MIN_CELL_LEVEL: i64 = 5;
/// Dgraph's `MaxCellLevel`: the smallest cells the index uses (about 120 m by 180 m).
pub const MAX_CELL_LEVEL: i64 = 16;
/// Dgraph's `MaxCells`: the most cells a region's cover uses.
pub const MAX_CELLS: i64 = 18;

pub(crate) const PARENT_PREFIX: &str = "p/";
pub(crate) const COVER_PREFIX: &str = "c/";

/// Dgraph's `parentCoverTokens`: the parents with prefix `p/`, then the cover with prefix `c/`.
pub(crate) fn parent_cover_tokens(parents: &[CellId], cover: &[CellId]) -> Vec<String> {
    let mut tokens = create_tokens(parents, PARENT_PREFIX);
    tokens.extend(create_tokens(cover, COVER_PREFIX));
    tokens
}

/// Dgraph's `IndexGeoTokens`: the geo index tokens of a geometry, its parent cells (`p/`) and its cover (`c/`).
///
/// Go collects the parents in a map, so their order is unspecified; this port lists them in cell id order.
pub fn index_geo_tokens(g: &Geometry) -> Result<Vec<String>, TypesError> {
    let (parents, cover) = index_cells(g)?;
    Ok(parent_cover_tokens(&parents, &cover))
}

/// Dgraph's `indexCells`: the parents up to [`MIN_CELL_LEVEL`] of the cover, and the cover, of a 2D point, polygon
/// or multipolygon.
pub fn index_cells(g: &Geometry) -> Result<(Vec<CellId>, Vec<CellId>), TypesError> {
    if g.stride() != 2 {
        return Err(TypesError::geo(
            "Covering only available for 2D co-ordinates.",
        ));
    }
    match g.kind() {
        GeometryKind::Point => Ok(index_cells_for_point(g, MIN_CELL_LEVEL, MAX_CELL_LEVEL)),
        GeometryKind::Polygon => {
            let l = loop_from_polygon(g)?;
            let cover = cover_loop(&l, MIN_CELL_LEVEL, MAX_CELL_LEVEL, MAX_CELLS);
            Ok((get_parent_cells(&cover, MIN_CELL_LEVEL), cover))
        }
        GeometryKind::MultiPolygon => {
            let mut cover = Vec::new();
            for i in 0..g.num_polygons() {
                let l = loop_from_polygon(&g.polygon_at(i))?;
                cover.extend(cover_loop(&l, MIN_CELL_LEVEL, MAX_CELL_LEVEL, MAX_CELLS));
            }
            Ok((get_parent_cells(&cover, MIN_CELL_LEVEL), cover))
        }
        kind => Err(TypesError::geo(format!(
            "Cannot index geometry of type {}",
            kind.go_type()
        ))),
    }
}

/// Dgraph's `pointFromCoord`: GeoJSON's `[lng, lat]` as an S2 point.
pub(crate) fn point_from_coord(c: &[f64]) -> Point {
    Point::from_lat_lng(LatLng::from_degrees(c[1], c[0]))
}

/// Go's `%+v` of a go-geom polygon, which Dgraph prints in an error.
fn polygon_debug(p: &Geometry) -> String {
    let floats: Vec<String> = p
        .flat_coords()
        .iter()
        .map(|f| strconv::format_float(*f, b'g', 64))
        .collect();
    let ends: Vec<String> = p.ends().iter().map(usize::to_string).collect();
    format!(
        "&{{geom2:{{geom1:{{geom0:{{layout:{} stride:{} flatCoords:[{}] srid:0}}}} ends:[{}]}}}}",
        p.layout().value(),
        p.stride(),
        floats.join(" "),
        ends.join(" ")
    )
}

/// Dgraph's `loopFromPolygon`: the outer ring of a polygon as an S2 loop, holes dropped, oriented
/// counter-clockwise by the planar shoelace test and inverted when its cap exceeds a hemisphere.
pub fn loop_from_polygon(p: &Geometry) -> Result<Loop, TypesError> {
    let stride = p.stride();
    let r = p.linear_ring(0);
    let n = r.len().checked_div(stride).unwrap_or(0);
    if n < 4 {
        return Err(TypesError::geo("Can't convert ring with less than 4 pts"));
    }
    if r[0] != r[(n - 1) * stride] || r[1] != r[(n - 1) * stride + 1] {
        return Err(TypesError::geo(format!(
            "Last coordinate not same as first for polygon: {}\n",
            polygon_debug(p)
        )));
    }
    let coord = |i: usize| &r[i * stride..i * stride + 2];
    let reverse = is_clockwise(r, stride);
    let pts = (0..n - 1)
        .map(|i| point_from_coord(coord(if reverse { n - 1 - i } else { i })))
        .collect();
    let mut l = Loop::from_points(pts);
    if l.cap_bound().radius().degrees() > 90.0 {
        l.invert();
    }
    Ok(l)
}

/// Dgraph's `isClockwise`: the planar shoelace test, a fast approximation for rings smaller than a hemisphere.
fn is_clockwise(r: &[f64], stride: usize) -> bool {
    let n = r.len() / stride;
    let mut a = 0.0;
    for i in 0..n {
        let (p1, p2) = (&r[i * stride..], &r[((i + 1) % n) * stride..]);
        a += (p2[0] - p1[0]) * (p1[1] + p2[1]);
    }
    a > 0.0
}

/// Dgraph's `indexCellsForPoint`: the point's cells from `min_level` to `max_level`, and its cell at `max_level`.
fn index_cells_for_point(
    p: &Geometry,
    min_level: i64,
    max_level: i64,
) -> (Vec<CellId>, Vec<CellId>) {
    let c = p.coords();
    let cell = CellId::from_lat_lng(LatLng::from_degrees(c[1], c[0]));
    let cells = (min_level..=max_level).map(|l| cell.parent(l)).collect();
    (cells, vec![cell.parent(max_level)])
}

/// Dgraph's `getParentCells`: every ancestor of the cover's cells down to `min_level`, each once.
pub(crate) fn get_parent_cells(cover: &[CellId], min_level: i64) -> Vec<CellId> {
    let parents: BTreeSet<CellId> = cover
        .iter()
        .flat_map(|c| (min_level..=c.level()).rev().map(move |l| c.parent(l)))
        .collect();
    parents.into_iter().collect()
}

/// Dgraph's `coverLoop`: golang/geo's region covering of a loop.
pub(crate) fn cover_loop(l: &Loop, min_level: i64, max_level: i64, max_cells: i64) -> Vec<CellId> {
    RegionCoverer::new(min_level, max_level, 0, max_cells)
        .covering(l)
        .into_vec()
}

/// Dgraph's `createTokens`: each cell's token after a prefix.
pub(crate) fn create_tokens(cells: &[CellId], prefix: &str) -> Vec<String> {
    cells
        .iter()
        .map(|c| format!("{prefix}{}", c.to_token()))
        .collect()
}
