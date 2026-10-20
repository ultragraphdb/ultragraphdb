/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/rdf_parser.go. Changed for ultragraphdb: translated to Rust; a line of bytes
 * that are not UTF-8 is read as Go reads it, each such byte one U+FFFD.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;

use ultragraphdb_core_facets::facet_for;
use ultragraphdb_core_keys::{STAR, predicate_lang};
use ultragraphdb_core_lex::{ITEM_EOF, ItemIterator, Lexer};
use ultragraphdb_core_types::{TypeId, Val, convert, object_value};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::value::Val as ApiVal;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_proto_pb::pb::metadata::HintType;

use crate::errors::chunker_error::{ChunkerError, ChunkerErrorEnum};
use crate::rdf_state::{
    ITEM_COMMA, ITEM_COMMENT, ITEM_EQUAL, ITEM_LABEL, ITEM_LANGUAGE, ITEM_LEFT_ROUND, ITEM_LITERAL,
    ITEM_OBJECT, ITEM_OBJECT_FUNC, ITEM_OBJECT_TYPE, ITEM_PREDICATE, ITEM_RIGHT_ROUND, ITEM_STAR,
    ITEM_SUBJECT, ITEM_SUBJECT_FUNC, ITEM_TEXT, ITEM_VALID_END, ITEM_VAR_NAME, LEX_TEXT,
};
use crate::utils::go_strconv::{parse_uint, quote, unquote};
use crate::utils::go_string::go_string;
use crate::utils::go_unicode::is_letter_or_digit;

/// Sanity check for subject, predicate and object strings: an object id can be empty, the subject and predicate
/// are checked not to be; anything else needs at least one letter or digit.
fn sane(s: &str) -> bool {
    s.is_empty() || s.chars().any(is_letter_or_digit)
}

/// Dgraph's `ParseRDFs`: every N-Quad of the lines, skipping empty and comment lines, with the type hints of their
/// predicates. All N-Quads are held at once.
///
/// Bytes that are not UTF-8 are read as Go reads them, each one U+FFFD, which a literal's value keeps as Go's
/// unquoting does; an IRI holds the replacement where Go keeps the raw byte, which a dgo string cannot hold.
///
/// # Errors
///
/// The error of the first line that does not parse.
pub fn parse_rdfs(b: &[u8]) -> Result<(Vec<api::NQuad>, pb::Metadata), ChunkerError> {
    let mut nqs = Vec::new();
    for line in b.split(|&c| c == b'\n') {
        let line = go_string(line);
        match parse_rdf(&line, &mut Lexer::new(&line)) {
            Ok(nq) => nqs.push(nq),
            Err(e) if e.kind() == &ChunkerErrorEnum::Empty => {}
            Err(e) => return Err(e),
        }
    }
    let md = calculate_type_hints(&nqs);
    Ok((nqs, md))
}

fn trim_spaces(s: &str) -> &str {
    s.trim_matches(' ')
}

