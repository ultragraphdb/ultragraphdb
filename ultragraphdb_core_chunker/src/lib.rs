/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `chunker` (parsers): RDF N-Quad and JSON mutation input.
//!
//! [`parse_rdf`] turns an RDF line into a dgo [`NQuad`](ultragraphdb_proto_dgraph::api::NQuad), [`parse_rdfs`] a
//! block of lines; [`parse_json`] and [`NQuadBuffer::parse_json`] turn a JSON object or array into N-Quads, nested
//! objects into edges and `|` keys into facets. Reading, decompressing and chunking input files belong to the
//! loaders.
//!
//! ```
//! use ultragraphdb_core_chunker::{JsonOp, parse_json, parse_rdf};
//! use ultragraphdb_core_lex::Lexer;
//!
//! let line = r#"<alice> <name> "Alice"@en (since=2006) ."#;
//! let nq = parse_rdf(line, &mut Lexer::new(line)).unwrap();
//! assert_eq!((nq.subject.as_str(), nq.predicate.as_str(), nq.lang.as_str()), ("alice", "name", "en"));
//! assert_eq!(nq.facets[0].key, "since");
//!
//! let (nqs, _) = parse_json(br#"{"uid": "_:a", "friend": {"uid": "_:b", "friend|close": true}}"#, JsonOp::Set)
//!     .unwrap();
//! assert_eq!((nqs[0].subject.as_str(), nqs[0].object_id.as_str()), ("_:a", "_:b"));
//! assert_eq!(nqs[0].facets[0].key, "close");
//! ```

mod chunk;
mod errors;
mod json_parser;
mod rdf_parser;
mod rdf_state;
mod types;
mod utils;

pub use crate::chunk::data_format;
pub use crate::errors::chunker_error::{ChunkerError, ChunkerErrorEnum};
pub use crate::json_parser::parse_json;
pub use crate::rdf_parser::{parse_rdf, parse_rdfs};
pub use crate::types::input_format::InputFormat;
pub use crate::types::json_op::JsonOp;
pub use crate::types::nquad_buffer::NQuadBuffer;
