/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_cell::{CellId, MAX_LEVEL};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::fixtures::{PUBLISHED, face_cell, leaf};

/// The published leaf cells are reproduced from their points.
#[test]
fn test_published_identifiers_are_reproduced() -> TestResult {
    for (id, lat, lng) in PUBLISHED {
        assert_eq!(leaf(lat, lng)?, CellId::new(id), "{lat}, {lng}");
        assert_eq!(leaf(lat, lng)?.level(), Some(MAX_LEVEL));
    }
    Ok(())
}

/// The published face definitions (`s2cell_id_test.cc`, `FaceDefinitions`): one point per face, both poles among
/// them.
#[test]
fn test_each_face_holds_its_published_point() -> TestResult {
    for (lat, lng, expected) in [
        (0.0, 0.0, 0),
        (0.0, 90.0, 1),
        (90.0, 0.0, 2),
        (0.0, 180.0, 3),
        (0.0, -90.0, 4),
        (-90.0, 0.0, 5),
    ] {
        assert_eq!(leaf(lat, lng)?.face(), Some(expected), "{lat}, {lng}");
    }
    Ok(())
}

/// The antimeridian is no seam: both sides of it lie on face 3.
#[test]
fn test_the_antimeridian_lies_within_one_face() -> TestResult {
    for lat in [-30.0, 0.0, 30.0] {
        assert_eq!(leaf(lat, 179.999_999)?.face(), Some(3));
        assert_eq!(leaf(lat, -179.999_999)?.face(), Some(3));
    }
    assert_eq!(leaf(0.0, -180.0)?.face(), Some(3));
    Ok(())
}

/// A coordinate that is NaN or infinite has no cell.
#[test]
fn test_a_non_finite_coordinate_has_no_cell() {
    for (lat, lng) in [
        (f64::NAN, 0.0),
        (0.0, f64::NAN),
        (f64::INFINITY, 0.0),
        (0.0, f64::NEG_INFINITY),
    ] {
        assert_eq!(CellId::at(lat, lng), None, "{lat}, {lng}");
    }
}

/// S2 numbers a point's antipode, on the opposite face, at the same position. So each published point on faces 0, 1
/// and 2 checks faces 3, 4 and 5.
#[test]
fn test_a_published_points_antipode_has_its_position_on_the_opposite_face() -> TestResult {
    for (id, lat, lng) in PUBLISHED {
        if id >> 61 > 2 {
            continue;
        }
        let antipode_lng = if lng > 0.0 { lng - 180.0 } else { lng + 180.0 };
        assert_eq!(
            leaf(-lat, antipode_lng)?,
            CellId::new(id + (3 << 61)),
            "{lat}, {lng}"
        );
    }
    Ok(())
}

/// The published parent arithmetic on face 3 (`TestCellIDParentChildRelationships`): position 0x12345678 at level
/// 26, its parent two levels up, and its descendant range.
#[test]
fn test_the_published_face_3_hierarchy_is_reproduced() {
    let cell = CellId::new(0x6000_0000_1234_5700);
    assert_eq!((cell.face(), cell.level()), (Some(3), Some(26)));
    assert_eq!(cell.parent(24), Some(CellId::new(0x6000_0000_1234_5000)));
    assert_eq!(
        cell.descendant_range(),
        Some((
            CellId::new(0x6000_0000_1234_5601),
            CellId::new(0x6000_0000_1234_57ff)
        ))
    );
}

/// Levels 0 and 30: a leaf's face is its level-0 ancestor, and a leaf is its own range.
#[test]
fn test_levels_0_and_30() -> TestResult {
    let cell = leaf(-75.000000031, 0.000000133)?;
    assert_eq!(cell.level(), Some(MAX_LEVEL));
    assert_eq!(cell.parent(30), Some(cell));
    assert_eq!(cell.parent(0), Some(face_cell(5)));
    assert_eq!(cell.parent(31), None);
    assert_eq!(cell.descendant_range(), Some((cell, cell)));
    assert_eq!(
        face_cell(5).descendant_range(),
        Some((
            CellId::new(0xa000_0000_0000_0001),
            CellId::new(0xbfff_ffff_ffff_ffff)
        ))
    );
    Ok(())
}

/// A cell lies inside every ancestor's range, and each range within one face, faces 4 and 5 included.
#[test]
fn test_a_cell_lies_inside_every_ancestors_range_within_its_face() -> TestResult {
    for (lat, lng) in [
        (-24.694439215, -47.537363213),
        (-69.219262171, 49.670072392),
    ] {
        let cell = leaf(lat, lng)?;
        let chain = cell.ancestors();
        assert_eq!(chain.len(), 30);
        for (expected_level, ancestor) in chain.iter().enumerate() {
            assert_eq!(ancestor.level(), u8::try_from(expected_level).ok());
            let (first, last) = ancestor.descendant_range().context("a cell")?;
            assert!(first <= cell && cell <= last, "{ancestor:?}");
            assert_eq!(first.face(), cell.face());
            assert_eq!(last.face(), cell.face());
        }
        let face = u64::from(cell.face().context("a face")?);
        assert_eq!(chain.first(), Some(&face_cell(face)));
    }
    Ok(())
}

/// An identifier that is no cell has no level, face, parent, ancestors or range.
#[test]
fn test_an_identifier_that_is_no_cell_has_no_hierarchy() {
    for value in [
        0,                     // no level marker
        0xd000_0000_0000_0000, // face 6
        0x4000_0000_0000_0002, // the marker at an odd position
        0x4000_0000_0000_0000, // the marker at bit 62, above level 0
    ] {
        let cell = CellId::new(value);
        assert_eq!(cell.level(), None, "{value:x}");
        assert_eq!(cell.face(), None, "{value:x}");
        assert_eq!(cell.parent(0), None, "{value:x}");
        assert!(cell.ancestors().is_empty(), "{value:x}");
        assert_eq!(cell.descendant_range(), None, "{value:x}");
    }
}

/// A token is the identifier's hexadecimal digits without the trailing zeros: sixteen for a leaf, one for a face, and
/// `X` for the identifier 0. A coarser cell's token starts its descendants'.
#[test]
fn test_a_token_drops_the_trailing_zeros() -> TestResult {
    let cell = leaf(49.703498679, 11.770681595)?;
    assert_eq!(cell.token(), "47a1cbd595522b39");
    assert_eq!(face_cell(2).token(), "5");
    assert_eq!(CellId::new(0).token(), "X");
    assert_eq!(CellId::new(cell.value()).value(), 0x47a1cbd595522b39);
    let coarse = cell.parent(5).context("a level-5 ancestor")?;
    assert_eq!(coarse.token(), "47a4");
    Ok(())
}
