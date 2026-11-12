/*
 * Copyright (c) 2013, Tom Payne. All rights reserved.
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Ported from go-geom v1.6.1, encoding/wkb/wkb.go and encoding/wkbcommon. Changed for ultragraphdb: translated to
 * Rust; Dgraph writes little-endian (NDR) and reads either byte order.
 * Copyright (c) 2026. Marvin Hansen.
 */

// Arithmetic on decoded store input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

use crate::errors::types_error::TypesError;
use crate::types::geometry::Geometry;
use crate::types::geometry_kind::GeometryKind;
use crate::types::layout::Layout;

const EOF: &str = "EOF";
const UNEXPECTED_EOF: &str = "unexpected EOF";

/// A `bytes.Buffer` read with `io.ReadFull`: the bytes not yet read.
struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    fn read_full(&mut self, n: usize) -> Result<&'a [u8], TypesError> {
        let Some((s, rest)) = self.data.split_at_checked(n) else {
            let at_end = self.data.is_empty();
            self.data = &[];
            return Err(TypesError::geo(if at_end && n > 0 {
                EOF
            } else {
                UNEXPECTED_EOF
            }));
        };
        self.data = rest;
        Ok(s)
    }

    fn read_u32(&mut self, big: bool) -> Result<u32, TypesError> {
        // read_full returns exactly the 4 bytes asked for, so the chunk is present.
        let b: [u8; 4] = *self
            .read_full(4)?
            .first_chunk()
            .ok_or_else(|| TypesError::geo(UNEXPECTED_EOF))?;
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
        read(&mut Reader { data })
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
            // A collection with no members left is closed below, so one is left here.
            top.left = top.left.checked_sub(1).ok_or_else(|| {
                TypesError::geo("wkb: invariant violated: a member past the count")
            })?;
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
    let big = match r.read_full(1)? {
        [0] => true,
        [1] => false,
        [b] => return Err(TypesError::geo(format!("wkb: unknown byte order: {b:b}"))),
        _ => return Err(TypesError::geo(UNEXPECTED_EOF)),
    };
    let t = r.read_u32(big)?;
    let layout = match t / 1000 {
        0 => Layout::XY,
        1 => Layout::XYZ,
        2 => Layout::XYM,
        3 => Layout::XYZM,
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
            let (kind, member, want) = multi_kinds(code);
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

/// The kind of a multi-geometry of type code 4, 5 or 6, its members' kind and Go's name of that kind.
fn multi_kinds(code: u32) -> (GeometryKind, GeometryKind, &'static str) {
    match code {
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
    }
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_floats(out: &mut Vec<u8>, v: &[f64]) {
    v.iter()
        .for_each(|f| out.extend_from_slice(&f.to_bits().to_le_bytes()));
}

/// A count as WKB's u32; go-geom writes `uint32(n)`, which this port refuses past u32 instead of truncating.
fn put_count(out: &mut Vec<u8>, n: usize) -> Result<(), TypesError> {
    let n =
        u32::try_from(n).map_err(|_| TypesError::geo(format!("wkb: count {n} exceeds uint32")))?;
    put_u32(out, n);
    Ok(())
}

fn put_flat1(out: &mut Vec<u8>, coords: &[f64], stride: usize) -> Result<(), TypesError> {
    // `layout_code` refuses no layout first, so the stride is at least 2.
    let n = coords
        .len()
        .checked_div(stride)
        .ok_or_else(|| TypesError::geo("geom: unsupported layout NoLayout"))?;
    put_count(out, n)?;
    put_floats(out, coords);
    Ok(())
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
            GeometryKind::Point
            | GeometryKind::LineString
            | GeometryKind::Polygon
            | GeometryKind::MultiPoint
            | GeometryKind::MultiLineString
            | GeometryKind::MultiPolygon => (g.layout, g.flat_coords().is_empty()),
        })
        .collect();
    // Each parent precedes its members, so walking backwards joins a member before its parent is read.
    for (i, &(_, parent_at)) in nodes.iter().enumerate().rev() {
        let (Some(p), Some(&(layout, empty))) = (parent_at, shape.get(i)) else {
            continue;
        };
        let (Some(&(parent_g, _)), Some(parent)) = (nodes.get(p), shape.get_mut(p)) else {
            continue;
        };
        if joined(parent_g) {
            parent.0 = parent.0.widest(layout);
        }
        parent.1 &= empty;
    }
    nodes
        .iter()
        .zip(shape)
        .try_for_each(|((g, _), (layout, empty))| write_one(out, g, layout, empty))
}

/// The part of a type code that names `layout`; no layout only for an empty collection.
fn layout_code(g: &Geometry, layout: Layout, empty: bool) -> Result<u32, TypesError> {
    match layout {
        Layout::NO_LAYOUT => {
            if g.kind() != GeometryKind::GeometryCollection || !empty {
                return Err(TypesError::geo(format!(
                    "geom: unsupported layout {}",
                    layout.name()
                )));
            }
            Ok(0)
        }
        Layout::XY => Ok(0),
        Layout::XYZ => Ok(1000),
        Layout::XYM => Ok(2000),
        Layout::XYZM => Ok(3000),
        _ => Err(TypesError::geo(format!(
            "geom: unsupported layout {}",
            layout.name()
        ))),
    }
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
    // A kind code below 8 and a layout code of at most 3000 sum far inside u32.
    let code = kind_code(g.kind())
        .checked_add(layout_code(g, layout, empty)?)
        .ok_or_else(|| TypesError::geo("wkb: invariant violated: a type code past u32"))?;
    put_u32(out, code);
    let stride = layout.stride();
    match g.kind() {
        GeometryKind::Point => {
            if g.is_empty() {
                return Err(TypesError::geo("cannot encode empty Point in WKB"));
            }
            put_floats(out, g.flat_coords());
        }
        GeometryKind::LineString => put_flat1(out, g.flat_coords(), stride)?,
        GeometryKind::Polygon => {
            put_count(out, g.ends().len())?;
            let mut offset = 0;
            for &end in g.ends() {
                put_flat1(out, ring(g, offset, end)?, stride)?;
                offset = end;
            }
        }
        GeometryKind::MultiPoint => {
            let points = g.multi_point_coords();
            put_count(out, points.len())?;
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
            put_count(out, g.ends().len())?;
            let mut offset = 0;
            for &end in g.ends() {
                let ls = Geometry::from_flat(
                    GeometryKind::LineString,
                    layout,
                    ring(g, offset, end)?.to_vec(),
                    Vec::new(),
                );
                write_one(out, &ls, layout, ls.is_empty())?;
                offset = end;
            }
        }
        GeometryKind::MultiPolygon => {
            put_count(out, g.num_polygons())?;
            for i in 0..g.num_polygons() {
                let p = g.polygon_at(i);
                write_one(out, &p, layout, p.is_empty())?;
            }
        }
        GeometryKind::GeometryCollection => put_count(out, g.geoms().len())?,
    }
    Ok(())
}

/// The coordinates of one ring or line of `g`, from one end to the next; the ends lie within the coordinates of a
/// geometry this crate built.
fn ring(g: &Geometry, start: usize, end: usize) -> Result<&[f64], TypesError> {
    g.flat_coords()
        .get(start..end)
        .ok_or_else(|| TypesError::geo("geom: ends past the coordinates"))
}
