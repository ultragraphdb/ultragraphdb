/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/scalar_types.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Dgraph's `TypeID`: the type of a value, stored in posting lists as `pb.Posting.ValType`. The identifiers can
/// never change; any `i32` is representable, as in Go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(i32);

impl TypeId {
    /// `DefaultID`.
    pub const DEFAULT: TypeId = TypeId(0);
    /// `BinaryID`.
    pub const BINARY: TypeId = TypeId(1);
    /// `IntID`.
    pub const INT: TypeId = TypeId(2);
    /// `FloatID`.
    pub const FLOAT: TypeId = TypeId(3);
    /// `BoolID`.
    pub const BOOL: TypeId = TypeId(4);
    /// `DateTimeID`.
    pub const DATETIME: TypeId = TypeId(5);
    /// `GeoID`.
    pub const GEO: TypeId = TypeId(6);
    /// `UidID`.
    pub const UID: TypeId = TypeId(7);
    /// `PasswordID`.
    pub const PASSWORD: TypeId = TypeId(8);
    /// `StringID`.
    pub const STRING: TypeId = TypeId(9);
    /// `pb.Posting_OBJECT`, which Dgraph's types package has no constant for.
    pub const OBJECT: TypeId = TypeId(10);
    /// `BigFloatID`.
    pub const BIGFLOAT: TypeId = TypeId(11);
    /// `VFloatID`: a vector of float32.
    pub const VFLOAT: TypeId = TypeId(12);
    /// `UndefinedID`.
    pub const UNDEFINED: TypeId = TypeId(100);

    /// A type identifier from its stored value.
    pub const fn new(id: i32) -> TypeId {
        TypeId(id)
    }

    /// The stored value, Go's `Enum()` as `pb.Posting.ValType`.
    pub const fn enum_value(self) -> i32 {
        self.0
    }

    /// Go's `Name`: the schema name, empty for an identifier without one.
    pub fn name(self) -> &'static str {
        match self.0 {
            0 => "default",
            1 => "binary",
            2 => "int",
            3 => "float",
            4 => "bool",
            5 => "datetime",
            6 => "geo",
            7 => "uid",
            8 => "password",
            9 => "string",
            11 => "bigfloat",
            12 => "float32vector",
            _ => "",
        }
    }

    /// Go's `TypeForName`.
    pub fn for_name(name: &str) -> Option<TypeId> {
        TYPE_NAMES.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
    }

    /// Go's `IsScalar`: every type but uid.
    pub fn is_scalar(self) -> bool {
        self != TypeId::UID
    }

    /// Go's `IsNumber`: int and float.
    pub fn is_number(self) -> bool {
        self == TypeId::INT || self == TypeId::FLOAT
    }
}

/// Dgraph's `typeNameMap`.
pub const TYPE_NAMES: [(&str, TypeId); 12] = [
    ("default", TypeId::DEFAULT),
    ("binary", TypeId::BINARY),
    ("int", TypeId::INT),
    ("float", TypeId::FLOAT),
    ("bool", TypeId::BOOL),
    ("datetime", TypeId::DATETIME),
    ("geo", TypeId::GEO),
    ("uid", TypeId::UID),
    ("string", TypeId::STRING),
    ("password", TypeId::PASSWORD),
    ("bigfloat", TypeId::BIGFLOAT),
    ("float32vector", TypeId::VFLOAT),
];