/// Dgraph's `ParseRDF`: the N-Quad of one line, after `http://www.w3.org/TR/n-quads/`, lexed by `l`.
///
/// # Errors
///
/// [`ChunkerErrorEnum::Empty`] for an empty or comment line, [`ChunkerErrorEnum::Lex`] when the line does not lex,
/// [`ChunkerErrorEnum::Types`] when a typed literal's value does not convert, [`ChunkerErrorEnum::Rdf`] otherwise.
pub fn parse_rdf<'a>(line: &'a str, l: &mut Lexer<'a>) -> Result<api::NQuad, ChunkerError> {
    let mut rnq = api::NQuad::default();
    let line = line.trim();
    if line.is_empty() {
        return Err(ChunkerError::new(ChunkerErrorEnum::Empty));
    }

    l.reset(line);
    l.run(LEX_TEXT);
    l.validate_result()?;
    let mut it = l.new_iterator();
    let mut oval: Vec<u8> = Vec::new();
    let mut seen_oval = false;
    let mut vend = false;
    let mut is_comment_line = false;
    let end_of_input = || ChunkerError::rdf(format!("invalid end of input. Input: [{line}]"));
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_SUBJECT => rnq.subject = trim_spaces(item.val()).to_string(),
            ITEM_SUBJECT_FUNC => rnq.subject = parse_function(&mut it)?,
            ITEM_OBJECT_FUNC => rnq.object_id = parse_function(&mut it)?,
            ITEM_PREDICATE => {
                // Here we split predicate and lang directive (ex: "name@en"), if needed.
                let (pred, lang) = predicate_lang(trim_spaces(item.val()));
                rnq.predicate = pred.to_string();
                rnq.lang = lang.to_string();
            }
            ITEM_OBJECT => rnq.object_id = trim_spaces(item.val()).to_string(),
            ITEM_STAR => {
                if rnq.subject.is_empty() {
                    rnq.subject = STAR.to_string();
                } else if rnq.predicate.is_empty() {
                    rnq.predicate = STAR.to_string();
                } else {
                    rnq.object_value = Some(default_val(STAR.to_string()));
                }
            }
            ITEM_LITERAL => {
                oval = unquote(item.val())
                    .ok_or_else(|| ChunkerError::rdf("while unquoting: invalid syntax"))?;
                seen_oval = true;
            }
            ITEM_LANGUAGE => rnq.lang = item.val().to_string(),
            ITEM_OBJECT_TYPE => rnq.object_value = Some(typed_value(&rnq, item.val(), &oval)?),
            ITEM_COMMENT => {
                is_comment_line = true;
                vend = true;
            }
            ITEM_VALID_END => {
                vend = true;
                if !it.next() {
                    return Err(end_of_input());
                }
                // RDF spec says N-Quads should be terminated with a newline. Since we break the input by newline
                // already. We should get EOF or # after dot(.)
                let t = it.item().typ();
                if t != ITEM_EOF && t != ITEM_COMMENT {
                    return Err(ChunkerError::rdf(format!(
                        "invalid end of input. Expected newline or # after . Input: [{line}]"
                    )));
                }
                break;
            }
            ITEM_LABEL => {
                rnq.namespace = parse_uint(trim_spaces(item.val()), 0).map_err(|_| {
                    ChunkerError::rdf(format!("invalid namespace ID. Input: [{line}]"))
                })?;
            }
            ITEM_LEFT_ROUND => {
                it.prev(); // backup '('
                parse_facets_rdf(&mut it, &mut rnq)
                    .map_err(|e| ChunkerError::rdf(format!("could not parse facet: {e}")))?;
            }
            _ => {}
        }
    }

    if !vend {
        return Err(end_of_input());
    }
    if is_comment_line {
        return Err(ChunkerError::new(ChunkerErrorEnum::Empty));
    }
    // We only want to set default value if we have seen ObjectValue within "" and if we didn't already set it.
    if seen_oval && rnq.object_value.is_none() {
        rnq.object_value = Some(default_val(go_string(&oval).into_owned()));
    }
    if rnq.subject.is_empty() || rnq.predicate.is_empty() {
        return Err(ChunkerError::rdf(format!(
            "empty required fields in NQuad. Input: [{line}]"
        )));
    }
    if rnq.object_id.is_empty() && rnq.object_value.is_none() {
        return Err(ChunkerError::rdf(format!(
            "no Object in NQuad. Input: [{line}]"
        )));
    }
    if !sane(&rnq.subject) || !sane(&rnq.predicate) || !sane(&rnq.object_id) {
        // Don't format the full line, as it may contain sensitive information
        return Err(ChunkerError::rdf(format!(
            "NQuad failed sanity check. Subject: {}, Predicate: {}, ObjectId: {}",
            quote(&rnq.subject),
            quote(&rnq.predicate),
            quote(&rnq.object_id)
        )));
    }
    Ok(rnq)
}

fn default_val(s: String) -> api::Value {
    api::Value {
        val: Some(ApiVal::DefaultVal(s)),
    }
}

/// The value of the literal `oval` typed by the IRI `typ`, as the dgo API carries it.
fn typed_value(rnq: &api::NQuad, typ: &str, oval: &[u8]) -> Result<api::Value, ChunkerError> {
    if rnq.predicate == STAR || rnq.subject == STAR {
        return Err(ChunkerError::rdf(
            "if predicate/subject is *, value should be * as well",
        ));
    }
    let val = trim_spaces(typ);
    if val == "*" {
        return Err(ChunkerError::rdf("itemObject can't be *"));
    }
    // Lets find out the storage type from the type map.
    let t =
        type_map(val).ok_or_else(|| ChunkerError::rdf(format!("unrecognized rdf type {val}")))?;
    if oval.is_empty() && t != TypeId::STRING {
        return Err(ChunkerError::rdf("invalid ObjectValue"));
    }
    // if this is a password value dont re-encrypt. issue#2765
    let src_tid = if t == TypeId::PASSWORD {
        t
    } else {
        TypeId::STRING
    };
    let p = convert(&Val::bytes(src_tid, oval), t)?;
    match p.value() {
        Some(v) => Ok(object_value(t, v)?),
        // `"null"^^<geo:geojson>` converts to no geometry, on which Dgraph's ObjectValue panics.
        None => Err(ChunkerError::rdf("invalid ObjectValue")),
    }
}

/// Parses `uid(<var name>)` or `val(<var name>)` and returns it without the spaces around the name.
fn parse_function(it: &mut ItemIterator<'_, '_>) -> Result<String, ChunkerError> {
    let mut s = it.item().val().to_string();

    it.next();
    let item = it.item();
    if item.typ() != ITEM_LEFT_ROUND {
        return Err(ChunkerError::rdf(format!(
            "expected '(', found: {}",
            item.val()
        )));
    }

    it.next();
    let item = it.item();
    if item.typ() != ITEM_VAR_NAME {
        return Err(ChunkerError::rdf(format!(
            "expected variable name, found: {}",
            item.val()
        )));
    }
    if item.val().trim().is_empty() {
        return Err(ChunkerError::rdf("empty variable name in function call"));
    }
    s = format!("{s}({})", item.val());

    it.next();
    let item = it.item();
    if item.typ() != ITEM_RIGHT_ROUND {
        return Err(ChunkerError::rdf(format!(
            "expected ')', found: {}",
            item.val()
        )));
    }
    Ok(s)
}

