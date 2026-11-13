/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The S2 cells a position on the Earth is filed under, and the hierarchy a cell sits in. S2 projects the sphere onto
//! the six faces of a cube and divides each face as a quadtree thirty levels deep, numbering the cells along a Hilbert
//! curve, so a cell's descendants at every level occupy one contiguous range of identifiers. The arithmetic is written
//! here, with no dependency, and checked against the vectors the reference library publishes.
//!
//! - [`CellId::at`] is the leaf cell, about a centimetre across, holding a point given in degrees.
//! - [`cells_around`] is the cells a point with an uncertainty is filed under.
//! - [`CellId::parent`], [`CellId::ancestors`] and [`CellId::descendant_range`] walk the hierarchy; [`CellId::token`]
//!   is a cell's short text.
//!
//! ```
//! use ultragraphdb_core_cell::CellId;
//!
//! let leaf = CellId::at(49.703498679, 11.770681595);
//! assert_eq!(leaf.map(CellId::value), Some(0x47a1cbd595522b39));
//! assert_eq!(leaf.and_then(CellId::level), Some(30));
//! ```

mod types;
mod utils;

pub use crate::types::cell_id::{CellId, MAX_LEVEL};
pub use crate::utils::cells::{EARTH_RADIUS_METRES, cells_around};
