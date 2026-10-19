//! The Dgraph endpoint, read from the environment.

use crate::TlsMode;
use crate::utils_test::errors::fixture_error::FixtureError;
use crate::utils_test::{DEFAULT_HOST, DEFAULT_PORT, HOST_ENV, PORT_ENV, SSLMODE_ENV, env_var};

/// Dgraph's admin HTTP port. Fixed by Dgraph, and not configurable here because the
/// configuration describes the gRPC endpoint a client connects to, not an admin port nothing
/// connects to.
pub const HTTP_PORT: u16 = 8080;

/// Where Dgraph is, and what transport security it expects.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    host: String,
    port: u16,
    tls_mode: TlsMode,
}

impl Endpoint {
    /// The production resolution path: [`HOST_ENV`], [`PORT_ENV`] and [`SSLMODE_ENV`], each with
    /// a default. There is no test-only constructor on the public API, so a change that breaks
    /// real callers breaks this too.
    pub fn from_env() -> Result<Self, FixtureError> {
        let host = env_var(HOST_ENV).unwrap_or_else(|| DEFAULT_HOST.to_string());

        let port = match env_var(PORT_ENV) {
            None => DEFAULT_PORT,
            Some(value) => value.parse::<u16>().map_err(|_| {
                FixtureError::configuration(format!("{PORT_ENV}={value:?} is not a port number"))
            })?,
        };

        // The typed posture, not a string: an unknown spelling is an error here rather than a
        // silent fall back to plaintext. `TlsMode::from_wire` is the client's own parser, so the
        // fixture and the connection string it hands out cannot disagree about what `require`
        // means.
        let tls_mode = match env_var(SSLMODE_ENV) {
            None => TlsMode::Disable,
            Some(value) => TlsMode::from_wire(&value).ok_or_else(|| {
                FixtureError::configuration(format!(
                    "{SSLMODE_ENV}={value:?} is not one of disable, require, verify-ca"
                ))
            })?,
        };

        Ok(Self {
            host,
            port,
            tls_mode,
        })
    }

    /// Only for tests of this module's own logic, and for a probe that has to rebuild an
    /// endpoint from what `docker_utils` reports. Not a way around [`Self::from_env`].
    pub(crate) fn new(host: impl Into<String>, port: u16, tls_mode: TlsMode) -> Self {
        Self {
            host: host.into(),
            port,
            tls_mode,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn tls_mode(&self) -> TlsMode {
        self.tls_mode
    }

    /// True when Dgraph is terminating TLS.
    ///
    /// This decides the health check's scheme as well as the client's: enabling TLS moves the
    /// WHOLE of Dgraph's HTTP port to HTTPS, not just gRPC. A plain HTTP request to a TLS-enabled
    /// alpha is answered with `client sent an HTTP request to an HTTPS server` and nothing else.
    pub fn is_tls(&self) -> bool {
        self.tls_mode.is_tls()
    }

    fn scheme(&self) -> &'static str {
        if self.is_tls() { "https" } else { "http" }
    }

    /// THE READINESS URL. Answers only for the alpha that serves it, and answers 503 until that
    /// alpha can actually serve.
    ///
    /// Deliberately NOT `?all`. That variant reports cluster MEMBERSHIP, which goes green
    /// earlier: an alpha will list itself and its zero as healthy while still refusing gRPC with
    /// "Please retry again, server is not ready to accept requests". Gating on it produced
    /// exactly that -- a fixture that reported ready and a client that could not connect. The
    /// two endpoints answer two different questions and only this one is about readiness.
    pub fn health_url(&self) -> String {
        format!("{}://{}:{HTTP_PORT}/health", self.scheme(), self.host)
    }

    /// THE CLUSTER-WIDE URL, for asserting the shape of a deployment: one entry per server, so a
    /// check can require that every alpha and zero is up and grouped as configured rather than
    /// only that whichever one answered is alive.
    pub fn cluster_health_url(&self) -> String {
        format!("{}://{}:{HTTP_PORT}/health?all", self.scheme(), self.host)
    }

    /// Dgraph's admin GraphQL endpoint.
    pub fn admin_url(&self) -> String {
        format!("{}://{}:{HTTP_PORT}/admin", self.scheme(), self.host)
    }

    /// The connection string a client parses, carrying the configured posture.
    ///
    /// The port is a parameter because `docker_utils` is authoritative about where a container
    /// actually landed; on the pre-existing path it is simply [`Self::port`].
    pub fn connection_string_at(&self, port: u16) -> String {
        let host = &self.host;
        match self.tls_mode {
            // Omitted rather than spelled out: plaintext is the client's default, so this is the
            // string a caller would actually write.
            TlsMode::Disable => format!("dgraph://{host}:{port}"),
            TlsMode::RequireNoVerify => format!("dgraph://{host}:{port}?sslmode=require"),
            TlsMode::VerifyCa => format!("dgraph://{host}:{port}?sslmode=verify-ca"),
        }
    }
}
