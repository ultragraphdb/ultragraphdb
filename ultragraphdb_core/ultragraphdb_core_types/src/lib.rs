/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `types`: scalar type identifiers, conversions, binary encodings, ordering, passwords, geo values.

mod errors;
mod traits;
mod types;
mod utils;

pub use crate::errors::types_error::{TypesError, TypesErrorEnum};
pub use crate::types::civil_time::CivilTime;
pub use crate::types::date_time::{DateTime, Zone};
pub use crate::types::sharded_map::{NUM_SHARDS, ShardedMap};
pub use crate::types::text_collator::TextCollator;
pub use crate::types::type_id::{TYPE_NAMES, TypeId};
pub use crate::types::val::Val;
pub use crate::types::value::Value;
pub use crate::utils::compare::{compare_between, compare_vals};
pub use crate::utils::conversion::{
    convert, decimal_trunc_i64, marshal, marshal_binary, marshal_json, parse_vfloat,
};
pub use crate::utils::password::{encrypt, verify_password};
pub use crate::utils::scalar_types::{good_time_zone, parse_time, value_for_type};
pub use crate::utils::sort::{equal, is_sortable, less, sort, sort_top_n};
pub use crate::utils::value::{
    bytes_as_float_array, float_array_as_bytes, float_array_as_string, type_for_value,
};
