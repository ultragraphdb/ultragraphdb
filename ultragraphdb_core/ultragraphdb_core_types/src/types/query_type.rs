/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/geofilter.go (QueryType). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Dgraph's `QueryType`: the kind of geo query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryType {
    /// Points within a geometry.
    Within,
    /// Polygons containing a point or polygon.
    Contains,
    /// Geometries intersecting a geometry.
    Intersects,
    /// Points within a distance of a point.
    Near,
}
