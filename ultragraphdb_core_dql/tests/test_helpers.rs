/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Stand-ins for testify's `require` as Dgraph's dql tests use it.

use std::collections::HashMap;
use std::fmt::Debug;

use ultragraphdb_core_dql::{DqlError, GraphQuery, Request};

/// A result split as Go returns it: the value (its default on error) and the error.
pub(crate) fn split<T: Default>(r: Result<T, DqlError>) -> (T, Option<DqlError>) {
    match r {
        Ok(v) => (v, None),
        Err(e) => (T::default(), Some(e)),
    }
}

/// `require.NoError`.
pub(crate) fn no_err<E: Debug>(err: &Option<E>) {
    assert!(err.is_none(), "unexpected error: {err:?}");
}

/// `require.Contains(t, err.Error(), want)`.
pub(crate) fn contains(err: &Option<DqlError>, want: &str) {
    let Some(e) = err else {
        panic!("expected an error containing {want:?}, got none");
    };
    assert!(
        e.to_string().contains(want),
        "error {:?} does not contain {want:?}",
        e.to_string()
    );
}

/// A request with variables.
pub(crate) fn vars(query: &str, vars: &[(&str, &str)]) -> Request {
    let vars: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    Request::with_variables(query, vars)
}

/// The attributes of a node's children, Go's `childAttrs`.
pub(crate) fn child_attrs(g: &GraphQuery) -> Vec<String> {
    g.children().iter().map(|c| c.attr().to_string()).collect()
}
