/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `types/facets`: facet value inference, encoding and term tokens.
//!
//! [`facet_for`] reads a facet's text as the RDF parser hands it over and infers its type: an empty or quoted
//! string, an int with Go's base prefixes, a float, a bool, a datetime. [`sort_and_validate`] sorts a facet list
//! and refuses a repeated key.
//!
//! ```
//! use ultragraphdb_core_facets::{facet_for, sort_and_validate, val_for};
//! use ultragraphdb_core_types::Value;
//! use ultragraphdb_proto_dgraph::api::facet::ValType;
//!
//! let n = facet_for("n", "0x10").unwrap();
//! assert_eq!(n.val_type(), ValType::Int);
//! assert_eq!(val_for(&n).unwrap().value(), Some(&Value::Int(16)));
//!
//! let note = facet_for("note", r#""Hello World""#).unwrap();
//! assert_eq!(note.tokens, ["\u{1}hello", "\u{1}world"]);
//!
//! let err = facet_for("note", "abc").unwrap_err();
//! assert_eq!(err.to_string(), "Could not parse the facet value : [abc]");
//!
//! let mut fs = vec![note, n];
//! sort_and_validate(&mut fs).unwrap();
//! assert_eq!(fs[0].key, "n");
//! ```

mod errors;
mod types;
mod utils;

pub use crate::errors::facets_error::{FacetsError, FacetsErrorEnum};
pub use crate::types::facet_type_id::FacetTypeId;
pub use crate::utils::facets::{
    copy_facets, facet_for, sort_and_validate, to_binary, type_id_for, val_for,
};
