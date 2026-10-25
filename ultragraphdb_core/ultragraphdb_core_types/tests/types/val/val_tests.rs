/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Dgraph's `Val` and `Safe` (types/scalar_types.go).

use ultragraphdb_core_types::{
    BIG_FLOAT_PRECISION, BigFloat, DateTime, Geometry, TypeId, Val, Value,
};

#[test]
fn test_constructors_and_accessors() {
    let v = Val::new(TypeId::INT, Value::Int(-7));
    assert_eq!(v.tid(), TypeId::INT);
    assert_eq!(v.value(), Some(&Value::Int(-7)));
    assert_eq!(v.clone().into_value(), Some(Value::Int(-7)));

    let b = Val::bytes(TypeId::STRING, "日本");
    assert_eq!(b.tid(), TypeId::STRING);
    assert_eq!(b.value(), Some(&Value::Bytes("日本".as_bytes().to_vec())));
    assert_eq!(
        Val::bytes(TypeId::BINARY, Vec::new()).into_value(),
        Some(Value::Bytes(Vec::new()))
    );

    let n = Val::nil(TypeId::FLOAT);
    assert_eq!(n.tid(), TypeId::FLOAT);
    assert_eq!(n.value(), None);
    assert_eq!(n.into_value(), None);
    assert_ne!(Val::nil(TypeId::INT), Val::new(TypeId::INT, Value::Int(0)));
}

#[test]
fn test_safe_keeps_a_value() {
    for v in [
        Val::new(TypeId::INT, Value::Int(3)),
        Val::new(TypeId::STRING, Value::str("x")),
        Val::bytes(TypeId::DATETIME, vec![1, 2]),
        // A value whose type is not its tid's is kept as it is.
        Val::new(TypeId::BOOL, Value::Uid(9)),
    ] {
        assert_eq!(v.safe().as_ref(), v.value());
    }
}

#[test]
fn test_safe_gives_the_zero_value_of_a_nil() {
    // Go's ValueForType: the zero value of each type, nil for one without.
    let cases = [
        (TypeId::BINARY, Some(Value::Bytes(Vec::new()))),
        (TypeId::INT, Some(Value::Int(0))),
        (TypeId::FLOAT, Some(Value::Float(0.0))),
        (TypeId::BOOL, Some(Value::Bool(false))),
        (TypeId::DATETIME, Some(Value::DateTime(DateTime::zero()))),
        (
            TypeId::BIGFLOAT,
            Some(Value::BigFloat(BigFloat::zero(BIG_FLOAT_PRECISION))),
        ),
        (TypeId::STRING, Some(Value::str(""))),
        (TypeId::DEFAULT, Some(Value::str(""))),
        (TypeId::PASSWORD, Some(Value::str(""))),
        (
            TypeId::GEO,
            Some(Value::Geo(Geometry::collection(Vec::new()))),
        ),
        (TypeId::UID, Some(Value::Uid(0))),
        (TypeId::VFLOAT, Some(Value::VFloat(Vec::new()))),
        (TypeId::OBJECT, None),
        (TypeId::UNDEFINED, None),
        (TypeId::new(-1), None),
    ];
    for (tid, want) in cases {
        assert_eq!(Val::nil(tid).safe(), want, "{tid:?}");
    }
}
