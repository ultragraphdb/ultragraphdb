/*
 * Copyright (c) 2013, Tom Payne. All rights reserved.
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Ported from go-geom v1.6.1, encoding/wkb/wkb.go and encoding/wkbcommon. Changed for ultragraphdb: translated to
 * Rust; Dgraph writes little-endian (NDR) and reads either byte order.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::types::geometry::Geometry;
use crate::types::geometry_kind::GeometryKind;
use crate::types::layout::Layout;

const EOF: &str = "EOF";
const UNEXPECTED_EOF: &str = "unexpected EOF";

/// A `bytes.Buffer` read with `io.ReadFull`.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn read_full(&mut self, n: usize) -> Result<&[u8], TypesError> {
        let rest = self.data.len() - self.pos;
        if rest < n {
            self.pos = self.data.len();
            return Err(TypesError::geo(if rest == 0 && n > 0 {
                EOF
            } else {
                UNEXPECTED_EOF
            }));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn read_u32(&mut self, big: bool) -> Result<u32, TypesError> {
        let b: [u8; 4] = self.read_full(4)?.try_into().expect("4 bytes");
        Ok(if big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }

    fn read_floats(&mut self, big: bool, n: usize) -> Result<Vec<f64>, TypesError> {
        let b = self.read_full(
            n.checked_mul(8)
                .ok_or_else(|| TypesError::geo(UNEXPECTED_EOF))?,
        )?;
        Ok(b.as_chunks::<8>()
            .0
            .iter()
            .map(|&a| {
                f64::from_bits(if big {
                    u64::from_be_bytes(a)
                } else {
                    u64::from_le_bytes(a)
                })
            })
            .collect())
    }

    fn read_flat1(&mut self, big: bool, stride: usize) -> Result<Vec<f64>, TypesError> {
        let n = self.read_u32(big)? as usize;
        self.read_floats(big, n * stride)
    }
}

fn kind_code(kind: GeometryKind) -> u32 {
    match kind {
        GeometryKind::Point => 1,
        GeometryKind::LineString => 2,
        GeometryKind::Polygon => 3,
        GeometryKind::MultiPoint => 4,
        GeometryKind::MultiLineString => 5,
        GeometryKind::MultiPolygon => 6,
        GeometryKind::GeometryCollection => 7,
    }
}

impl Geometry {
    /// go-geom's `wkb.Unmarshal`.
    pub fn unmarshal_wkb(data: &[u8]) -> Result<Geometry, TypesError> {
        read(&mut Reader { data, pos: 0 })
    }

    /// go-geom's `wkb.Marshal(g, binary.LittleEndian)`.
    pub fn marshal_wkb(&self) -> Result<Vec<u8>, TypesError> {
        let mut out = Vec::new();
        write(&mut out, self)?;
        Ok(out)
    }
}

fn read(r: &mut Reader<'_>) -> Result<Geometry, TypesError> {
    let order = r.read_full(1)?[0];
    let big = match order {
        0 => true,
        1 => false,
        b => return Err(TypesError::geo(format!("wkb: unknown byte order: {b:b}"))),
    };
    let t = r.read_u32(big)?;
    let layout = match 1000 * (t / 1000) {
        0 => Layout::XY,
        1000 => Layout::XYZ,
        2000 => Layout::XYM,
        3000 => Layout::XYZM,
        _ => return Err(TypesError::geo(format!("wkb: unknown type: {t}"))),
    };
    let stride = layout.stride();
    match t % 1000 {
        1 => Ok(Geometry::from_flat(
            GeometryKind::Point,
            layout,
            r.read_floats(big, stride)?,
            Vec::new(),
        )),
        2 => Ok(Geometry::from_flat(
            GeometryKind::LineString,
            layout,
            r.read_flat1(big, stride)?,
            Vec::new(),
        )),
        3 => {
            let n = r.read_u32(big)?;
            let (mut flat, mut ends) = (Vec::new(), Vec::new());
            for _ in 0..n {
                flat.extend(r.read_flat1(big, stride)?);
                ends.push(flat.len());
            }
            Ok(Geometry::from_flat(
                GeometryKind::Polygon,
                layout,
                flat,
                ends,
            ))
        }
        code @ (4..=6) => {
            let (kind, member, want) = match code {
                4 => (GeometryKind::MultiPoint, GeometryKind::Point, "*geom.Point"),
                5 => (
                    GeometryKind::MultiLineString,
                    GeometryKind::LineString,
                    "*geom.LineString",
                ),
                _ => (
                    GeometryKind::MultiPolygon,
                    GeometryKind::Polygon,
                    "*geom.Polygon",
                ),
            };
            let n = r.read_u32(big)?;
            let mut g = Geometry::empty_multi(kind, layout);
            for _ in 0..n {
                let m = read(r)?;
                if m.kind() != member {
                    return Err(TypesError::geo(format!(
                        "wkb: got {}, want {want}",
                        m.kind().go_type()
                    )));
                }
                g.push(m)?;
            }
            Ok(g)
        }
        7 => {
            let n = r.read_u32(big)?;
            let mut g = Geometry::collection(Vec::new());
            for _ in 0..n {
                let m = read(r)?;
                g.push(m)?;
            }
            if g.is_empty() && g.geoms().is_empty() {
                g = Geometry::empty_collection(layout);
            }
            Ok(g)
        }
        _ => Err(TypesError::geo(format!("wkb: unsupported type: {t}"))),
    }
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_floats(out: &mut Vec<u8>, v: &[f64]) {
    v.iter()
        .for_each(|f| out.extend_from_slice(&f.to_bits().to_le_bytes()));
}

fn put_flat1(out: &mut Vec<u8>, coords: &[f64], stride: usize) {
    put_u32(out, (coords.len() / stride) as u32);
    put_floats(out, coords);
}

fn write(out: &mut Vec<u8>, g: &Geometry) -> Result<(), TypesError> {
    out.push(1);
    let mut code = kind_code(g.kind());
    let layout = g.layout();
    match layout {
        Layout::NO_LAYOUT => {
            if g.kind() != GeometryKind::GeometryCollection || !g.is_empty() {
                return Err(TypesError::geo(format!(
                    "geom: unsupported layout {}",
                    layout.name()
                )));
            }
        }
        Layout::XY => {}
        Layout::XYZ => code += 1000,
        Layout::XYM => code += 2000,
        Layout::XYZM => code += 3000,
        _ => {
            return Err(TypesError::geo(format!(
                "geom: unsupported layout {}",
                layout.name()
            )));
        }
    }
    put_u32(out, code);
    let stride = layout.stride();
    match g.kind() {
        GeometryKind::Point => {
            if g.is_empty() {
                return Err(TypesError::geo("cannot encode empty Point in WKB"));
            }
            put_floats(out, g.flat_coords());
        }
        GeometryKind::LineString => put_flat1(out, g.flat_coords(), stride),
        GeometryKind::Polygon => {
            put_u32(out, g.ends().len() as u32);
            let mut offset = 0;
            for &end in g.ends() {
                put_flat1(out, &g.flat_coords()[offset..end], stride);
                offset = end;
            }
        }
        GeometryKind::MultiPoint => {
            let points = g.multi_point_coords();
            put_u32(out, points.len() as u32);
            for p in points {
                let p = match p {
                    Some(c) => {
                        Geometry::from_flat(GeometryKind::Point, layout, c.to_vec(), Vec::new())
                    }
                    None => Geometry::point_empty(layout),
                };
                write(out, &p)?;
            }
        }
        GeometryKind::MultiLineString => {
            put_u32(out, g.ends().len() as u32);
            let mut offset = 0;
            for &end in g.ends() {
                let ls = Geometry::from_flat(
                    GeometryKind::LineString,
                    layout,
                    g.flat_coords()[offset..end].to_vec(),
                    Vec::new(),
                );
                write(out, &ls)?;
                offset = end;
            }
        }
        GeometryKind::MultiPolygon => {
            put_u32(out, g.num_polygons() as u32);
            for i in 0..g.num_polygons() {
                write(out, &g.polygon_at(i))?;
            }
        }
        GeometryKind::GeometryCollection => {
            put_u32(out, g.geoms().len() as u32);
            for m in g.geoms() {
                write(out, m)?;
            }
        }
    }
    Ok(())
}
