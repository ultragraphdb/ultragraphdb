/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Geometries on the Earth, in CRS84: longitude and latitude in degrees, with an optional elevation. A [`Geometry`] is
//! read from GeoJSON (RFC 7946) or from well-known binary, checked once as it is built, and written back in either
//! form.
//!
//! What a geometry holds is whole: every coordinate finite and within CRS84's range, a line of two positions or more,
//! a polygon's rings closed, and one geometry's positions all with an elevation or all without. Well-known binary is
//! read within bounds: a count is checked against the bytes that remain before anything is allocated for it, and
//! collections nest to [`Geometry::MAX_NESTING`].
//!
//! ```
//! use ultragraphdb_core_geo::{GeoError, Geometry, Shape};
//!
//! # fn main() -> Result<(), GeoError> {
//! let point = Geometry::from_geojson(br#"{"type":"Point","coordinates":[8.63,50.12]}"#)?;
//! assert!(matches!(point.shape(), Shape::Point(p) if p.lat() == 50.12));
//! assert_eq!(Geometry::from_wkb(&point.to_wkb())?, point);
//! assert_eq!(point.to_geojson(), r#"{"type":"Point","coordinates":[8.63,50.12]}"#);
//! # Ok(())
//! # }
//! ```

mod errors;
mod types;

pub use crate::errors::geo_error::{GeoError, GeoErrorEnum};
pub use crate::types::geometry::Geometry;
pub use crate::types::position::Position;
pub use crate::types::shape::Shape;
