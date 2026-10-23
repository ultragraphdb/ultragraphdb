/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Stand-ins for testify's `require` as Dgraph's dql tests use it, and the checks of a query's nesting bound.

use std::collections::HashMap;
use std::fmt::Debug;

use ultragraphdb_core_dql::{
    DqlError, FilterTree, GraphQuery, MathTree, ParseResult, Request, parse,
};

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

/// The nesting a query may reach.
pub(crate) const MAX_DEPTH: usize = 1000;

/// The text a query nested deeper than [`MAX_DEPTH`] is refused with.
pub(crate) const TOO_DEEP: &str = "Query nested more than 1000 levels deep";

/// Runs `f` on a thread with a 256 KiB stack.
pub(crate) fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(f)
        .expect("thread")
        .join()
        .expect("the thread finishes")
}

/// The levels of the deepest path through a result's blocks, filters and math expressions, each block's filter
/// and math expression hanging one level below it; counted without recursion.
pub(crate) fn levels(res: &ParseResult) -> usize {
    enum Node<'a> {
        Block(&'a GraphQuery),
        Filter(&'a FilterTree),
        Math(&'a MathTree),
    }
    let mut deepest = 0;
    let mut todo: Vec<(Node<'_>, usize)> =
        res.query().iter().map(|q| (Node::Block(q), 1)).collect();
    while let Some((node, level)) = todo.pop() {
        deepest = deepest.max(level);
        match node {
            Node::Block(gq) => {
                todo.extend(gq.children().iter().map(|c| (Node::Block(c), level + 1)));
                todo.extend(
                    [gq.filter(), gq.facets_filter()]
                        .into_iter()
                        .flatten()
                        .map(|f| (Node::Filter(f), level + 1)),
                );
                todo.extend(gq.math_exp().map(|m| (Node::Math(m), level + 1)));
            }
            Node::Filter(f) => todo.extend(f.child().iter().map(|c| (Node::Filter(c), level + 1))),
            Node::Math(m) => todo.extend(m.child().iter().map(|c| (Node::Math(c), level + 1))),
        }
    }
    deepest
}

/// Parses the query on a 256 KiB stack, then clones and drops its result there: the levels the result reaches,
/// or the error text.
pub(crate) fn parse_deep(query: String) -> Result<usize, String> {
    on_small_stack(move || {
        let res = parse(&Request::new(query)).map_err(|e| e.to_string())?;
        let copy = res.clone();
        let depth = levels(&copy);
        drop(copy);
        drop(res);
        Ok(depth)
    })
}

/// Checks that a query `MAX_DEPTH` levels deep parses, and one a level deeper is refused, both on a small stack.
pub(crate) fn check_depth_bound(query: impl Fn(usize) -> String) {
    assert_eq!(parse_deep(query(MAX_DEPTH)), Ok(MAX_DEPTH));
    assert_eq!(parse_deep(query(MAX_DEPTH + 1)), Err(TOO_DEEP.to_string()));
    // Far deeper, the refusal comes before the parser builds or recurses that deep.
    assert_eq!(
        parse_deep(query(100 * MAX_DEPTH)),
        Err(TOO_DEEP.to_string())
    );
}

/// Parses the query on a 256 KiB stack and there walks its result every way a caller can: clones it, compares the
/// clone, formats both `Debug` forms and every filter's and math expression's `debug_string`, and drops both. The
/// levels the result reaches.
pub(crate) fn walk_deep(query: String) -> usize {
    on_small_stack(move || {
        let res = parse(&Request::new(query)).expect("the query parses");
        let copy = res.clone();
        assert!(copy == res, "the clone differs");
        assert!(!format!("{res:?}").is_empty());
        assert!(!format!("{res:#?}").is_empty());
        let mut todo: Vec<&GraphQuery> = res.query().iter().collect();
        while let Some(gq) = todo.pop() {
            for f in [gq.filter(), gq.facets_filter()].into_iter().flatten() {
                f.debug_string().expect("a filter's text");
            }
            if let Some(m) = gq.math_exp() {
                m.debug_string().expect("a math expression's text");
            }
            todo.extend(gq.children());
        }
        let depth = levels(&res);
        drop(copy);
        drop(res);
        depth
    })
}
