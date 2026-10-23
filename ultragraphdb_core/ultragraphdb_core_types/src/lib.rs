/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `types`: scalar type identifiers, conversions, binary encodings, ordering, passwords, geo values.

mod errors;
mod s2;
mod traits;
mod types;
mod utils;

pub use crate::errors::types_error::{TypesError, TypesErrorEnum};
pub use crate::s2::{
    Angle, Cap, Cell, CellId, CellUnion, ChordAngle, Crossing, Direction, EdgeCrosser, FACE_BITS,
    LatLng, Loop, MAX_LEVEL, NUM_FACES, POS_BITS, Point, R1Interval, R2Point, R2Rect, R3Vector,
    Rect, RectBounder, Region, RegionCoverer, S1Interval, angle_contains_vertex,
    chord_angle_between_points, crossing_sign, edge_or_vertex_crossing, expand_for_subregions,
    go_asin, go_atan2, go_cos, go_frexp, go_ilogb, go_ldexp, go_max, go_min, go_nextafter,
    go_remainder, go_sin, ordered_ccw, robust_sign, sign, vertex_crossing,
};
pub use crate::types::area::Area;
pub use crate::types::big_float::BigFloat;
pub use crate::types::date_time::{DateTime, Zone};
pub use crate::types::geo_query_data::GeoQueryData;
pub use crate::types::geometry::Geometry;
pub use crate::types::geometry_kind::GeometryKind;
pub use crate::types::layout::Layout;
pub use crate::types::length::Length;
pub use crate::types::query_type::QueryType;
pub use crate::types::sharded_map::{NUM_SHARDS, ShardedMap};
pub use crate::types::text_collator::TextCollator;
pub use crate::types::type_id::{TYPE_NAMES, TypeId};
pub use crate::types::val::Val;
pub use crate::types::value::Value;
pub use crate::utils::compare::{compare_between, compare_vals};
pub use crate::utils::conversion::{convert, marshal, marshal_json, object_value, parse_vfloat};
pub use crate::utils::earth::{EARTH_RADIUS_METERS, earth_angle, earth_area, earth_distance};
pub use crate::utils::geofilter::{get_geo_tokens, is_geo_func, match_geo, query_tokens_geo};
pub use crate::utils::password::{encrypt, verify_password};
pub use crate::utils::s2::{convert_to_geom, loop_contains, loop_intersects};
pub use crate::utils::s2index::{
    MAX_CELL_LEVEL, MAX_CELLS, MIN_CELL_LEVEL, index_cells, index_geo_tokens, loop_from_polygon,
};
pub use crate::utils::scalar_types::{
    BIG_FLOAT_PRECISION, good_time_zone, parse_time, value_for_type,
};
pub use crate::utils::sort::{equal, is_sortable, less, sort, sort_top_n, sort_with_facet};
pub use crate::utils::value::{
    bytes_as_float_array, float_array_as_bytes, float_array_as_string, type_for_value,
};
