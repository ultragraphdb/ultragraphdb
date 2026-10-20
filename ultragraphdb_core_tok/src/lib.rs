/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `tok`, `tok/index`, `tok/options`, `tok/constraints`, and from `tok/hnsw` the
//! factory's registry entry: the index tokenizers, their registry, and the vector index interfaces and options.
//!
//! Text analysis is bleve v2.5.7's as Dgraph configures it, ported: blevesearch/segment's Unicode 8.0.0 word
//! segmenter, `to_lower`, NFKC, the stop-word lists of 28 languages, the stemmers of Dgraph's `langStemmers` (the
//! Snowball ones over `rust-stemmers`) and the CJK bigram filter.
//!
//! ```
//! use ultragraphdb_core_tok::{build_tokens, get_tokenizer, get_tokenizer_for_lang};
//! use ultragraphdb_core_types::Value;
//!
//! let fulltext = get_tokenizer_for_lang(&get_tokenizer("fulltext").unwrap(), "en");
//! let tokens = build_tokens(&Value::str("Stemming works!"), &fulltext).unwrap();
//! assert_eq!(tokens, vec![b"\x08stem".to_vec(), b"\x08work".to_vec()]);
//! ```

mod bleve;
mod errors;
mod tables;
mod traits;
mod types;
mod utils;

pub use crate::errors::options_error::{OptionsError, OptionsErrorEnum};
pub use crate::errors::tok_error::{TokError, TokErrorEnum};
pub use crate::traits::cache_type::CacheType;
pub use crate::traits::float::Float;
pub use crate::traits::index_factory::IndexFactory;
pub use crate::traits::local_cache::LocalCache;
pub use crate::traits::option_type::OptionType;
pub use crate::traits::txn::Txn;
pub use crate::traits::vector_index::{OptionalIndexSupport, OptionalSearchOptions, VectorIndex};
pub use crate::types::allowed_options::AllowedOptions;
pub use crate::types::analysis_token::AnalysisToken;
pub use crate::types::factory_create_spec::FactoryCreateSpec;
pub use crate::types::key_value::KeyValue;
pub use crate::types::metric::Metric;
pub use crate::types::option_parser::OptionParser;
pub use crate::types::option_value::OptionValue;
pub use crate::types::option_value_pair::OptionValuePair;
pub use crate::types::options::Options;
pub use crate::types::search_path_result::SearchPathResult;
pub use crate::types::segment_type::SegmentType;
pub use crate::types::stemmer::Stemmer;
pub use crate::types::token_type::TokenType;
pub use crate::types::tokenizer::Tokenizer;
pub use crate::types::vector_index_factory::VectorIndexFactory;
pub use crate::types::vector_index_options::{
    SearchFilter, VectorIndexOptions, accept_all, accept_none,
};
pub use crate::utils::bleve::analyze;
pub use crate::utils::hnsw::{
    COSINE, DOT_PROD, EF_CONSTRUCTION_OPT, EF_SEARCH_OPT, EMPTY_HNSW_TREE_ERROR, EUCLIDEAN,
    EXPONENT_OPT, HNSW, MAX_LEVELS_OPT, METRIC_OPT, VEC_DEAD, VEC_ENTRY, VEC_KEYWORD,
};
pub use crate::utils::ident::{
    IDENT_BIG_FLOAT, IDENT_BOOL, IDENT_CUSTOM, IDENT_DAY, IDENT_DELIMITER, IDENT_EXACT,
    IDENT_EXACT_LANG, IDENT_FLOAT, IDENT_FULL_TEXT, IDENT_GEO, IDENT_HASH, IDENT_HOUR, IDENT_INT,
    IDENT_MONTH, IDENT_NGRAM, IDENT_NONE, IDENT_SHA, IDENT_TERM, IDENT_TRIGRAM, IDENT_VFLOAT,
    IDENT_YEAR,
};
pub use crate::utils::index_factory::{
    get_factory_create_spec_from_spec, get_index_factories_from_specs, get_index_factory,
    get_index_factory_from_spec, get_index_factory_opts_from_spec,
};
pub use crate::utils::index_helper::{bytes_as_float_array, bytes_to_float};
pub use crate::utils::langbase::lang_base;
pub use crate::utils::option_parser::{
    float32_opt_parser, float64_opt_parser, int_opt_parser, string_opt_parser, uint_opt_parser,
};
pub use crate::utils::segment::word_segments;
pub use crate::utils::stemmers::filter_stemmers;
pub use crate::utils::stopwords::{filter_stopwords, stop_words};
pub use crate::utils::tok::{
    TOKENIZER_NAMES, build_ngram_query_tokens, build_tokens, encode_geo_tokens,
    encode_regex_tokens, get_tokenizer, get_tokenizer_by_id, get_tokenizer_for_lang,
    get_tokenizers,
};
pub use crate::utils::tokens::{
    get_full_text_tokens, get_ngram_query_tokens, get_ngram_tokens, get_term_tokens, get_tokens,
};
