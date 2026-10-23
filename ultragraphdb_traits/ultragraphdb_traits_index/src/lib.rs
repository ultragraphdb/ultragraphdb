/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `tok/index` and `tok/options`, the vector index interfaces and the index options
//! they take: `IndexFactory`, `VectorIndex` with `OptionalIndexSupport` and `OptionalSearchOptions`, the store a
//! vector index reads and writes through (`CacheType`, `LocalCache`, `Txn`), and `Options` and `AllowedOptions`
//! with their values and parsers.
//!
//! ```
//! use ultragraphdb_traits_index::{AllowedOptions, OptionValue, Options};
//!
//! let mut allowed = AllowedOptions::new();
//! allowed.add_int_option("exponent");
//! let mut opts = Options::new();
//! opts.set_opt("exponent", allowed.get_parsed_option("exponent", "4").unwrap());
//! assert_eq!(opts.get_opt::<i64>("exponent"), Ok(Some(4)));
//! assert_eq!(opts.get_interface_opt("exponent"), Some(&OptionValue::Int(4)));
//! ```

mod traits;
mod types;
mod utils;

pub use crate::traits::cache_type::CacheType;
pub use crate::traits::float::Float;
pub use crate::traits::index_factory::IndexFactory;
pub use crate::traits::local_cache::LocalCache;
pub use crate::traits::option_type::OptionType;
pub use crate::traits::txn::Txn;
pub use crate::traits::vector_index::{OptionalIndexSupport, OptionalSearchOptions, VectorIndex};
pub use crate::types::allowed_options::AllowedOptions;
pub use crate::types::key_value::KeyValue;
pub use crate::types::metric::Metric;
pub use crate::types::option_parser::OptionParser;
pub use crate::types::option_value::OptionValue;
pub use crate::types::option_value_pair::OptionValuePair;
pub use crate::types::options::Options;
pub use crate::types::search_path_result::SearchPathResult;
pub use crate::types::vector_index_options::{
    SearchFilter, VectorIndexOptions, accept_all, accept_none,
};
pub use crate::utils::hnsw::{COSINE, DOT_PROD, EUCLIDEAN};
pub use crate::utils::option_parser::{
    float32_opt_parser, float64_opt_parser, int_opt_parser, string_opt_parser, uint_opt_parser,
};
