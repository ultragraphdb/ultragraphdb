/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go, worker/mutation.go and worker/draft.go (the errors of index
 * maintenance, runMutation, ValidateAndConvert, checkSchema, validateSchemaForUnique, typeSanityCheck,
 * applyMutations and the drops). Changed for ultragraphdb: translated to Rust; Go's wrapped errors as variants
 * holding their cause; the cases where Dgraph panics or exits as variants of their own; the abort of a request
 * that read before a drop or schema change it writes under, which Dgraph has no request to fail for.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod apply_error_display;
mod apply_error_error;
mod apply_error_from;

use ultragraphdb_core_keys::KeyError;
use ultragraphdb_core_catalog::SchemaError;
use ultragraphdb_core_tok::TokError;
use ultragraphdb_core_types::TypesError;
use ultragraphdb_error_num::NumError;
use ultragraphdb_store_hnsw::HnswError;
use ultragraphdb_store_posting::PostingError;

/// Why applying edges, a schema change or a drop failed. `Display` gives Dgraph's text wherever Dgraph has one.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyError(pub ApplyErrorEnum);

/// The classification of an [`ApplyError`].
#[derive(Debug, Clone, PartialEq)]
pub enum ApplyErrorEnum {
    /// A posting-list read or write failed.
    Posting(PostingError),
    /// The schema state refused or failed an operation.
    Schema(SchemaError),
    /// A key could not be built or parsed.
    Key(KeyError),
    /// A value could not be converted to the schema's type.
    Types(TypesError),
    /// A tokenizer or vector index specification failed.
    Tok(TokError),
    /// The vector index failed.
    Hnsw(HnswError),
    /// A drop's namespace did not parse.
    Num(NumError),
    /// A request that read below a drop or schema change it writes under, refused at commit: dgo's `ErrAborted`
    /// text, which a client sees for an aborted transaction.
    Aborted,
    /// A write to a predicate without a schema: `runMutation`'s refusal.
    NoSchema { attr: String },
    /// A predicate drop reaching `runMutation`, which Dgraph returns as an error.
    NeverReach,
    /// An edge without a predicate.
    EmptyPredicate {
        entity: u64,
        value_id: u64,
        value: Vec<u8>,
    },
    /// An index edge for uid 0.
    InvalidUid,
    /// A value of a predicate whose type is not scalar, indexed.
    NotIndexable { attr: String },
    /// A value indexed for a predicate without an index.
    NotIndexed { attr: String },
    /// A count read that could not be made: Dgraph's `ErrTsTooOld`, wrapped with where it was read.
    TsTooOld { context: &'static str },
    /// A value with a language for a predicate without `@lang`.
    LangDirective { attr: String, edge: String },
    /// A scalar for a uid predicate.
    UidIsScalar { attr: String, edge: String },
    /// A uid for a scalar predicate.
    ScalarIsUid { attr: String, edge: String },
    /// A value for a vector predicate that is not a vector.
    NotVector { attr: String, edge: String },
    /// A write to one of the predicates a vector index keeps.
    VectorKeyword { edge: String },
    /// A converted value that is not bytes.
    ConvertEdgeType { from: String, to: String },
    /// A tokenizer the registry does not hold.
    UnknownTokenizer { name: String },
    /// A schema update without a predicate.
    NoPredicate,
    /// A schema update for an internal predicate.
    InternalName { attr: String },
    /// An index directive without a tokenizer, naming the update as Go's `%+v` prints it.
    TokenizerRequired { schema: String },
    /// A tokenizer without the index directive.
    DirectiveRequired,
    /// An index on a uid predicate.
    IndexOnUid { attr: String },
    /// `@reverse` on a scalar predicate.
    ReverseNonUid { attr: String },
    /// `@upsert` without an index.
    UpsertNeedsIndex { attr: String },
    /// `@upsert` dropped from a `@unique` predicate.
    DropUpsert { attr: String },
    /// The index of a `@unique` predicate dropped.
    DropUniqueIndex {
        tokenizers: Vec<String>,
        attr: String,
    },
    /// A `@unique` string predicate without a hash or exact index.
    UniqueStringIndex { attr: String },
    /// A `@unique` int predicate without an int index.
    UniqueIntIndex { attr: String },
    /// `@unique` on a type it does not support, named as Go prints the protobuf enum.
    UniqueUnsupported { value_type: String },
    /// A change to or from `password`.
    PasswordChange { from: String, to: String },
    /// A list predicate holding data changed to a scalar.
    ListToScalar {
        from: String,
        to: String,
        attr: String,
    },
    /// A uid predicate changed to a scalar, or the reverse, while it holds data.
    ScalarUid { attr: String },
    /// A list predicate holding no data changed to a scalar.
    ListToScalarEmpty { attr: String },
    /// A vector index option its factory refuses, at the schema change; Go accepts an option below 1 and panics at
    /// the first search.
    VectorOption { factory: String, error: String },
    /// A type field without a name.
    FieldName,
    /// An object field without its object type's name.
    FieldObjectName,
    /// A type field with a directive.
    FieldDirective,
    /// A type field with tokenizers.
    FieldTokenizers,
    /// Mutations without a start timestamp.
    StartTsRequired,
    /// A schema change while an index is built: edgraph's `errIndexingInProgress`.
    IndexingInProgress,
    /// A rebuild of a predicate that lost its schema.
    RebuildNoSchema { attr: String },
    /// Edges applied apart that failed, each cause in order: Dgraph's wrapped `Got error while running mutation`.
    Mutations { causes: Vec<ApplyError> },
    /// A deletion of the root namespace, which Dgraph asserts never happens.
    RootNamespace,
}

impl ApplyError {
    #[allow(non_snake_case)]
    pub fn Posting(error: PostingError) -> Self {
        Self(ApplyErrorEnum::Posting(error))
    }

