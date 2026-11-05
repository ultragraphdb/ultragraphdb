/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;
use std::sync::Arc;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::{List, ListSource, PostingIdentity, WalkEnd, WalkStep};

use crate::errors::quad_error::QuadError;
use crate::types::kernel_scan::KernelScan;
use crate::types::page::Page;
use crate::types::page_end::PageEnd;
use crate::utils::postings::posting_object;

/// The one list path of a page of the read kernel, whichever source answers it: each key's list read at the page's version and
/// streamed to a sink object by object, noting where the walk stands so a stopped sink can resume after the last object it took. A plain page ([`crate::QuadStore::page`]) and a transaction's run it over [`ListSource`]s, so the two
/// cannot disagree about a key's row.
#[derive(Debug)]
pub struct PageWalk<'a, F> {
    scan: &'a KernelScan,
    version: u64,
    sink: F,
    rows: usize,
    /// The last posting the sink took of the key under way.
    at: PostingIdentity,
    /// The list of the key paused inside.
    paused_in: Option<Arc<List>>,
}

impl<'a, F> PageWalk<'a, F>
where
    F: FnMut(&[u8], &EncodedTerm, &[SystemId]) -> ControlFlow<()>,
{
    /// The walk of `scan`'s lists read at `version`, every object offered to `sink` with its key and statement IDs. `sink`
    /// breaks on an object it does not take.
    pub fn new(scan: &'a KernelScan, version: u64, sink: F) -> Self {
        Self {
            scan,
            version,
            sink,
            rows: 0,
            at: PostingIdentity::default(),
            paused_in: None,
        }
    }

    /// Streams the list of `key`, as `source` sees it, to the sink. The key the scan resumes after a posting of was a row before
    /// this page reached it, so finishing it completes a row even with nothing left to give, and it is read from the list the page
    /// before kept, so a list crossing many pages is read from the store once. A key resumed at a posting a reader named is a row
    /// of this page only when it gives one.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of the list.
    pub fn visit<L: ListSource>(&mut self, key: &[u8], source: &L) -> Result<WalkStep, QuadError> {
        let version = self.version;
        let resuming = key == self.scan.from();
        let after = self.scan.after().filter(|_| resuming);
        let held = self.scan.held().filter(|_| resuming);
        let (sink, at) = (&mut self.sink, &mut self.at);
        let (mut took, mut stopped) = (false, false);
        let mut read = |list: &List| {
            let walked = list.iterate_after(version, after, |posting| {
                let object = posting_object(key, posting)?;
                match sink(key, &object, posting.statement_ids()) {
                    ControlFlow::Continue(()) => {
                        list.identity_into(posting, at);
                        took = true;
                        Ok::<_, QuadError>(ControlFlow::Continue(()))
                    }
                    ControlFlow::Break(()) => {
                        stopped = true;
                        Ok(ControlFlow::Break(()))
                    }
                }
            });
            // The first pause inside a list keeps it; one resumed already has it kept.
            (
                walked,
                (took && stopped && held.is_none()).then(|| list.clone()),
            )
        };
        let (walked, kept) = match held {
            Some(list) => read(list),
            None => source.with_list(key, version, read)?,
        };
        walked?;
        self.paused_in = held.cloned().or_else(|| kept.map(Arc::new));
        Ok(match (stopped, took) {
            (true, true) => WalkStep::PausedInside,
            (true, false) => WalkStep::PausedBefore,
            (false, true) => self.completed(),
            (false, false) if after.is_some_and(|after| !after.is_included()) => self.completed(),
            (false, false) => WalkStep::Nothing,
        })
    }

    /// How the page ended, given how its walk did.
    pub fn end(self, walked: WalkEnd) -> Page {
        let end = match walked {
            WalkEnd::Exhausted => PageEnd::Exhausted,
            WalkEnd::Stopped { last, .. } => PageEnd::Stopped { last },
            WalkEnd::Paused { last, next } => paused(self.scan, last, next, self.at),
            WalkEnd::Yielded { next } => PageEnd::Yielded { from: next },
        };
        let inside = matches!(&end, PageEnd::Paused { after: Some(_), .. });
        Page::new(end, self.rows).with_held(self.paused_in.filter(|_| inside))
    }

    fn completed(&mut self) -> WalkStep {
        self.rows += 1;
        WalkStep::Row
    }
}

/// Where a page paused at `next` resumes: inside it when it is its own last key, otherwise where the previous span ended, so
/// that the next page's span extends it.
fn paused(scan: &KernelScan, last: Option<Vec<u8>>, next: Vec<u8>, at: PostingIdentity) -> PageEnd {
    match last {
        Some(last) if last == next => PageEnd::Paused {
            last: Some(last),
            from: next,
            after: Some(at),
        },
        Some(last) => {
            let mut from = last.clone();
            from.push(0);
            PageEnd::Paused {
                last: Some(last),
                from,
                after: None,
            }
        }
        None => PageEnd::Paused {
            last: None,
            after: scan.after().filter(|_| next == scan.from()).cloned(),
            from: next,
        },
    }
}
