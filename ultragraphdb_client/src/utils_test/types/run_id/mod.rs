//! Which run this is.

use crate::utils_test::{RUN_ID_ENV, env_var};

/// A correlation id for one test run.
///
/// The reason a caller wants one is data scoping. A suite sharing a Dgraph with concurrent runs
/// has to keep out of their way by naming its own data, and a name is only useful if every run
/// gets a different one.
///
/// A LOCAL RUN HAS NO SUCH ID. There is deliberately no constant fallback -- that is what once
/// let two concurrent runs share one fixture and tear down each other's data -- so an unset
/// [`RUN_ID_ENV`] synthesises a local id and SAYS SO through [`Self::is_supplied`], rather than
/// borrowing a name that means "the CI run".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunId {
    value: String,
    supplied: bool,
}

impl RunId {
    /// The supplied id when there is one, otherwise a synthesised local id.
    ///
    /// Infallible on purpose: a missing correlation id must not fail a test that would otherwise
    /// have run. The distinction is reported by [`Self::is_supplied`] instead, so a log line
    /// cannot quietly imply CI provenance for a local run.
    pub fn resolve() -> Self {
        match env_var(RUN_ID_ENV) {
            Some(value) => Self {
                value,
                supplied: true,
            },
            // The pid keeps two suites started on one machine from claiming the same id.
            None => Self {
                value: format!("local-{}", std::process::id()),
                supplied: false,
            },
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// True when the environment supplied the id; false for a synthesised local one.
    pub fn is_supplied(&self) -> bool {
        self.supplied
    }
}
