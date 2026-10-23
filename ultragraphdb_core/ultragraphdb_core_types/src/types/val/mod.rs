/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/scalar_types.go (Val, Safe). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::type_id::TypeId;
use crate::types::value::Value;
use crate::utils::scalar_types::value_for_type;

/// Dgraph's `Val`: a value with its type. A value of `None` is Go's nil `Value`, e.g. a default value variable.
#[derive(Debug, Clone, PartialEq)]
pub struct Val {
    tid: TypeId,
    value: Option<Value>,
}

impl Val {
    /// A typed value.
    pub fn new(tid: TypeId, value: Value) -> Val {
        Val {
            tid,
            value: Some(value),
        }
    }

    /// A value of a type whose `Value` is nil.
    pub fn nil(tid: TypeId) -> Val {
        Val { tid, value: None }
    }

    /// The unconverted bytes of a type, the form `Convert` takes.
    pub fn bytes(tid: TypeId, bytes: impl Into<Vec<u8>>) -> Val {
        Val::new(tid, Value::Bytes(bytes.into()))
    }

    /// The type.
    pub fn tid(&self) -> TypeId {
        self.tid
    }

    /// The value, `None` when Go's is nil.
    pub fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// The value, consuming the `Val`.
    pub fn into_value(self) -> Option<Value> {
        self.value
    }

    /// Go's `Safe`: the value, or the zero value of the type when it is nil.
    pub fn safe(&self) -> Option<Value> {
        match &self.value {
            Some(v) => Some(v.clone()),
            None => value_for_type(self.tid).into_value(),
        }
    }
}
