/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, worker/mutation_unit_test.go (TestConvertEdgeType, TestValidateEdgeTypeError,
 * TestTypeSanityCheck). Changed for ultragraphdb: translated to Rust; the errors' texts compared whole; added: the
 * refusals of ValidateAndConvert Go's tests leave out.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::{attr_in_root_namespace, namespace_attr};
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb::schema_update::Directive;
use ultragraphdb_proto_pb::pb::{DirectedEdge, SchemaUpdate, TypeUpdate, directed_edge, posting};
use ultragraphdb_store_apply::{type_sanity_check, validate_and_convert};

fn schema_of(to: TypeId) -> SchemaUpdate {
    SchemaUpdate {
        value_type: to.enum_value(),
        ..SchemaUpdate::default()
    }
}

#[test]
fn test_convert_edge_type() {
    let set_edge = || DirectedEdge {
        value: b"set edge".to_vec(),
        attr: attr_in_root_namespace("name"),
        ..DirectedEdge::default()
    };
    // A string is stored as a string.
    let mut edge = set_edge();
    validate_and_convert(&mut edge, &schema_of(TypeId::STRING)).unwrap();
    assert_eq!(
        edge,
        DirectedEdge {
            value_type: 9,
            ..set_edge()
        }
    );
    // A delete in another namespace alike.
    let del = || DirectedEdge {
        attr: namespace_attr(0xf2, "name"),
        op: directed_edge::Op::Del as i32,
        ..set_edge()
    };
    let mut edge = del();
    validate_and_convert(&mut edge, &schema_of(TypeId::STRING)).unwrap();
    assert_eq!(
        edge,
        DirectedEdge {
            value_type: 9,
            ..del()
        }
    );
    // A uid for a string predicate, and a value for a uid predicate, are refused.
    let mut uid = DirectedEdge {
        value_id: 123,
        attr: attr_in_root_namespace("name"),
        ..DirectedEdge::default()
    };
    assert!(validate_and_convert(&mut uid, &schema_of(TypeId::STRING)).is_err());
    let mut value = set_edge();
    assert!(validate_and_convert(&mut value, &schema_of(TypeId::UID)).is_err());
}

#[test]
fn test_validate_edge_type_error() {
    let mut edge = DirectedEdge {
        value: b"set edge".to_vec(),
        attr: attr_in_root_namespace("name"),
        ..DirectedEdge::default()
    };
    assert!(validate_and_convert(&mut edge, &schema_of(TypeId::DATETIME)).is_err());
}

#[test]
fn test_type_sanity_check() {
    let field = |su: SchemaUpdate| TypeUpdate {
        fields: vec![su],
        ..TypeUpdate::default()
    };
    let named = || SchemaUpdate {
        predicate: attr_in_root_namespace("name"),
        ..SchemaUpdate::default()
    };
    let err = type_sanity_check(&field(SchemaUpdate {
        predicate: attr_in_root_namespace(""),
        ..SchemaUpdate::default()
    }))
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("Field in type definition must have a name")
    );
    let err = type_sanity_check(&field(SchemaUpdate {
        value_type: posting::ValType::Object as i32,
        ..named()
    }))
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("Field with value type OBJECT must specify the name")
    );
    let err = type_sanity_check(&field(SchemaUpdate {
        directive: Directive::Reverse as i32,
        ..named()
    }))
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("Field in type definition cannot have a directive")
    );
    let err = type_sanity_check(&field(SchemaUpdate {
        tokenizer: vec!["int".into()],
        ..named()
    }))
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("Field in type definition cannot have tokenizers")
    );
    assert!(type_sanity_check(&field(named())).is_ok());
}

/// The value the spec converts and the one it refuses: `"12"` becomes the int 12, `"twelve"` is refused with Go's
/// `strconv` text.
#[test]
fn test_an_untyped_value_is_converted_to_the_schema_type_or_refused() {
    let edge = |v: &str| DirectedEdge {
        value: v.as_bytes().to_vec(),
        attr: attr_in_root_namespace("age"),
        entity: 1,
        ..DirectedEdge::default()
    };
    let mut twelve = edge("12");
    validate_and_convert(&mut twelve, &schema_of(TypeId::INT)).unwrap();
    assert_eq!(twelve.value_type, TypeId::INT.enum_value());
    assert_eq!(twelve.value, 12i64.to_le_bytes());
    let err = validate_and_convert(&mut edge("twelve"), &schema_of(TypeId::INT)).unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"strconv.ParseInt: parsing "twelve": invalid syntax"#
    );
}

/// The refusals Go's tests leave out: a language without `@lang`, a write to the vector index's predicates, a
/// non-vector for a vector predicate; a star delete and a predicate drop pass unconverted.
#[test]
fn test_validate_and_convert_refusals() {
    let attr = attr_in_root_namespace("name");
    let mut lang = DirectedEdge {
        value: b"x".to_vec(),
        attr: attr.clone(),
        lang: "en".into(),
        ..DirectedEdge::default()
    };
    let err = validate_and_convert(&mut lang, &schema_of(TypeId::STRING)).unwrap_err();
    assert!(
        err.to_string()
            .starts_with("Attr: [name] should have @lang directive in schema to mutate edge: ["),
        "{err}"
    );
    let vector_key = SchemaUpdate {
        predicate: attr_in_root_namespace("emb__vector_"),
        ..schema_of(TypeId::STRING)
    };
    let mut edge = DirectedEdge {
        value: b"x".to_vec(),
        attr: attr_in_root_namespace("emb__vector_"),
        ..DirectedEdge::default()
    };
    assert!(
        validate_and_convert(&mut edge, &vector_key)
            .unwrap_err()
            .to_string()
            .starts_with("Not allowed to insert mutations in vector index keys")
    );
    let mut int = DirectedEdge {
        value: 5i64.to_le_bytes().to_vec(),
        value_type: posting::ValType::Int as i32,
        attr: attr_in_root_namespace("emb"),
        ..DirectedEdge::default()
    };
    assert!(
        validate_and_convert(&mut int, &schema_of(TypeId::VFLOAT))
            .unwrap_err()
            .to_string()
            .starts_with(r#"Input for predicate "emb" of type vector is not vector."#)
    );
    let star = || DirectedEdge {
        value: b"_STAR_ALL".to_vec(),
        attr: attr.clone(),
        entity: 1,
        op: directed_edge::Op::Del as i32,
        ..DirectedEdge::default()
    };
    let mut s = star();
    validate_and_convert(&mut s, &schema_of(TypeId::INT)).unwrap();
    assert_eq!(s, star());
}
