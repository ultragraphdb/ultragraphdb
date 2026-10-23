/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `schema` (parsing and in-memory state): the schema language and the schema state.
//!
//! [`parse`] turns schema text into predicate updates (`pb::SchemaUpdate`) and type updates (`pb::TypeUpdate`),
//! each named by its namespaced attribute (`0-name`); [`parse_with_namespace`] places every one in a given
//! namespace. [`SchemaState`] holds the current definitions and answers for them; [`initial_schema`] and
//! [`initial_types`] are the definitions every namespace starts with. Loading and storing the schema is stage 2's.
//!
//! ```
//! use ultragraphdb_core_schema::{SchemaError, SchemaMode, SchemaState, parse};
//!
//! # fn main() -> Result<(), SchemaError> {
//! let parsed = parse("name: string @index(exact, term) @upsert .\nfriend: [uid] @reverse .")?;
//! assert_eq!(parsed.preds()[0].predicate, "0-name");
//! assert_eq!(parsed.preds()[0].tokenizer, ["exact", "term"]);
//!
//! let state = SchemaState::new();
//! for update in parsed.into_parts().0 {
//!     state.set(&update.predicate.clone(), update);
//! }
//! assert!(state.has_upsert("0-name"));
//! assert!(state.is_reversed(SchemaMode::Query, "0-friend"));
//!
//! let err = parse("friend: [bool] .").unwrap_err();
//! assert_eq!(err.to_string(), "line 1 column 9: Unsupported type for list: [bool].");
//! # Ok(())
//! # }
//! ```

mod errors;
mod parse;
mod schema;
mod state;
mod types;

pub use crate::errors::schema_error::{SchemaError, SchemaErrorEnum};
pub use crate::parse::{has_tokenizer_or_vector_index_spec, parse, parse_with_namespace};
pub use crate::schema::initial::{
    complete_initial_schema, complete_initial_types, initial_schema, initial_types,
};
pub use crate::schema::pre_defined::{
    IS_UNIQUE_DGRAPH_XID, check_and_modify_pre_def_predicate, compare_schema_updates,
    is_pre_def_type_changed,
};
pub use crate::types::parsed_schema::ParsedSchema;
pub use crate::types::schema_mode::SchemaMode;
pub use crate::types::schema_state::SchemaState;