    #[allow(non_snake_case)]
    pub fn Schema(error: SchemaError) -> Self {
        Self(ApplyErrorEnum::Schema(error))
    }

    #[allow(non_snake_case)]
    pub fn Key(error: KeyError) -> Self {
        Self(ApplyErrorEnum::Key(error))
    }

    #[allow(non_snake_case)]
    pub fn Types(error: TypesError) -> Self {
        Self(ApplyErrorEnum::Types(error))
    }

    #[allow(non_snake_case)]
    pub fn Tok(error: TokError) -> Self {
        Self(ApplyErrorEnum::Tok(error))
    }

    #[allow(non_snake_case)]
    pub fn Hnsw(error: HnswError) -> Self {
        Self(ApplyErrorEnum::Hnsw(error))
    }

    #[allow(non_snake_case)]
    pub fn Num(error: NumError) -> Self {
        Self(ApplyErrorEnum::Num(error))
    }

    #[allow(non_snake_case)]
    pub fn Aborted() -> Self {
        Self(ApplyErrorEnum::Aborted)
    }

    #[allow(non_snake_case)]
    pub fn NoSchema(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::NoSchema { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn NeverReach() -> Self {
        Self(ApplyErrorEnum::NeverReach)
    }

    #[allow(non_snake_case)]
    pub fn EmptyPredicate(entity: u64, value_id: u64, value: Vec<u8>) -> Self {
        Self(ApplyErrorEnum::EmptyPredicate {
            entity,
            value_id,
            value,
        })
    }

    #[allow(non_snake_case)]
    pub fn InvalidUid() -> Self {
        Self(ApplyErrorEnum::InvalidUid)
    }

    #[allow(non_snake_case)]
    pub fn NotIndexable(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::NotIndexable { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn NotIndexed(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::NotIndexed { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn TsTooOld(context: &'static str) -> Self {
        Self(ApplyErrorEnum::TsTooOld { context })
    }

    #[allow(non_snake_case)]
    pub fn LangDirective(attr: impl Into<String>, edge: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::LangDirective {
            attr: attr.into(),
            edge: edge.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn UidIsScalar(attr: impl Into<String>, edge: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::UidIsScalar {
            attr: attr.into(),
            edge: edge.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn ScalarIsUid(attr: impl Into<String>, edge: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::ScalarIsUid {
            attr: attr.into(),
            edge: edge.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn NotVector(attr: impl Into<String>, edge: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::NotVector {
            attr: attr.into(),
            edge: edge.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn VectorKeyword(edge: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::VectorKeyword { edge: edge.into() })
    }

    #[allow(non_snake_case)]
    pub fn ConvertEdgeType(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::ConvertEdgeType {
            from: from.into(),
            to: to.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn UnknownTokenizer(name: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::UnknownTokenizer { name: name.into() })
    }

    #[allow(non_snake_case)]
    pub fn NoPredicate() -> Self {
        Self(ApplyErrorEnum::NoPredicate)
    }

    #[allow(non_snake_case)]
    pub fn InternalName(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::InternalName { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn TokenizerRequired(schema: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::TokenizerRequired {
            schema: schema.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn DirectiveRequired() -> Self {
        Self(ApplyErrorEnum::DirectiveRequired)
    }

    #[allow(non_snake_case)]
    pub fn IndexOnUid(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::IndexOnUid { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn ReverseNonUid(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::ReverseNonUid { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn UpsertNeedsIndex(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::UpsertNeedsIndex { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn DropUpsert(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::DropUpsert { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn DropUniqueIndex(tokenizers: Vec<String>, attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::DropUniqueIndex {
            tokenizers,
            attr: attr.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn UniqueStringIndex(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::UniqueStringIndex { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn UniqueIntIndex(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::UniqueIntIndex { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn UniqueUnsupported(value_type: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::UniqueUnsupported {
            value_type: value_type.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn PasswordChange(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::PasswordChange {
            from: from.into(),
            to: to.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn ListToScalar(
        from: impl Into<String>,
        to: impl Into<String>,
        attr: impl Into<String>,
    ) -> Self {
        Self(ApplyErrorEnum::ListToScalar {
            from: from.into(),
            to: to.into(),
            attr: attr.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn ScalarUid(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::ScalarUid { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn ListToScalarEmpty(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::ListToScalarEmpty { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn VectorOption(factory: impl Into<String>, error: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::VectorOption {
            factory: factory.into(),
            error: error.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn FieldName() -> Self {
        Self(ApplyErrorEnum::FieldName)
    }

    #[allow(non_snake_case)]
    pub fn FieldObjectName() -> Self {
        Self(ApplyErrorEnum::FieldObjectName)
    }

    #[allow(non_snake_case)]
    pub fn FieldDirective() -> Self {
        Self(ApplyErrorEnum::FieldDirective)
    }

    #[allow(non_snake_case)]
    pub fn FieldTokenizers() -> Self {
        Self(ApplyErrorEnum::FieldTokenizers)
    }

    #[allow(non_snake_case)]
    pub fn StartTsRequired() -> Self {
        Self(ApplyErrorEnum::StartTsRequired)
    }

    #[allow(non_snake_case)]
    pub fn IndexingInProgress() -> Self {
        Self(ApplyErrorEnum::IndexingInProgress)
    }

    #[allow(non_snake_case)]
    pub fn RebuildNoSchema(attr: impl Into<String>) -> Self {
        Self(ApplyErrorEnum::RebuildNoSchema { attr: attr.into() })
    }

    #[allow(non_snake_case)]
    pub fn Mutations(causes: Vec<ApplyError>) -> Self {
        Self(ApplyErrorEnum::Mutations { causes })
    }

    #[allow(non_snake_case)]
    pub fn RootNamespace() -> Self {
        Self(ApplyErrorEnum::RootNamespace)
    }

    /// Whether the request was refused for having read before a drop or schema change it writes under.
    pub fn is_aborted(&self) -> bool {
        matches!(self.0, ApplyErrorEnum::Aborted)
    }
}
