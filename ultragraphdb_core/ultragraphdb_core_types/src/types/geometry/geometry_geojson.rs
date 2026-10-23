/*
 * Copyright (c) 2013, Tom Payne. All rights reserved.
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Ported from go-geom v1.6.1, encoding/geojson/geojson.go. Changed for ultragraphdb: translated to Rust over the
 * port of encoding/json in ultragraphdb_common_utils, with Go 1.27's type errors; Go's JSON layout of the geometry
 * struct and of float64 coordinates; nested geometry collections decoded on a stack rather than by recursion.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::types::geometry::Geometry;
use crate::types::geometry_kind::GeometryKind;
use crate::types::layout::Layout;
use crate::utils::json::{self, JsonValue};
use crate::utils::json_float::json_float;
use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_error_json::JsonError;

/// Go's `UnmarshalTypeError` of a value at the root of what it is decoded into, its text.
fn type_error(value: &str, go_type: &str) -> String {
    JsonError::UnmarshalType(value.into(), go_type.into(), String::new(), String::new()).to_string()
}

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

    /// Go's `UnmarshalTypeError` (Go 1.27): the value's kind, where it sits, and the Go type it missed.
    fn error(&mut self, value: &str, go_type: &str) {
        if self.first.is_some() {
            return;
        }
        let go_type = if go_type.is_empty() && self.path.is_empty() {
            self.root_type
        } else {
            go_type
        };
        // Below the root, the root type's name and the path; a root type without a name gives none.
        let (root, field) = if self.path.is_empty() {
            ("", String::new())
        } else {
            (self.root, self.path.join("."))
        };
        let e = JsonError::UnmarshalType(value.into(), go_type.into(), root.into(), field);
        self.first = Some(e.to_string());
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

    /// A `geojson.CRS` object's fields: `type`, a string, and `properties`, a `map[string]interface{}`.
    fn crs(&mut self, members: &[(String, JsonValue)]) {
        for (k, val) in members {
            let (field, go_type) = match k.to_ascii_lowercase().as_str() {
                "type" if !matches!(val, JsonValue::Null | JsonValue::String(_)) => {
                    ("type", "string")
                }
                "properties" if !matches!(val, JsonValue::Null | JsonValue::Object(_)) => {
                    ("properties", "map[string]interface {}")
                }
                _ => continue,
            };
            self.path.push(field.into());
            self.error(val.kind(), go_type);
            self.path.pop();
        }
    }

    /// A `geojson.Geometry` object's fields; the raw coordinates and member geometries are moved out.
    fn geometry(&mut self, members: Vec<(String, JsonValue)>) -> GeoJson {
        let mut g = GeoJson::default();
        for (k, val) in members {
            let field = k.to_ascii_lowercase();
            match field.as_str() {
                "type" => match &val {
                    JsonValue::String(s) => g.kind = s.clone(),
                    JsonValue::Null => {}
                    other => {
                        self.path.push("type".into());
                        self.error(other.kind(), "string");
                        self.path.pop();
                    }
                },
                "coordinates" => g.coordinates = (val != JsonValue::Null).then_some(val),
                "geometries" => g.geometries = (val != JsonValue::Null).then_some(val),
                "crs" => {
                    self.path.push("crs".into());
                    match &val {
                        JsonValue::Null => {}
                        JsonValue::Object(crs) => self.crs(crs),
                        other => self.error(other.kind(), "geojson.CRS"),
                    }
                    self.path.pop();
                }
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
            JsonValue::Object(m) => m.clone(),
            other => {
                return Err(TypesError::geo(type_error(
                    other.kind(),
                    "geojson.Geometry",
                )));
            }
        };
        // go-geom unmarshals into a `**Geometry`, a type without a name, so a field's error names no root.
        let mut d = Decoder::new("", "geojson.Geometry");
        let g = d.geometry(members);
        d.finish(Some(g))
    }

    /// go-geom's `Geometry.Decode`. A collection's members are decoded depth first, as Go's recursion decodes
    /// them, on a stack of the collections being decoded: Go's JSON nests them up to 4999 deep.
    pub(crate) fn decode(self) -> Result<Geometry, TypesError> {
        if self.kind != "GeometryCollection" {
            return self.decode_one();
        }
        // The collections being decoded: the members left and the geometries decoded.
        let mut stack = vec![(self.collection_members()?.into_iter(), Vec::new())];
        while let Some((members, done)) = stack.last_mut() {
            match members.next() {
                Some(m) if m.kind == "GeometryCollection" => {
                    let members = m.collection_members()?.into_iter();
                    stack.push((members, Vec::new()));
                }
                Some(m) => done.push(m.decode_one()?),
                None => {
                    let Some((_, done)) = stack.pop() else {
                        break;
                    };
                    let g = Geometry::collection(done);
                    match stack.last_mut() {
                        Some((_, parent)) => parent.push(g),
                        None => return Ok(g),
                    }
                }
            }
        }
        Ok(Geometry::collection(Vec::new()))
    }

    /// [`GeoJson::decode`] of a geometry that is not a collection.
    fn decode_one(self) -> Result<Geometry, TypesError> {
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
            _ => Err(TypesError::geo(format!(
                "geojson: unsupported type: {kind}"
            ))),
        }
    }

    /// The members of a `GeometryCollection`: Go's `json.Unmarshal` of its raw geometries into
    /// `[]*geojson.Geometry`, a null member a geometry without a type.
    fn collection_members(self) -> Result<Vec<GeoJson>, TypesError> {
        let Some(mut geometries) = self.geometries else {
            return Ok(Vec::new());
        };
        let items = match &mut geometries {
            JsonValue::Array(items) => std::mem::take(items),
            other => {
                return Err(TypesError::geo(type_error(
                    other.kind(),
                    "[]geojson.Geometry",
                )));
            }
        };
        let mut d = Decoder::new("", "[]geojson.Geometry");
        let mut out = Vec::with_capacity(items.len());
        for (i, mut item) in items.into_iter().enumerate() {
            let g = d.at(i, |d| match &mut item {
                JsonValue::Object(m) => d.geometry(std::mem::take(m)),
                JsonValue::Null => GeoJson::default(),
                other => {
                    d.error(other.kind(), "geojson.Geometry");
                    GeoJson::default()
                }
            });
            out.push(g);
        }
        d.finish(out)
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

/// go-geom's GeoJSON encoding, its recursion into the members of collections kept on a stack: WKB nests
/// collections without a limit, which a recursion would overflow a thread's stack with.
fn encode(out: &mut String, root: &Geometry) -> Option<()> {
    let mut stack: Vec<std::iter::Enumerate<std::slice::Iter<'_, Geometry>>> = Vec::new();
    let mut next = Some(root);
    loop {
        if let Some(g) = next.take() {
            out.push_str("{\"type\":\"");
            out.push_str(g.kind().name());
            out.push_str("\",");
            if g.kind() == GeometryKind::GeometryCollection {
                out.push_str("\"geometries\":[");
                stack.push(g.geoms().iter().enumerate());
            } else {
                encode_coordinates(out, g)?;
                out.push('}');
            }
        }
        // The next member of the innermost open collection, closing those whose members are written.
        loop {
            let Some(members) = stack.last_mut() else {
                return Some(());
            };
            match members.next() {
                Some((i, m)) => {
                    if i > 0 {
                        out.push(',');
                    }
                    next = Some(m);
                    break;
                }
                None => {
                    stack.pop();
                    out.push_str("]}");
                }
            }
        }
    }
}

/// The `"coordinates"` member of a geometry that is not a collection.
fn encode_coordinates(out: &mut String, g: &Geometry) -> Option<()> {
    let stride = g.layout().stride();
    out.push_str("\"coordinates\":");
    match g.kind() {
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
    Some(())
}
