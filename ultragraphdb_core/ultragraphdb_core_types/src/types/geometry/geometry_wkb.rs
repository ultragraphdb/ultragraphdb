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
        let b: [u8; 4] = self
            .read_full(4)?
            .try_into()
            .expect("read_full returns exactly the 4 bytes asked for");
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
        let len = n
            .checked_mul(stride)
            .ok_or_else(|| TypesError::geo(UNEXPECTED_EOF))?;
        self.read_floats(big, len)
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

/// A multi-geometry or collection being read: the geometry so far, the members left to read, the member kind a
/// multi-geometry takes with its Go type name, and the layout an empty collection keeps.
struct Open {
    g: Geometry,
    left: u32,
    member: Option<(GeometryKind, &'static str)>,
    layout: Layout,
}

impl Open {
    fn close(self) -> Geometry {
        if self.member.is_none() && self.g.geoms().is_empty() {
            return Geometry::empty_collection(self.layout);
        }
        self.g
    }
}

/// What one geometry's header and body read to: a whole geometry, or a multi-geometry or collection whose members
/// follow.
enum Read {
    Done(Geometry),
    Open(Open),
}

/// go-geom's `Read`, its recursion into the members of multi-geometries and collections kept on a stack: WKB nests
/// collections without a limit, which a recursion would overflow a thread's stack with.
fn read(r: &mut Reader<'_>) -> Result<Geometry, TypesError> {
    let mut stack: Vec<Open> = Vec::new();
    loop {
        let mut done = match read_one(r)? {
            Read::Done(g) => Some(g),
            Read::Open(open) if open.left == 0 => Some(open.close()),
            Read::Open(open) => {
                stack.push(open);
                None
            }
        };
        while let Some(m) = done.take() {
            let Some(top) = stack.last_mut() else {
                return Ok(m);
            };
            if let Some((member, want)) = top.member
                && m.kind() != member
            {
                return Err(TypesError::geo(format!(
                    "wkb: got {}, want {want}",
                    m.kind().go_type()
                )));
            }
            top.g.push(m)?;
            top.left -= 1;
            if top.left == 0
                && let Some(open) = stack.pop()
            {
                done = Some(open.close());
            }
        }
    }
}

/// Reads one geometry's byte order, type and body; a multi-geometry's or collection's members are left to read.
fn read_one(r: &mut Reader<'_>) -> Result<Read, TypesError> {
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
        1 => Ok(Read::Done(Geometry::from_flat(
            GeometryKind::Point,
            layout,
            r.read_floats(big, stride)?,
            Vec::new(),
        ))),
        2 => Ok(Read::Done(Geometry::from_flat(
            GeometryKind::LineString,
            layout,
            r.read_flat1(big, stride)?,
            Vec::new(),
        ))),
        3 => {
            let n = r.read_u32(big)?;
            let (mut flat, mut ends) = (Vec::new(), Vec::new());
            for _ in 0..n {
                flat.extend(r.read_flat1(big, stride)?);
                ends.push(flat.len());
            }
            Ok(Read::Done(Geometry::from_flat(
                GeometryKind::Polygon,
                layout,
                flat,
                ends,
            )))
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
            Ok(Read::Open(Open {
                left: r.read_u32(big)?,
                g: Geometry::empty_multi(kind, layout),
                member: Some((member, want)),
                layout,
            }))
        }
        7 => Ok(Read::Open(Open {
            left: r.read_u32(big)?,
            g: Geometry::collection(Vec::new()),
            member: None,
            layout,
        })),
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

/// go-geom's `Write`, its recursion into the members of collections kept on a stack: a collection's header, then
/// each member in order.
fn write(out: &mut Vec<u8>, root: &Geometry) -> Result<(), TypesError> {
    // Every geometry in writing order with its parent's place, then each one's layout and emptiness, those of a
    // collection without a layout joined from its members, deepest first, so that no subtree is walked twice.
    let mut nodes: Vec<(&Geometry, Option<usize>)> = Vec::new();
    let mut stack = vec![(root, None)];
    while let Some((g, parent)) = stack.pop() {
        let at = nodes.len();
        nodes.push((g, parent));
        if g.kind() == GeometryKind::GeometryCollection {
            stack.extend(g.geoms().iter().rev().map(|m| (m, Some(at))));
        }
    }
    let joined = |g: &Geometry| {
        g.kind() == GeometryKind::GeometryCollection && g.layout == Layout::NO_LAYOUT
    };
    let mut shape: Vec<(Layout, bool)> = nodes
        .iter()
        .map(|(g, _)| match g.kind() {
            GeometryKind::GeometryCollection => (g.layout, true),
            _ => (g.layout, g.flat_coords().is_empty()),
        })
        .collect();
    for i in (0..nodes.len()).rev() {
        if let Some(p) = nodes[i].1 {
            let (layout, empty) = shape[i];
            let parent = &mut shape[p];
            if joined(nodes[p].0) {
                parent.0 = parent.0.widest(layout);
            }
            parent.1 &= empty;
        }
    }
    nodes
        .iter()
        .zip(shape)
        .try_for_each(|((g, _), (layout, empty))| write_one(out, g, layout, empty))
}

/// Writes one geometry of `layout`, its own or for a collection without one its members' widest, and `empty`
/// whether it holds no coordinates; a collection's header and member count alone. The members of a multi-geometry
/// are points, lines or polygons, which hold no members of their own.
fn write_one(
    out: &mut Vec<u8>,
    g: &Geometry,
    layout: Layout,
    empty: bool,
) -> Result<(), TypesError> {
    out.push(1);
    let mut code = kind_code(g.kind());
    match layout {
        Layout::NO_LAYOUT => {
            if g.kind() != GeometryKind::GeometryCollection || !empty {
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
                write_one(out, &p, layout, p.is_empty())?;
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
                write_one(out, &ls, layout, ls.is_empty())?;
                offset = end;
            }
        }
        GeometryKind::MultiPolygon => {
            put_u32(out, g.num_polygons() as u32);
            for i in 0..g.num_polygons() {
                let p = g.polygon_at(i);
                write_one(out, &p, layout, p.is_empty())?;
            }
        }
        GeometryKind::GeometryCollection => put_u32(out, g.geoms().len() as u32),
    }
    Ok(())
}
