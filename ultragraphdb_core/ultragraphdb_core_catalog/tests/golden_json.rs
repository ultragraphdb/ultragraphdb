/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The golden files of tools/golden/schema.go and the comparison of parse results with them.

use serde::Deserialize;
use ultragraphdb_core_catalog::{ParsedSchema, SchemaError};
use ultragraphdb_proto_pb::pb;
use ultragraphdb_test_bdd::Golden;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OptionJson {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SpecJson {
    pub name: String,
    pub options: Vec<OptionJson>,
}

/// A `pb.SchemaUpdate` as the generator writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpdateJson {
    pub predicate: String,
    pub value_type: i32,
    pub directive: i32,
    pub tokenizer: Vec<String>,
    pub count: bool,
    pub list: bool,
    pub upsert: bool,
    pub lang: bool,
    pub unique: bool,
    pub non_nullable: bool,
    pub non_nullable_list: bool,
    pub object_type_name: String,
    pub no_conflict: bool,
    pub index_specs: Vec<SpecJson>,
}

/// A `pb.TypeUpdate` as the generator writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TypeJson {
    pub type_name: String,
    pub fields: Vec<UpdateJson>,
}

#[derive(Debug, Deserialize)]
pub struct Outcome {
    pub preds: Option<Vec<UpdateJson>>,
    pub types: Option<Vec<TypeJson>>,
    pub err: String,
    pub is_err: bool,
}

#[derive(Debug, Deserialize)]
pub struct ParseCase {
    pub input: String,
    pub parse: Outcome,
    pub ns5: Outcome,
}

#[derive(Debug, Deserialize)]
pub struct InitialCase {
    pub namespace: u64,
    pub acl_enabled: bool,
    pub schema: Vec<UpdateJson>,
    pub types: Vec<TypeJson>,
    pub complete_schema: Vec<UpdateJson>,
    pub complete_types: Vec<TypeJson>,
}

#[derive(Debug, Deserialize)]
pub struct PredefCase {
    pub input: String,
    pub namespace: u64,
    pub pred_changed: Vec<bool>,
    pub preds_after: Vec<UpdateJson>,
    pub type_changed: Vec<bool>,
}

fn load<T: serde::de::DeserializeOwned>(name: &str) -> Golden<T> {
    let path = ultragraphdb_test_bdd::testdata_dir().join(format!("golden/{name}.json"));
    ultragraphdb_test_bdd::load_golden::<T>(&path)
}

pub fn parse_cases() -> Golden<ParseCase> {
    load("schema_parse")
}

pub fn initial_cases() -> Golden<InitialCase> {
    load("schema_initial")
}

pub fn predef_cases() -> Golden<PredefCase> {
    load("schema_predefined")
}

pub fn update_json(u: &pb::SchemaUpdate) -> UpdateJson {
    UpdateJson {
        predicate: u.predicate.clone(),
        value_type: u.value_type,
        directive: u.directive,
        tokenizer: u.tokenizer.clone(),
        count: u.count,
        list: u.list,
        upsert: u.upsert,
        lang: u.lang,
        unique: u.unique,
        non_nullable: u.non_nullable,
        non_nullable_list: u.non_nullable_list,
        object_type_name: u.object_type_name.clone(),
        no_conflict: u.no_conflict,
        index_specs: u
            .index_specs
            .iter()
            .map(|s| SpecJson {
                name: s.name.clone(),
                options: s
                    .options
                    .iter()
                    .map(|o| OptionJson {
                        key: o.key.clone(),
                        value: o.value.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub fn type_json(t: &pb::TypeUpdate) -> TypeJson {
    TypeJson {
        type_name: t.type_name.clone(),
        fields: t.fields.iter().map(update_json).collect(),
    }
}

pub fn compare_updates(got: &[pb::SchemaUpdate], want: &[UpdateJson]) -> Result<(), String> {
    let got: Vec<UpdateJson> = got.iter().map(update_json).collect();
    if got == want {
        return Ok(());
    }
    Err(format!("predicates\n  got  {got:?}\n  want {want:?}"))
}

pub fn compare_types(got: &[pb::TypeUpdate], want: &[TypeJson]) -> Result<(), String> {
    let got: Vec<TypeJson> = got.iter().map(type_json).collect();
    if got == want {
        return Ok(());
    }
    Err(format!("types\n  got  {got:?}\n  want {want:?}"))
}

/// Whether a parse gave what Dgraph's gave: the same updates, or an error with the same text.
pub fn compare_outcome(
    got: &Result<ParsedSchema, SchemaError>,
    want: &Outcome,
) -> Result<(), String> {
    match got {
        Err(e) if want.is_err => {
            if e.to_string() == want.err {
                Ok(())
            } else {
                Err(format!("error\n  got  {e}\n  want {}", want.err))
            }
        }
        Err(e) => Err(format!("unexpected error {e}")),
        Ok(res) if want.is_err => Err(format!("expected error {:?}, got {res:?}", want.err)),
        Ok(res) => {
            compare_updates(res.preds(), want.preds.as_deref().unwrap_or_default())?;
            compare_types(res.types(), want.types.as_deref().unwrap_or_default())
        }
    }
}
