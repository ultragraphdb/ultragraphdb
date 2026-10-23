/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `dql`: the DQL query, mutation and upsert parser, and N-Quads to directed edges.
//!
//! [`parse`] turns a query into [`GraphQuery`] trees, one per block, with the variables each block defines and
//! uses; [`parse_mutation`] and [`parse_dql`] turn mutation and upsert blocks into API requests, their N-Quads
//! and conditions kept as text; [`NQuad::to_edge_using`] turns an N-Quad into a directed edge.
//!
//! ```
//! use ultragraphdb_core_dql::{DqlError, Request, parse};
//!
//! # fn main() -> Result<(), DqlError> {
//! let res = parse(&Request::new("{ me(func: uid(0x1)) { name@en:ru friends(first: 2) { name } } }"))?;
//! let me = &res.query()[0];
//! assert_eq!(me.alias(), "me");
//! assert_eq!(me.uid(), [1]);
//! assert_eq!(me.children()[0].langs(), ["en", "ru"]);
//! assert_eq!(me.children()[1].args()["first"], "2");
//! # Ok(())
//! # }
//! ```

mod dql;
mod errors;
mod math;
mod mutation;
mod parser;
mod parser_mutation;
mod state;
mod types;
mod utils;

pub use crate::dql::parse_dql;
pub use crate::errors::dql_error::{DqlError, DqlErrorEnum};
pub use crate::mutation::{delete_predicate_edge, parse_uid, type_val_from};
pub use crate::parser::function::is_inequality_fn;
pub use crate::parser::{parse, parse_with_need_vars};
pub use crate::parser_mutation::parse_mutation;
pub use crate::state::{LEX_QUERY, LEX_TOP_LEVEL};
pub use crate::types::arg::Arg;
pub use crate::types::facet_order::FacetOrder;
pub use crate::types::filter_tree::FilterTree;
pub use crate::types::function::Function;
pub use crate::types::graph_query::GraphQuery;
pub use crate::types::group_by_attr::GroupByAttr;
pub use crate::types::math_tree::MathTree;
pub use crate::types::mutation::Mutation;
pub use crate::types::nquad::NQuad;
pub use crate::types::object::Object;
pub use crate::types::parse_result::ParseResult;
pub use crate::types::recurse_args::RecurseArgs;
pub use crate::types::request::Request;
pub use crate::types::shortest_path_args::ShortestPathArgs;
pub use crate::types::var_context::VarContext;
pub use crate::types::var_type::VarType;
pub use crate::types::vars::Vars;
