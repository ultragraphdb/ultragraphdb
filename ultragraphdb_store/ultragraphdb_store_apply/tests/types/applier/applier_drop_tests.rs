/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The drops besides the scenarios': a type's, a namespace's deletion, the drop values refused, and the proof that
//! every drop goes through the posting lists' drop, which advances the cache's epoch, never the store's directly.

use ultragraphdb_core_keys::{data_prefix, namespace_attr, type_key};
use ultragraphdb_core_catalog::SchemaMode;
use ultragraphdb_proto_pb::pb::{Mutations, SchemaUpdate, TypeUpdate, mutations::DropOp};
use ultragraphdb_store_sim::OpRecord;

use crate::utils::helpers::{
    TestApplier, alter_now, attr, dkey, mutate, new_applier, set_str, stored_keys, uids,
};

fn person(ns: u64) -> TypeUpdate {
    TypeUpdate {
        type_name: namespace_attr(ns, "Person"),
        fields: vec![SchemaUpdate {
            predicate: namespace_attr(ns, "name"),
            ..SchemaUpdate::default()
        }],
    }
}

fn drop(
    a: &std::sync::Arc<TestApplier>,
    op: DropOp,
    value: &str,
) -> Result<u64, ultragraphdb_store_apply::ApplyError> {
    a.apply_mutations(&Mutations {
        start_ts: a.lists().latest_ts(),
        drop_op: op as i32,
        drop_value: value.to_string(),
        ..Mutations::default()
    })
}

#[test]
fn test_a_type_is_dropped_with_its_key() {
    let a = new_applier();
    a.update_types(vec![person(0)]).unwrap();
    assert!(!stored_keys(&a, &type_key(&namespace_attr(0, "Person")).unwrap()).is_empty());
    drop(&a, DropOp::Type, &namespace_attr(0, "Person")).unwrap();
    assert!(a.schema().get_type(&namespace_attr(0, "Person")).is_none());
    assert!(stored_keys(&a, &type_key(&namespace_attr(0, "Person")).unwrap()).is_empty());
}

#[test]
fn test_a_type_without_field_names_is_refused() {
    let a = new_applier();
    let mut t = person(0);
    t.fields[0].predicate = namespace_attr(0, "");
    assert_eq!(
        a.update_types(vec![t]).unwrap_err().to_string(),
        "Field in type definition must have a name"
    );
}

/// Deleting a namespace bans it: its keys are no longer served and later writes to it are dropped. The root
/// namespace is refused.
#[test]
fn test_a_deleted_namespace_is_banned() {
    let a = new_applier();
    let mut ups = crate::utils::helpers::updates("name: string .");
    ups[0].predicate = namespace_attr(5, "name");
    a.alter(ups).unwrap();
    let edge = ultragraphdb_proto_pb::pb::DirectedEdge {
        attr: namespace_attr(5, "name"),
        ..set_str("name", 1, "A")
    };
    mutate(&a, vec![edge.clone()]).unwrap();
    let key = ultragraphdb_core_keys::data_key(&namespace_attr(5, "name"), 1).unwrap();
    assert_eq!(uids(&a, &key), vec![u64::MAX]);
    a.delete_namespace(5).unwrap();
    assert!(
        a.schema()
            .get(SchemaMode::Query, &namespace_attr(5, "name"))
            .is_none()
    );
    assert!(uids(&a, &key).is_empty());
    assert_eq!(
        a.delete_namespace(0).unwrap_err().to_string(),
        "the root namespace cannot be deleted"
    );
}

#[test]
fn test_a_drop_value_that_is_no_namespace_is_refused() {
    let a = new_applier();
    assert_eq!(
        drop(&a, DropOp::Data, "three").unwrap_err().to_string(),
        r#"strconv.ParseUint: parsing "three": invalid syntax"#
    );
    let err = a
        .apply_mutations(&Mutations::default())
        .unwrap_err()
        .to_string();
    assert_eq!(err, "StartTs must be provided");
}

/// Every drop of the applier is the posting lists' drop: the cache's epoch advances with it, and the store's drop
/// is logged at the timestamp the applier returns. A drop made on the store directly would leave the epoch, and
/// rollups and cache fills read before it would write the dropped lists back (ledger 38).
#[test]
fn test_every_drop_goes_through_the_posting_lists() {
    let a = new_applier();
    alter_now(&a, "name: string @index(exact) .");
    type Drop = fn(&std::sync::Arc<TestApplier>) -> u64;
    let ops: [(&str, Drop); 4] = [
        ("predicate", |a| a.drop_predicate(&attr("name")).unwrap()),
        ("data", |a| drop(a, DropOp::Data, "0").unwrap()),
        ("all in namespace", |a| {
            drop(a, DropOp::AllInNs, "0").unwrap()
        }),
        ("all", |a| drop(a, DropOp::All, "").unwrap()),
    ];
    for (name, op) in ops {
        alter_now(&a, "name: string @index(exact) .");
        mutate(&a, vec![set_str("name", 1, "A")]).unwrap();
        a.lists().store().take_log();
        let epoch = a.lists().epoch();
        let ts = op(&a);
        assert_eq!(a.lists().epoch(), epoch + 1, "{name}");
        let dropped: Vec<u64> = a
            .lists()
            .store()
            .take_log()
            .into_iter()
            .filter_map(|r| match r {
                OpRecord::Dropped { ts, .. } => Some(ts),
                _ => None,
            })
            .collect();
        if name == "all" {
            assert_eq!(dropped.len(), 1, "{name}");
        } else {
            assert_eq!(dropped, vec![ts], "{name}");
        }
        assert!(uids(&a, &dkey("name", 1)).is_empty(), "{name}");
        assert!(
            stored_keys(&a, &data_prefix(0)).is_empty() || name == "predicate",
            "{name}"
        );
    }
}
