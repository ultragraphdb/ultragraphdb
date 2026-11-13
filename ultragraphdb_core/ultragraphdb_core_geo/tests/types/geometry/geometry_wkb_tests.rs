/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_geo::{GeoErrorEnum, Geometry, Position, Shape};
use ultragraphdb_tool_bdd::TestResult;

use super::fixtures::{one_of_each, one_of_each_elevated};

/// Whether `bytes` are refused as well-known binary.
fn refused(bytes: &[u8]) -> bool {
    matches!(Geometry::from_wkb(bytes), Err(e) if matches!(e.0, GeoErrorEnum::Wkb(_)))
}

/// A header: the byte order, then the type's code in it.
fn header(little: bool, code: u32) -> Vec<u8> {
    let bytes = if little {
        code.to_le_bytes()
    } else {
        code.to_be_bytes()
    };
    [u8::from(little)].into_iter().chain(bytes).collect()
}

/// Every geometry written as well-known binary reads back to itself.
#[test]
fn test_every_geometry_round_trips() -> TestResult {
    for geometry in one_of_each()?.into_iter().chain(one_of_each_elevated()?) {
        assert_eq!(
            Geometry::from_wkb(&geometry.to_wkb())?,
            geometry,
            "{}",
            geometry.name()
        );
    }
    Ok(())
}

/// A point is written as its byte order, its code and its two doubles, little-endian; with an elevation, the code
/// plus a thousand and a third double.
#[test]
fn test_a_point_is_written_byte_for_byte() -> TestResult {
    let position = Position::new(1.0, 2.0)?;
    let flat: Vec<u8> = header(true, 1)
        .into_iter()
        .chain(1.0_f64.to_le_bytes())
        .chain(2.0_f64.to_le_bytes())
        .collect();
    assert_eq!(Geometry::point(position).to_wkb(), flat);
    let raised: Vec<u8> = header(true, 1_001)
        .into_iter()
        .chain(1.0_f64.to_le_bytes())
        .chain(2.0_f64.to_le_bytes())
        .chain(3.0_f64.to_le_bytes())
        .collect();
    assert_eq!(
        Geometry::point(position.with_elevation(3.0)?).to_wkb(),
        raised
    );
    Ok(())
}

/// Big-endian bytes are read, members in an order of their own included.
#[test]
fn test_big_endian_is_read() -> TestResult {
    let line: Vec<u8> = header(false, 2)
        .into_iter()
        .chain(2_u32.to_be_bytes())
        .chain(
            [1.0_f64, 2.0, 3.0, 4.0]
                .into_iter()
                .flat_map(f64::to_be_bytes),
        )
        .collect();
    let expected = vec![Position::new(1.0, 2.0)?, Position::new(3.0, 4.0)?];
    assert_eq!(
        Geometry::from_wkb(&line)?.shape(),
        &Shape::LineString(expected.clone())
    );
    let mixed: Vec<u8> = header(false, 4)
        .into_iter()
        .chain(2_u32.to_be_bytes())
        .chain(Geometry::point(Position::new(1.0, 2.0)?).to_wkb())
        .chain(header(false, 1))
        .chain([3.0_f64, 4.0].into_iter().flat_map(f64::to_be_bytes))
        .collect();
    assert_eq!(
        Geometry::from_wkb(&mixed)?.shape(),
        &Shape::MultiPoint(expected)
    );
    Ok(())
}

/// Bytes cut short anywhere inside a geometry are refused, and so are bytes running on past it.
#[test]
fn test_cut_and_trailing_bytes_are_refused() -> TestResult {
    for geometry in one_of_each()? {
        let bytes = geometry.to_wkb();
        for end in 0..bytes.len() {
            assert!(
                bytes.get(..end).is_some_and(refused),
                "{} cut at {end}",
                geometry.name()
            );
        }
        let long: Vec<u8> = bytes.into_iter().chain([0]).collect();
        assert!(refused(&long), "{}", geometry.name());
    }
    Ok(())
}

/// An unknown byte order or type code is refused.
#[test]
fn test_an_unknown_order_or_type_is_refused() {
    let point = [0_u8; 16];
    let with = |head: Vec<u8>| -> Vec<u8> { head.into_iter().chain(point).collect() };
    assert!(refused(&with(vec![2, 1, 0, 0, 0])));
    for code in [0, 8, 1_000, 1_008, 2_001, 3_001, 0x2000_0001, u32::MAX] {
        assert!(refused(&with(header(true, code))), "{code}");
    }
}

/// A count the remaining bytes cannot fill is refused before anything is allocated for it.
#[test]
fn test_a_count_past_the_bytes_is_refused() {
    for code in [2, 3, 4, 5, 6, 7, 1_002] {
        let bytes: Vec<u8> = header(true, code)
            .into_iter()
            .chain(u32::MAX.to_le_bytes())
            .collect();
        assert!(refused(&bytes), "{code}");
    }
    // Two positions stated, one present.
    let short: Vec<u8> = header(true, 2)
        .into_iter()
        .chain(2_u32.to_le_bytes())
        .chain([0_u8; 16])
        .collect();
    assert!(refused(&short));
}

/// A multi-geometry holding a member of another type is refused.
#[test]
fn test_a_member_of_another_type_is_refused() -> TestResult {
    let members = one_of_each()?;
    for (code, own) in [(4_u32, "Point"), (5, "LineString"), (6, "Polygon")] {
        for member in members.iter().filter(|g| g.name() != own) {
            let bytes: Vec<u8> = header(true, code)
                .into_iter()
                .chain(1_u32.to_le_bytes())
                .chain(member.to_wkb())
                .collect();
            assert!(refused(&bytes), "a {} in type {code}", member.name());
        }
    }
    Ok(())
}

/// Collections nest to the bound and are refused one deeper.
#[test]
fn test_collections_nest_to_the_bound() -> TestResult {
    let nested = |depth: usize| -> Vec<u8> {
        (0..depth)
            .flat_map(|_| header(true, 7).into_iter().chain(1_u32.to_le_bytes()))
            .chain(header(true, 7))
            .chain(0_u32.to_le_bytes())
            .collect()
    };
    let deepest = Geometry::from_wkb(&nested(Geometry::MAX_NESTING))?;
    assert_eq!(deepest.name(), "GeometryCollection");
    assert!(refused(&nested(Geometry::MAX_NESTING + 1)));
    Ok(())
}

/// What a position and a geometry refuse, the bytes are refused for.
#[test]
fn test_coordinates_and_shapes_are_checked() {
    let point = |lng: f64, lat: f64| -> Vec<u8> {
        header(true, 1)
            .into_iter()
            .chain(lng.to_le_bytes())
            .chain(lat.to_le_bytes())
            .collect()
    };
    for (lng, lat) in [(f64::NAN, 0.0), (0.0, 91.0), (f64::INFINITY, 0.0)] {
        assert!(matches!(
            Geometry::from_wkb(&point(lng, lat)),
            Err(e) if matches!(e.0, GeoErrorEnum::Coordinate(_))
        ));
    }
    let short_line: Vec<u8> = header(true, 2)
        .into_iter()
        .chain(1_u32.to_le_bytes())
        .chain([0_u8; 16])
        .collect();
    assert!(matches!(
        Geometry::from_wkb(&short_line),
        Err(e) if matches!(e.0, GeoErrorEnum::Shape(_))
    ));
}
