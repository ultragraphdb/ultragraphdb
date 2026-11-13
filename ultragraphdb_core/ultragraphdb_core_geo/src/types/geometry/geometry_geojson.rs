/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::Write;

use serde_json::Value;

use crate::errors::geo_error::GeoError;
use crate::types::geometry::Geometry;
use crate::types::position::Position;
use crate::types::shape::Shape;

impl Geometry {
    /// The geometry the GeoJSON `text` holds (RFC 7946, section 3.1): an object with a `type` and, by that type, its
    /// `coordinates` or its `geometries`. Other members are passed over. `serde_json` bounds how deep the document
    /// nests, so no text overflows the stack.
    ///
    /// # Errors
    ///
    /// [`GeoError::json`] for a text that is no JSON document; [`GeoError::structure`] for one that is no geometry
    /// object; [`GeoError::unsupported_type`] for a `type` other than GeoJSON's seven geometry types;
    /// [`GeoError::coordinate`] and [`GeoError::shape`] as [`Position::new`] and [`Geometry::new`] refuse.
    pub fn from_geojson(text: impl AsRef<[u8]>) -> Result<Self, GeoError> {
        let document: Value =
            serde_json::from_slice(text.as_ref()).map_err(|e| GeoError::json(e.to_string()))?;
        geometry(&document)
    }

    /// The geometry as GeoJSON: its `type`, then its `coordinates` or `geometries`, each number in its shortest
    /// digits.
    #[must_use]
    pub fn to_geojson(&self) -> String {
        let mut out = String::new();
        write_geometry(&mut out, self);
        out
    }
}

/// The geometry a JSON value holds.
fn geometry(value: &Value) -> Result<Geometry, GeoError> {
    let Value::Object(members) = value else {
        return Err(GeoError::structure(format!(
            "a geometry is an object, not {}",
            kind(value)
        )));
    };
    let name = match members.get("type") {
        Some(Value::String(name)) => name.as_str(),
        Some(other) => {
            return Err(GeoError::structure(format!(
                "a type is a string, not {}",
                kind(other)
            )));
        }
        None => return Err(GeoError::unsupported_type("")),
    };
    let member = |key: &str| {
        members
            .get(key)
            .ok_or_else(|| GeoError::structure(format!("a {name} without {key}")))
    };
    let shape = match name {
        "Point" => Shape::Point(position(member("coordinates")?)?),
        "LineString" => Shape::LineString(list(member("coordinates")?, position)?),
        "Polygon" => Shape::Polygon(list(member("coordinates")?, |ring| list(ring, position))?),
        "MultiPoint" => Shape::MultiPoint(list(member("coordinates")?, position)?),
        "MultiLineString" => {
            Shape::MultiLineString(list(member("coordinates")?, |line| list(line, position))?)
        }
        "MultiPolygon" => Shape::MultiPolygon(list(member("coordinates")?, |polygon| {
            list(polygon, |ring| list(ring, position))
        })?),
        "GeometryCollection" => Shape::GeometryCollection(list(member("geometries")?, geometry)?),
        other => return Err(GeoError::unsupported_type(other)),
    };
    Geometry::new(shape)
}

/// A JSON array's elements, each read by `read`.
fn list<T>(
    value: &Value,
    read: impl Fn(&Value) -> Result<T, GeoError>,
) -> Result<Vec<T>, GeoError> {
    match value {
        Value::Array(items) => items.iter().map(read).collect(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_) => {
            Err(GeoError::structure(format!(
                "an array of coordinates or geometries, not {}",
                kind(value)
            )))
        }
    }
}

/// A position: an array of a longitude, a latitude and, where stated, an elevation.
fn position(value: &Value) -> Result<Position, GeoError> {
    let numbers = list(value, |item| {
        item.as_f64().ok_or_else(|| {
            GeoError::structure(format!("a coordinate is a number, not {}", kind(item)))
        })
    })?;
    match numbers.as_slice() {
        [lng, lat] => Position::new(*lng, *lat),
        [lng, lat, elevation] => Position::new(*lng, *lat)?.with_elevation(*elevation),
        other => Err(GeoError::structure(format!(
            "a position of {} numbers, where two or three are needed",
            other.len()
        ))),
    }
}

/// A JSON value's kind, as an error names it.
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Writes `geometry` as a GeoJSON object.
fn write_geometry(out: &mut String, geometry: &Geometry) {
    out.push_str("{\"type\":\"");
    out.push_str(geometry.name());
    match geometry.shape() {
        Shape::Point(p) => {
            out.push_str("\",\"coordinates\":");
            write_position(out, p);
        }
        Shape::LineString(line) | Shape::MultiPoint(line) => {
            out.push_str("\",\"coordinates\":");
            write_list(out, line, write_position);
        }
        Shape::Polygon(rings) | Shape::MultiLineString(rings) => {
            out.push_str("\",\"coordinates\":");
            write_list(out, rings, |o, ring| write_list(o, ring, write_position));
        }
        Shape::MultiPolygon(polygons) => {
            out.push_str("\",\"coordinates\":");
            write_list(out, polygons, |o, rings| {
                write_list(o, rings, |inner, ring| {
                    write_list(inner, ring, write_position)
                });
            });
        }
        Shape::GeometryCollection(members) => {
            out.push_str("\",\"geometries\":");
            write_list(out, members, write_geometry);
        }
    }
    out.push('}');
}

/// Writes `items` as a JSON array, each by `write`.
fn write_list<T>(out: &mut String, items: &[T], write: impl Fn(&mut String, &T)) {
    out.push('[');
    for (at, item) in items.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        write(out, item);
    }
    out.push(']');
}

/// Writes a position as its array of numbers. A finite double's `Display` is its shortest digits, a JSON number.
fn write_position(out: &mut String, p: &Position) {
    // Writing to a String never fails.
    let written = match p.elevation() {
        Some(elevation) => write!(out, "[{},{},{}]", p.lng(), p.lat(), elevation),
        None => write!(out, "[{},{}]", p.lng(), p.lat()),
    };
    debug_assert!(written.is_ok(), "a String takes every write");
}
