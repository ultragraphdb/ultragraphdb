/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashSet;

use ultragraphdb_core_keys::parse_attr;
use ultragraphdb_core_tok::get_tokenizer;
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_proto_pb::pb::schema_update::Directive;

use crate::errors::schema_error::SchemaError;
use crate::parse::has_tokenizer_or_vector_index_spec;

/// Dgraph's `resolveTokenizers`: checks every update's index against its value type, once the whole schema is
/// parsed.
pub(crate) fn resolve_tokenizers(updates: &[pb::SchemaUpdate]) -> Result<(), SchemaError> {
    for schema in updates {
        let typ = TypeId::new(schema.value_type);
        let indexed = schema.directive == Directive::Index as i32;

        if (typ == TypeId::UID || typ == TypeId::DEFAULT || typ == TypeId::PASSWORD) && indexed {
            return Err(SchemaError::parse(format!(
                "Indexing not allowed on predicate {} of type {}",
                parse_attr(&schema.predicate),
                typ.name()
            )));
        }

        if typ == TypeId::UID {
            continue;
        }

        let has_index = has_tokenizer_or_vector_index_spec(schema);
        if !has_index && indexed {
            return Err(SchemaError::parse(format!(
                "Require type of tokenizer for pred: {} of type: {} for indexing.",
                schema.predicate,
                typ.name()
            )));
        } else if has_index && !indexed {
            return Err(SchemaError::parse(format!(
                "Tokenizers present without indexing on attr {}",
                parse_attr(&schema.predicate)
            )));
        }
        // check for valid tokenizer types and duplicates
        let mut seen = HashSet::new();
        let mut seen_sortable_tok = false;
        for t in &schema.tokenizer {
            let Some(tokenizer) = get_tokenizer(t) else {
                return Err(SchemaError::parse(format!("Invalid tokenizer {t}")));
            };
            if tokenizer.type_id() != typ {
                return Err(SchemaError::parse(format!(
                    "Tokenizer: {} isn't valid for predicate: {} of type: {}",
                    tokenizer.name(),
                    parse_attr(&schema.predicate),
                    typ.name()
                )));
            }
            if !seen.insert(tokenizer.name()) {
                return Err(SchemaError::parse(format!(
                    "Duplicate tokenizers present for attr {}",
                    parse_attr(&schema.predicate)
                )));
            }
            if tokenizer.is_sortable() {
                if seen_sortable_tok {
                    return Err(SchemaError::parse(format!(
                        "More than one sortable index encountered for: {}",
                        schema.predicate
                    )));
                }
                seen_sortable_tok = true;
            }
        }
    }
    Ok(())
}
