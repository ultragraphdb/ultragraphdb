/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go, worker/mutation.go and worker/draft.go (the texts of their
 * errors), edgraph/server.go (`errIndexingInProgress`) and dgo v250's txn.go (`ErrAborted`). Changed for
 * ultragraphdb: translated to Rust; a text of its own where Dgraph asserts.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt::{Display, Formatter, Result};

use ultragraphdb_common_utils::go_strconv;

use super::{ApplyError, ApplyErrorEnum};

/// Go's `%v` of a byte slice: its bytes in decimal, space separated, in brackets.
fn go_bytes(b: &[u8]) -> String {
    let bytes: Vec<String> = b.iter().map(u8::to_string).collect();
    format!("[{}]", bytes.join(" "))
}

impl Display for ApplyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.0 {
            ApplyErrorEnum::Posting(error) => write!(f, "{error}"),
            ApplyErrorEnum::Schema(error) => write!(f, "{error}"),
            ApplyErrorEnum::Key(error) => write!(f, "{error}"),
            ApplyErrorEnum::Types(error) => write!(f, "{error}"),
            ApplyErrorEnum::Tok(error) => write!(f, "{error}"),
            ApplyErrorEnum::Hnsw(error) => write!(f, "{error}"),
            ApplyErrorEnum::Num(error) => write!(f, "{error}"),
            ApplyErrorEnum::Aborted => f.write_str("Transaction has been aborted. Please retry"),
            ApplyErrorEnum::NoSchema { attr } => {
                write!(f, "runMutation: Unable to find schema for {attr}")
            }
            ApplyErrorEnum::NeverReach => f.write_str("We should never reach here"),
            ApplyErrorEnum::EmptyPredicate {
                entity,
                value_id,
                value,
            } => write!(
                f,
                "Predicate cannot be empty for edge with subject: [{entity}], object: [{value_id}] and value: [{}]",
                go_bytes(value)
            ),
            ApplyErrorEnum::InvalidUid => f.write_str("invalid UID with value 0"),
            ApplyErrorEnum::NotIndexable { attr } => {
                write!(f, "Cannot index attribute {attr} of type object.")
            }
            ApplyErrorEnum::NotIndexed { attr } => write!(f, "Attribute {attr} is not indexed."),
            ApplyErrorEnum::TsTooOld { context } => write!(f, "{context}: Transaction is too old"),
            ApplyErrorEnum::LangDirective { attr, edge } => write!(
                f,
                "Attr: [{attr}] should have @lang directive in schema to mutate edge: [{edge}]"
            ),
            ApplyErrorEnum::UidIsScalar { attr, edge } => write!(
                f,
                "Input for predicate {} of type uid is scalar. Edge: {edge}",
                go_strconv::quote(attr)
            ),
            ApplyErrorEnum::ScalarIsUid { attr, edge } => write!(
                f,
                "Input for predicate {} of type scalar is uid. Edge: {edge}",
                go_strconv::quote(attr)
            ),
            ApplyErrorEnum::NotVector { attr, edge } => write!(
                f,
                "Input for predicate {} of type vector is not vector. Did you forget to add quotes before []?. \
                 Edge: {edge}",
                go_strconv::quote(attr)
            ),
            ApplyErrorEnum::VectorKeyword { edge } => write!(
                f,
                "Not allowed to insert mutations in vector index keys, edge: [{edge}]"
            ),
            ApplyErrorEnum::ConvertEdgeType { from, to } => write!(
                f,
                "failure to convert edge type: '{from}' to schema type: '{to}'"
            ),
            ApplyErrorEnum::UnknownTokenizer { name } => {
                write!(f, "Could not find valid tokenizer for {name}")
            }
            ApplyErrorEnum::NoPredicate => f.write_str("No predicate specified in schema mutation"),
            ApplyErrorEnum::InternalName { attr } => write!(
                f,
                "Cannot create user-defined predicate with internal name {attr}"
            ),
            ApplyErrorEnum::TokenizerRequired { schema } => write!(
                f,
                "Tokenizer must be specified while indexing a predicate: {schema}"
            ),
            ApplyErrorEnum::DirectiveRequired => {
                f.write_str("Directive must be SchemaUpdate_INDEX when a tokenizer is specified")
            }
            ApplyErrorEnum::IndexOnUid { attr } => write!(
                f,
                "Index not allowed on predicate of type uid on predicate {attr}"
            ),
            ApplyErrorEnum::ReverseNonUid { attr } => {
                write!(f, "Cannot reverse for non-uid type on predicate {attr}")
            }
            ApplyErrorEnum::UpsertNeedsIndex { attr } => write!(
                f,
                "Index tokenizer is mandatory for: [{attr}] when specifying @upsert directive"
            ),
            ApplyErrorEnum::DropUpsert { attr } => write!(
                f,
                "could not drop @upsert from [{attr}] predicate when @unique directive specified"
            ),
            ApplyErrorEnum::DropUniqueIndex { tokenizers, attr } => write!(
                f,
                "could not drop index [{}] from [{attr}] predicate when @unique directive specified",
                tokenizers.join(" ")
            ),
            ApplyErrorEnum::UniqueStringIndex { attr } => write!(
                f,
                "index for predicate [{attr}] is missing, add either hash or exact index with @unique"
            ),
            ApplyErrorEnum::UniqueIntIndex { attr } => write!(
                f,
                "index for predicate [{attr}] is missing, add int index with @unique"
            ),
            ApplyErrorEnum::UniqueUnsupported { value_type } => write!(
                f,
                "@unique directive not supported on [{value_type}] type predicate"
            ),
            ApplyErrorEnum::PasswordChange { from, to } => {
                write!(f, "Schema change not allowed from {from} to {to}")
            }
            ApplyErrorEnum::ListToScalar { from, to, attr } => write!(
                f,
                "Schema change not allowed from [{from}] => {to} without deleting pred: {attr}"
            ),
            ApplyErrorEnum::ScalarUid { attr } => write!(
                f,
                "Schema change not allowed from scalar to uid or vice versa while there is data for pred: {attr}"
            ),
            ApplyErrorEnum::ListToScalarEmpty { attr } => write!(
                f,
                "Type can't be changed from list to scalar for attr: [{attr}] without dropping it first."
            ),
            ApplyErrorEnum::VectorOption { factory, error } => {
                write!(f, "IndexFactory: {factory} issues this error: '{error}'")
            }
            ApplyErrorEnum::FieldName => f.write_str("Field in type definition must have a name"),
            ApplyErrorEnum::FieldObjectName => {
                f.write_str("Field with value type OBJECT must specify the name of the object type")
            }
            ApplyErrorEnum::FieldDirective => {
                f.write_str("Field in type definition cannot have a directive")
            }
            ApplyErrorEnum::FieldTokenizers => {
                f.write_str("Field in type definition cannot have tokenizers")
            }
            ApplyErrorEnum::StartTsRequired => f.write_str("StartTs must be provided"),
            ApplyErrorEnum::IndexingInProgress => {
                f.write_str("errIndexingInProgress. Please retry")
            }
            ApplyErrorEnum::RebuildNoSchema { attr } => {
                write!(f, "Rebuilder.run: Unable to find schema for {attr}")
            }
            // draft.go: errs = errors.Wrap(err, errs.Error()) for each error, after "Got error while running mutation".
            ApplyErrorEnum::Mutations { causes } => {
                f.write_str("Got error while running mutation")?;
                causes.iter().try_for_each(|c| write!(f, ": {c}"))
            }
            ApplyErrorEnum::RootNamespace => f.write_str("the root namespace cannot be deleted"),
        }
    }
}