/// Parses `(key=value, ...)` into the N-Quad's facets; the error is the text after `could not parse facet: `.
fn parse_facets_rdf(it: &mut ItemIterator<'_, '_>, rnq: &mut api::NQuad) -> Result<(), String> {
    const UNEXPECTED_END: &str = "unexpected end of facets";
    if !it.next() {
        return Err(UNEXPECTED_END.to_string());
    }
    let item = it.item();
    if item.typ() != ITEM_LEFT_ROUND {
        return Err(format!("expected '(' but found {} at facet", item.val()));
    }

    while it.next() {
        // parse one key value pair
        // parse key
        let item = it.item();
        if item.typ() != ITEM_TEXT {
            return Err(format!("expected key but found {}", item.val()));
        }
        let facet_key = item.val().trim();
        if facet_key.is_empty() {
            return Err("empty facetKeys not allowed".to_string());
        }
        // parse =
        if !it.next() {
            return Err(UNEXPECTED_END.to_string());
        }
        let item = it.item();
        if item.typ() != ITEM_EQUAL {
            return Err(format!("expected = after facetKey. Found {}", item.val()));
        }
        // parse value or empty value
        if !it.next() {
            return Err(UNEXPECTED_END.to_string());
        }
        let item = it.item();
        let facet_val = if item.typ() == ITEM_TEXT {
            item.val()
        } else {
            ""
        };
        let facet = facet_for(facet_key, facet_val).map_err(|e| e.to_string())?;
        rnq.facets.push(facet);

        // empty value case..
        if item.typ() == ITEM_RIGHT_ROUND {
            break;
        }
        if item.typ() == ITEM_COMMA {
            continue;
        }
        if item.typ() != ITEM_TEXT {
            return Err(format!("expected , or ) or text but found {}", item.val()));
        }
        // value was present...
        if !it.next() {
            // get either ')' or ','
            return Err(UNEXPECTED_END.to_string());
        }
        let item = it.item();
        if item.typ() == ITEM_RIGHT_ROUND {
            break;
        }
        if item.typ() == ITEM_COMMA {
            continue;
        }
        return Err(format!(
            "expected , or ) after facet. Received {}",
            item.val()
        ));
    }
    Ok(())
}

/// The type hints of the N-Quads' predicates: a predicate a subject has twice is a list.
fn calculate_type_hints(nqs: &[api::NQuad]) -> pb::Metadata {
    // Stores the count of <subject, pred> pairs to help figure out whether schemas should be created as scalars or
    // lists of scalars.
    let mut schema_count_map: HashMap<(&str, &str), usize> = HashMap::new();
    let mut pred_hints: HashMap<String, i32> = HashMap::new();
    for nq in nqs {
        let count = schema_count_map
            .entry((nq.subject.as_str(), nq.predicate.as_str()))
            .or_insert(0);
        *count += 1;
        if *count > 1 {
            pred_hints.insert(nq.predicate.clone(), HintType::List as i32);
        }
    }
    pb::Metadata { pred_hints }
}

/// The storage type of an RDF object type.
fn type_map(t: &str) -> Option<TypeId> {
    Some(match t {
        "xs:password" => TypeId::PASSWORD,
        "xs:string" => TypeId::STRING,
        "xs:date" => TypeId::DATETIME,
        "xs:dateTime" => TypeId::DATETIME,
        "xs:int" => TypeId::INT,
        "xs:integer" => TypeId::INT,
        "xs:positiveInteger" => TypeId::INT,
        "xs:boolean" => TypeId::BOOL,
        "xs:double" => TypeId::FLOAT,
        "xs:float" => TypeId::FLOAT,
        "xs:base64Binary" => TypeId::BINARY,
        "xs:decimal" => TypeId::BIGFLOAT,
        "geo:geojson" => TypeId::GEO,
        "xs:[]float32" => TypeId::VFLOAT,
        "http://www.w3.org/2001/XMLSchema#string" => TypeId::STRING,
        "http://www.w3.org/2001/XMLSchema#dateTime" => TypeId::DATETIME,
        "http://www.w3.org/2001/XMLSchema#date" => TypeId::DATETIME,
        "http://www.w3.org/2001/XMLSchema#int" => TypeId::INT,
        "http://www.w3.org/2001/XMLSchema#positiveInteger" => TypeId::INT,
        "http://www.w3.org/2001/XMLSchema#integer" => TypeId::INT,
        "http://www.w3.org/2001/XMLSchema#boolean" => TypeId::BOOL,
        "http://www.w3.org/2001/XMLSchema#double" => TypeId::FLOAT,
        "http://www.w3.org/2001/XMLSchema#float" => TypeId::FLOAT,
        "http://www.w3.org/2001/XMLSchema#gYear" => TypeId::DATETIME,
        "http://www.w3.org/2001/XMLSchema#gYearMonth" => TypeId::DATETIME,
        _ => return None,
    })
}
