/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/mutation.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_types::{TypeId, Val, Value, bytes_as_float_array};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::value::Val as ApiVal;

use crate::errors::dql_error::DqlError;
use crate::utils::go_strconv::parse_uint;

pub(crate) const ERR_INVALID_UID: &str = "UID must to be greater than 0";

/// Parses a uid, `0x1f` or `31` (Go's base prefixes and underscores).
///
/// # Errors
///
/// Go's `strconv` error when the text is no uid, `UID must to be greater than 0` for zero.
pub fn parse_uid(xid: &str) -> Result<u64, DqlError> {
    match parse_uint(xid, 0) {
        Ok(0) => Err(DqlError::conversion(ERR_INVALID_UID)),
        Ok(uid) => Ok(uid),
        Err(e) => Err(DqlError::conversion(e.to_string())),
    }
}

/// The typed value of an API value, by the variant it holds; an empty string when it holds none (or a date or
/// uid, which have no type).
pub fn type_val_from(val: &api::Value) -> Val {
    match &val.val {
        Some(ApiVal::BytesVal(b)) => Val::new(TypeId::BINARY, Value::Bytes(b.clone())),
        Some(ApiVal::IntVal(i)) => Val::new(TypeId::INT, Value::Int(*i)),
        Some(ApiVal::StrVal(s)) => Val::new(TypeId::STRING, Value::str(s)),
        Some(ApiVal::BoolVal(b)) => Val::new(TypeId::BOOL, Value::Bool(*b)),
        Some(ApiVal::DoubleVal(d)) => Val::new(TypeId::FLOAT, Value::Float(*d)),
        Some(ApiVal::GeoVal(b)) => Val::new(TypeId::GEO, Value::Bytes(b.clone())),
        Some(ApiVal::BigfloatVal(b)) => Val::new(TypeId::BIGFLOAT, Value::Bytes(b.clone())),
        Some(ApiVal::DatetimeVal(b)) => Val::new(TypeId::DATETIME, Value::Bytes(b.clone())),
        Some(ApiVal::PasswordVal(s)) => Val::new(TypeId::PASSWORD, Value::str(s)),
        Some(ApiVal::Vfloat32Val(b)) => {
            Val::new(TypeId::VFLOAT, Value::VFloat(bytes_as_float_array(b)))
        }
        Some(ApiVal::DefaultVal(s)) => Val::new(TypeId::DEFAULT, Value::str(s)),
        Some(ApiVal::DateVal(_) | ApiVal::UidVal(_)) | None => {
            Val::new(TypeId::STRING, Value::str(""))
        }
    }
}
