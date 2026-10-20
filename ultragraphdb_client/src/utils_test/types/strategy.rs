//! How an endpoint is made usable.

use crate::utils_test::errors::fixture_error::FixtureError;
use crate::utils_test::types::exclusivity::Exclusivity;
use crate::utils_test::{STRATEGY_ENV, env_var};

/// The two ways to obtain a Dgraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Strategy {
    /// Provision a `dgraph/standalone` container. The default: a workstation with a Docker
    /// daemon needs no configuration at all.
    #[default]
    Container,
    /// Verify a Dgraph this process did not create -- a CI service container, or a deployed
    /// cluster.
    Existing,
}

impl Strategy {
    /// Read [`STRATEGY_ENV`], defaulting to [`Strategy::Container`].
    pub fn from_env() -> Result<Self, FixtureError> {
        match env_var(STRATEGY_ENV) {
            None => Ok(Self::Container),
            Some(value) => Self::parse(&value),
        }
    }

    /// Parse the wire spelling. An unknown value is REFUSED rather than defaulted.
    ///
    /// The refusal is the point. Defaulting a misspelled `existng` to `container` would start a
    /// container against a configuration that meant to verify a shared cluster, and the run
    /// would pass having tested the wrong thing. There are two values and both are named here.
    pub fn parse(value: &str) -> Result<Self, FixtureError> {
        match value {
            "container" => Ok(Self::Container),
            "existing" => Ok(Self::Existing),
            other => Err(FixtureError::unsupported_strategy(other)),
        }
    }

    /// A container this process started is ours; anything pre-existing is not.
    pub fn exclusivity(self) -> Exclusivity {
        match self {
            Self::Container => Exclusivity::Exclusive,
            Self::Existing => Exclusivity::Shared,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Container => "container",
            Self::Existing => "existing",
        }
    }
}
