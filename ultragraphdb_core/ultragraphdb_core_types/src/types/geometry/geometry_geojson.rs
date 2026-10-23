/*
 * Copyright (c) 2013, Tom Payne. All rights reserved.
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Ported from go-geom v1.6.1, encoding/geojson/geojson.go. Changed for ultragraphdb: translated to Rust over this
 * crate's port of encoding/json; Go's JSON layout of the geometry struct and of float64 coordinates.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::types::geometry::Geometry;
use crate::types::geometry_kind::GeometryKind;
use crate::types::layout::Layout;
use crate::utils::json::{self, JsonValue, type_error};
use crate::utils::json_float::json_float;
use ultragraphdb_common_utils::go_strconv;

/// go-geom's `geojson.Geometry`: the type and the raw coordinates or member geometries (`None` when absent or
/// null).
#[derive(Debug, Clone, Default)]
pub(crate) struct GeoJson {
    pub(crate) kind: String,
    pub(crate) coordinates: Option<JsonValue>,
    pub(crate) geometries: Option<JsonValue>,
}

/// Collects Go's first `UnmarshalTypeError` while decoding continues, as `encoding/json` does, with the error's
/// path: the root type's name and the array indices or field names below it.
struct Decoder {
    root: &'static str,
    root_type: &'static str,
    path: Vec<String>,
    first: Option<String>,
}

impl Decoder {
    fn new(root: &'static str, root_type: &'static str) -> Self {
        Self {
            root,
            root_type,
            path: Vec::new(),
            first: None,
        }
    }

    /// Go's `UnmarshalTypeError.Error` (Go 1.27): the value's kind, where it sits, and the Go type it missed.
    fn error(&mut self, value: &str, go_type: &str) {
        if self.first.is_some() {
            return;
        }
        let msg = if self.path.is_empty() {
            json::type_error(
                value,
                if go_type.is_empty() {
                    self.root_type
                } else {
                    go_type
                },
            )
        } else {
            let field = self.path.join(".");
            let numeric = self
                .path
                .last()
                .is_some_and(|t| t.bytes().all(|c| c.is_ascii_digit()));
            let into = if numeric { "" } else { "Go struct field " };
            format!(
                "json: cannot unmarshal {value} into {into}{}.{field} of type {go_type}",
                self.root
            )
        };
        self.first = Some(msg);
    }

    fn finish<T>(self, v: T) -> Result<T, TypesError> {
        match self.first {
            Some(e) => Err(TypesError::geo(e)),
            None => Ok(v),
        }
    }

    fn at<T>(&mut self, i: usize, f: impl FnOnce(&mut Self) -> T) -> T {
        self.path.push(i.to_string());
        let v = f(self);
        self.path.pop();
        v
    }

    fn float(&mut self, v: &JsonValue) -> f64 {
        match v {
            JsonValue::Null => 0.0,
            JsonValue::Number(n) => match go_strconv::parse_float(n, 64) {
                Ok(f) => f,
                Err(_) => {
                    self.error(&format!("number {n}"), "float64");
                    0.0
                }
            },
            other => {
                self.error(other.kind(), "float64");
                0.0
            }
        }
    }

    /// `geom.Coord`: `None` for null.
    fn coord(&mut self, v: &JsonValue) -> Option<Vec<f64>> {
        match v {
            JsonValue::Null => None,
            JsonValue::Array(items) => Some(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, x)| self.at(i, |d| d.float(x)))
                    .collect(),
            ),
            other => {
                self.error(other.kind(), "geom.Coord");
                None
            }
        }
    }

    fn coords1(&mut self, v: &JsonValue) -> Vec<Option<Vec<f64>>> {
        match v {
            JsonValue::Null => Vec::new(),
            JsonValue::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, x)| self.at(i, |d| d.coord(x)))
                .collect(),
            other => {
                self.error(other.kind(), "[]geom.Coord");
                Vec::new()
            }
        }
    }

    fn coords2(&mut self, v: &JsonValue) -> Vec<Vec<Option<Vec<f64>>>> {
        match v {
            JsonValue::Null => Vec::new(),
            JsonValue::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, x)| self.at(i, |d| d.coords1(x)))
                .collect(),
            other => {
                self.error(other.kind(), "[][]geom.Coord");
                Vec::new()
            }
        }
    }

    fn coords3(&mut self, v: &JsonValue) -> Vec<Vec<Vec<Option<Vec<f64>>>>> {
        match v {
            JsonValue::Null => Vec::new(),
            JsonValue::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, x)| self.at(i, |d| d.coords2(x)))
                .collect(),
            other => {
                self.error(other.kind(), "[][][]geom.Coord");
                Vec::new()
            }
        }
    }

    /// A `geojson.Geometry` object's fields.
    fn geometry(&mut self, members: &[(String, JsonValue)]) -> GeoJson {
        let mut g = GeoJson::default();
        for (k, val) in members {
            let field = k.to_ascii_lowercase();
            match field.as_str() {
                "type" => match val {
                    JsonValue::String(s) => g.kind = s.clone(),
                    JsonValue::Null => {}
                    other => {
                        self.path.push("type".into());
                        self.error(other.kind(), "string");
                        self.path.pop();
                    }
                },
                "coordinates" => g.coordinates = (*val != JsonValue::Null).then(|| val.clone()),
                "geometries" => g.geometries = (*val != JsonValue::Null).then(|| val.clone()),
                "crs" => match val {
                    JsonValue::Null | JsonValue::Object(_) => {}
                    other => {
                        self.path.push("crs".into());
                        self.error(other.kind(), "geojson.CRS");
                        self.path.pop();
                    }
                },
                _ => {}
            }
        }
        g
    }
}

/// A nil `geom.Coord` inside a list is an empty coordinate when go-geom deflates it.
fn unwrap1(c: Vec<Option<Vec<f64>>>) -> Vec<Vec<f64>> {
    c.into_iter().map(Option::unwrap_or_default).collect()
}

fn unwrap2(c: Vec<Vec<Option<Vec<f64>>>>) -> Vec<Vec<Vec<f64>>> {
    c.into_iter().map(unwrap1).collect()
}

/// go-geom's `guessLayout0`.
fn guess_layout0(c: &[f64]) -> Result<Layout, TypesError> {
    match c.len() {
        n @ (0 | 1) => Err(TypesError::geo(format!(
            "geojson: dimensionality too low ({n})"
        ))),
        2 => Ok(Layout::XY),
        3 => Ok(Layout::XYZ),
        4 => Ok(Layout::XYZM),
        n => Ok(Layout::new(n as u32)),
    }
}

fn guess_layout1(c: &[Option<Vec<f64>>]) -> Result<Layout, TypesError> {
    match c.first() {
        None => Ok(Layout::XY),
        Some(first) => guess_layout0(first.as_deref().unwrap_or(&[])),
    }
}

fn guess_layout2(c: &[Vec<Option<Vec<f64>>>]) -> Result<Layout, TypesError> {
    c.first().map_or(Ok(Layout::XY), |f| guess_layout1(f))
}

fn guess_layout3(c: &[Vec<Vec<Option<Vec<f64>>>>]) -> Result<Layout, TypesError> {
    c.first().map_or(Ok(Layout::XY), |f| guess_layout2(f))
}

impl GeoJson {
    /// Go's `json.Unmarshal` of an object into `geojson.Geometry`: fields matched without regard to case, the
    /// last occurrence winning, unknown ones ignored. `Ok(None)` for JSON `null`.
    pub(crate) fn from_value(v: &JsonValue) -> Result<Option<GeoJson>, TypesError> {
        let members = match v {
            JsonValue::Null => return Ok(None),
            JsonValue::Object(m) => m,
            other => {
                return Err(TypesError::geo(type_error(
                    other.kind(),
                    "geojson.Geometry",
                )));
            }
        };
        let mut d = Decoder::new("Geometry", "geojson.Geometry");
        let g = d.geometry(members);
        d.finish(Some(g))
    }

    /// go-geom's `Geometry.Decode`.
    pub(crate) fn decode(&self) -> Result<Geometry, TypesError> {
        let kind = self.kind.as_str();
        let mut d = match kind {
            "Point" => Decoder::new("Coord", "geom.Coord"),
            "LineString" | "MultiPoint" => Decoder::new("", "[]geom.Coord"),
            "Polygon" | "MultiLineString" => Decoder::new("", "[][]geom.Coord"),
            _ => Decoder::new("", "[][][]geom.Coord"),
        };
        let Some(c) = &self.coordinates else {
            return match kind {
                "Point" => Ok(Geometry::point_empty(Layout::NO_LAYOUT)),
                "LineString" => Ok(Geometry::empty_multi(
                    GeometryKind::LineString,
                    Layout::NO_LAYOUT,
                )),
                "Polygon" => Ok(Geometry::empty_multi(
                    GeometryKind::Polygon,
                    Layout::NO_LAYOUT,
                )),
                "MultiPoint" => Ok(Geometry::empty_multi(
                    GeometryKind::MultiPoint,
                    Layout::NO_LAYOUT,
                )),
                "MultiLineString" => Ok(Geometry::empty_multi(
                    GeometryKind::MultiLineString,
                    Layout::NO_LAYOUT,
                )),
                "MultiPolygon" => Ok(Geometry::empty_multi(
                    GeometryKind::MultiPolygon,
                    Layout::NO_LAYOUT,
                )),
                "GeometryCollection" => self.decode_collection(),
                _ => Err(TypesError::geo(format!(
                    "geojson: unsupported type: {kind}"
                ))),
            };
        };
        match kind {
            "Point" => {
                let coords = d.coord(c);
                let coords = d.finish(coords)?.unwrap_or_default();
                if coords.is_empty() {
                    return Ok(Geometry::point_empty(Layout::XY));
                }
                Geometry::point(guess_layout0(&coords)?, &coords)
            }
            "LineString" => {
                let coords = d.coords1(c);
                let coords = d.finish(coords)?;
                Geometry::line_string(guess_layout1(&coords)?, &unwrap1(coords))
            }
            "Polygon" => {
                let coords = d.coords2(c);
                let coords = d.finish(coords)?;
                Geometry::polygon(guess_layout2(&coords)?, &unwrap2(coords))
            }
            "MultiPoint" => {
                let coords = d.coords1(c);
                let coords = d.finish(coords)?;
                Geometry::multi_point(guess_layout1(&coords)?, &coords)
            }
            "MultiLineString" => {
                let coords = d.coords2(c);
                let coords = d.finish(coords)?;
                Geometry::multi_line_string(guess_layout2(&coords)?, &unwrap2(coords))
            }
            "MultiPolygon" => {
                let coords = d.coords3(c);
                let coords = d.finish(coords)?;
                let layout = guess_layout3(&coords)?;
                Geometry::multi_polygon(
                    layout,
                    &coords.into_iter().map(unwrap2).collect::<Vec<_>>(),
                )
            }
            "GeometryCollection" => self.decode_collection(),
            _ => Err(TypesError::geo(format!(
                "geojson: unsupported type: {kind}"
            ))),
        }
    }

    fn decode_collection(&self) -> Result<Geometry, TypesError> {
        let members = match &self.geometries {
            None => Vec::new(),
            Some(JsonValue::Array(items)) => {
                let mut d = Decoder::new("", "[]geojson.Geometry");
                let mut out = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let g = d.at(i, |d| match item {
                        JsonValue::Object(m) => d.geometry(m),
                        JsonValue::Null => GeoJson::default(),
                        other => {
                            d.error(other.kind(), "geojson.Geometry");
                            GeoJson::default()
                        }
                    });
                    out.push(g);
                }
                d.finish(out)?
            }
            Some(other) => {
                return Err(TypesError::geo(type_error(
                    other.kind(),
                    "[]geojson.Geometry",
                )));
            }
        };
        let geoms = members
            .iter()
            .map(GeoJson::decode)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Geometry::collection(geoms))
    }
}

impl Geometry {
    /// go-geom's `geojson.Unmarshal`: `Ok(None)` for JSON `null`, the geometry otherwise.
    pub fn from_geojson(text: impl AsRef<[u8]>) -> Result<Option<Geometry>, TypesError> {
        let v = json::parse(text.as_ref()).map_err(TypesError::geo)?;
        match GeoJson::from_value(&v)? {
            None => Ok(None),
            Some(g) => g.decode().map(Some),
        }
    }

    /// go-geom's `geojson.Marshal`: `{"type":...,"coordinates":...}` or `{"type":...,"geometries":[...]}`;
    /// `None` when a coordinate is NaN or infinite, which Go's `json.Marshal` refuses.
    pub fn to_geojson(&self) -> Option<String> {
        let mut out = String::new();
        encode(&mut out, self)?;
        Some(out)
    }
}

fn push_coord(out: &mut String, c: &[f64]) -> Option<()> {
    out.push('[');
    for (i, f) in c.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_float(*f)?);
    }
    out.push(']');
    Some(())
}

fn push_coords1(out: &mut String, flat: &[f64], stride: usize) -> Option<()> {
    out.push('[');
    if stride > 0 {
        for (i, c) in flat.chunks(stride).enumerate() {
            if i > 0 {
                out.push(',');
            }
            push_coord(out, c)?;
        }
    }
    out.push(']');
    Some(())
}

fn push_coords2(out: &mut String, flat: &[f64], ends: &[usize], stride: usize) -> Option<()> {
    out.push('[');
    let mut offset = 0;
    for (i, &end) in ends.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_coords1(out, &flat[offset..end], stride)?;
        offset = end;
    }
    out.push(']');
    Some(())
}

fn encode(out: &mut String, g: &Geometry) -> Option<()> {
    out.push_str("{\"type\":\"");
    out.push_str(g.kind().name());
    out.push_str("\",");
    let stride = g.layout().stride();
    match g.kind() {
        GeometryKind::GeometryCollection => {
            out.push_str("\"geometries\":[");
            for (i, m) in g.geoms().iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                encode(out, m)?;
            }
            out.push(']');
        }
        kind => {
            out.push_str("\"coordinates\":");
            match kind {
                GeometryKind::Point => {
                    if g.is_empty() {
                        out.push_str("[]");
                    } else {
                        push_coord(out, g.flat_coords())?;
                    }
                }
                GeometryKind::LineString => push_coords1(out, g.flat_coords(), stride)?,
                GeometryKind::Polygon | GeometryKind::MultiLineString => {
                    push_coords2(out, g.flat_coords(), g.ends(), stride)?
                }
                GeometryKind::MultiPoint => {
                    out.push('[');
                    for (i, c) in g.multi_point_coords().into_iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        match c {
                            Some(c) => push_coord(out, c)?,
                            None => out.push_str("null"),
                        }
                    }
                    out.push(']');
                }
                _ => {
                    out.push('[');
                    for i in 0..g.num_polygons() {
                        if i > 0 {
                            out.push(',');
                        }
                        let p = g.polygon_at(i);
                        push_coords2(out, p.flat_coords(), p.ends(), stride)?;
                    }
                    out.push(']');
                }
            }
        }
    }
    out.push('}');
    Some(())
}
