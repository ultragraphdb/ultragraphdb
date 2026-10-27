/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/geofilter.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::s2::{Loop, Point};
use crate::types::geo_query_data::GeoQueryData;
use crate::types::geometry::Geometry;
use crate::types::geometry_kind::GeometryKind;
use crate::types::query_type::QueryType;
use crate::utils::earth::earth_angle;
use crate::utils::s2index::{
    COVER_PREFIX, MAX_CELL_LEVEL, MAX_CELLS, MIN_CELL_LEVEL, PARENT_PREFIX, cover_loop,
    create_tokens, get_parent_cells, index_cells, loop_from_polygon, parent_cover_tokens,
    point_from_coord,
};

/// Dgraph's `IsGeoFunc`: `near`, `contains`, `within` and `intersects`.
pub fn is_geo_func(name: &str) -> bool {
    matches!(name, "near" | "contains" | "within" | "intersects")
}

/// Dgraph's `queryTokensGeo`: the index tokens for a geo query on a point, polygon or multipolygon;
/// `max_distance` (metres) is a near query's radius.
pub fn query_tokens_geo(
    qt: QueryType,
    g: &Geometry,
    max_distance: f64,
) -> Result<(Vec<String>, GeoQueryData), TypesError> {
    let mut loops: Vec<Loop> = Vec::new();
    let mut pt: Option<Point> = None;
    match g.kind() {
        GeometryKind::Point => {
            let p = point_from_coord(g.coords())?;
            pt = Some(p);
            if qt == QueryType::Near {
                // A distance of NaN or infinity passes Go's check and panics in its exact predicates.
                if max_distance <= 0.0 || !max_distance.is_finite() {
                    return Err(TypesError::geo(
                        "Invalid max distance specified for a near query",
                    ));
                }
                loops.push(Loop::regular(p, earth_angle(max_distance), 100));
            }
        }
        GeometryKind::Polygon => loops.push(loop_from_polygon(g)?),
        GeometryKind::MultiPolygon => {
            for i in 0..g.num_polygons() {
                loops.push(loop_from_polygon(&g.polygon_at(i))?);
            }
        }
        kind => {
            return Err(TypesError::geo(format!(
                "Cannot query using a geometry of type {}",
                kind.go_type()
            )));
        }
    }
    let (parents, cover) = if qt == QueryType::Near {
        if loops.is_empty() {
            return Err(TypesError::geo(
                "Internal error while processing near query.",
            ));
        }
        let cover = cover_loop(&loops[0], MIN_CELL_LEVEL, MAX_CELL_LEVEL, MAX_CELLS);
        (get_parent_cells(&cover, MIN_CELL_LEVEL), cover)
    } else {
        index_cells(g)?
    };
    match qt {
        QueryType::Within => {
            if loops.is_empty() {
                return Err(TypesError::geo("Require a polygon for within query"));
            }
            Ok((
                create_tokens(&cover, PARENT_PREFIX),
                GeoQueryData::new(None, loops, qt),
            ))
        }
        QueryType::Contains => Ok((
            create_tokens(&parents, COVER_PREFIX),
            GeoQueryData::new(pt, loops, qt),
        )),
        QueryType::Near => {
            if pt.is_none() {
                return Err(TypesError::geo("Require a point for a within query."));
            }
            Ok((
                parent_cover_tokens(&parents, &cover),
                GeoQueryData::new(None, loops, QueryType::Intersects),
            ))
        }
        QueryType::Intersects => {
            if loops.is_empty() {
                return Err(TypesError::geo("Require a polygon for intersects query"));
            }
            Ok((
                parent_cover_tokens(&parents, &cover),
                GeoQueryData::new(None, loops, qt),
            ))
        }
    }
}
