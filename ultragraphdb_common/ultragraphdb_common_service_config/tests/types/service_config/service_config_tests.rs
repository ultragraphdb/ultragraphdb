/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::Path;

use ultragraphdb_common_service_config::{
    ServiceConfig, ServiceConfigError, ServiceConfigErrorEnum,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_tool_toml::TomlErrorEnum;

/// A document naming every setting a table holds.
const EVERY_SETTING: &str = r#"
[cluster]
role_secret_file = "/secret"
frontend_address = "tcp://f:1"
partition_count = 12
[compute]
role_address = "tcp://c:2"
[endpoint]
address = "tcp://e:3"
tls_cert_file = "/cert"
tls_key_file = "/key"
plaintext = false
connections_max = 13
[frontend]
role_address = "tcp://f:4"
data_dir = "/f"
[metadata]
block_size_max = 14
sessions_max = 15
snapshot_rounds = 16
[observability]
address = "tcp://o:5"
[runtime]
pin_cores = true
[sequencer]
epoch_ms_min = 17
epoch_ms_max = 18
[storage]
partition_first = 19
partition_count = 20
role_address = "tcp://s:6"
data_dir = "/s"
close_request_ms = 21
"#;

fn variable(name: &str, value: &str) -> (String, String) {
    (name.to_string(), value.to_string())
}

fn refusal(
    result: Result<ServiceConfig, ServiceConfigError>,
) -> TestResult<ServiceConfigErrorEnum> {
    match result {
        Ok(config) => Err(TestError::new(format!("accepted as {config:?}"))),
        Err(error) => Ok(error.0),
    }
}

/// One to one: every key of the document reaches the field of its name.
#[test]
fn test_every_setting_reaches_its_field() -> TestResult {
    let c = ServiceConfig::parse(Some(EVERY_SETTING.as_bytes()), [])?;
    assert_eq!(c.cluster().role_secret_file(), Some("/secret"));
    assert_eq!(c.cluster().frontend_address(), Some("tcp://f:1"));
    assert_eq!(c.cluster().partition_count(), Some(12));
    assert_eq!(c.compute().role_address(), Some("tcp://c:2"));
    assert_eq!(c.endpoint().address(), Some("tcp://e:3"));
    assert_eq!(c.endpoint().tls_cert_file(), Some("/cert"));
    assert_eq!(c.endpoint().tls_key_file(), Some("/key"));
    assert_eq!(c.endpoint().plaintext(), Some(false));
    assert_eq!(c.endpoint().connections_max(), Some(13));
    assert_eq!(c.frontend().role_address(), Some("tcp://f:4"));
    assert_eq!(c.frontend().data_dir(), Some("/f"));
    assert_eq!(c.metadata().block_size_max(), Some(14));
    assert_eq!(c.metadata().sessions_max(), Some(15));
    assert_eq!(c.metadata().snapshot_rounds(), Some(16));
    assert_eq!(c.observability().address(), Some("tcp://o:5"));
    assert_eq!(c.runtime().pin_cores(), Some(true));
    assert_eq!(c.sequencer().epoch_ms_min(), Some(17));
    assert_eq!(c.sequencer().epoch_ms_max(), Some(18));
    assert_eq!(c.storage().partition_first(), Some(19));
    assert_eq!(c.storage().partition_count(), Some(20));
    assert_eq!(c.storage().role_address(), Some("tcp://s:6"));
    assert_eq!(c.storage().data_dir(), Some("/s"));
    assert_eq!(c.storage().close_request_ms(), Some(21));
    Ok(())
}

/// Each kind of setting is overridden by the variable its table and key spell.
#[test]
fn test_a_variable_overrides_the_file() -> TestResult {
    let c = ServiceConfig::parse(
        Some(EVERY_SETTING.as_bytes()),
        [
            variable("ULTRAGRAPHDB_STORAGE_PARTITION_COUNT", "2"),
            variable("ULTRAGRAPHDB_SEQUENCER_EPOCH_MS_MAX", "99"),
            variable("ULTRAGRAPHDB_RUNTIME_PIN_CORES", "false"),
            variable("ULTRAGRAPHDB_ENDPOINT_PLAINTEXT", "true"),
            variable("ULTRAGRAPHDB_CLUSTER_ROLE_SECRET_FILE", "/other"),
        ],
    )?;
    assert_eq!(c.storage().partition_count(), Some(2));
    assert_eq!(c.sequencer().epoch_ms_max(), Some(99));
    assert_eq!(c.runtime().pin_cores(), Some(false));
    assert_eq!(c.endpoint().plaintext(), Some(true));
    assert_eq!(c.cluster().role_secret_file(), Some("/other"));
    assert_eq!(
        c.storage().partition_first(),
        Some(19),
        "what no variable names stays"
    );
    Ok(())
}

/// Other programs' variables are not this crate's to refuse.
#[test]
fn test_a_variable_without_the_prefix_is_ignored() -> TestResult {
    let c = ServiceConfig::parse(
        None,
        [variable("PATH", "/bin"), variable("ULTRAGRAPHDBX", "1")],
    )?;
    assert_eq!(c, ServiceConfig::default());
    Ok(())
}

/// A misspelled key stops the start, naming the key.
#[test]
fn test_an_unknown_setting_is_refused() -> TestResult {
    let text = "[storage]\npartiton_count = 4\n";
    let error = refusal(ServiceConfig::parse(Some(text.as_bytes()), []))?;
    let ServiceConfigErrorEnum::Toml(toml) = error else {
        return Err(TestError::new(format!("refused as {error:?}")));
    };
    assert!(matches!(toml.0, TomlErrorEnum::UnreadKey { ref key, .. } if key == "partiton_count"));
    Ok(())
}

/// A key in the root table is mirrored by no field, and a file holding none is fine.
#[test]
fn test_a_root_key_is_refused_and_an_empty_root_is_not() -> TestResult {
    let error = refusal(ServiceConfig::parse(Some(b"name = \"dev\"\n"), []))?;
    assert!(
        matches!(error, ServiceConfigErrorEnum::Toml(_)),
        "{error:?}"
    );
    assert_eq!(
        ServiceConfig::parse(Some(b"# nothing\n"), [])?,
        ServiceConfig::default()
    );
    Ok(())
}

/// A file's value of the wrong kind is refused even when a variable would override it.
#[test]
fn test_a_files_wrong_kind_is_refused_under_a_variable() -> TestResult {
    let text = "[storage]\npartition_count = \"four\"\n";
    let error = refusal(ServiceConfig::parse(
        Some(text.as_bytes()),
        [variable("ULTRAGRAPHDB_STORAGE_PARTITION_COUNT", "2")],
    ))?;
    assert!(
        matches!(error, ServiceConfigErrorEnum::Toml(_)),
        "{error:?}"
    );
    Ok(())
}

/// A boolean variable holds `true` or `false`, as TOML spells them.
#[test]
fn test_a_boolean_variable_of_the_wrong_kind_is_refused() -> TestResult {
    let error = refusal(ServiceConfig::parse(
        None,
        [variable("ULTRAGRAPHDB_RUNTIME_PIN_CORES", "yes")],
    ))?;
    assert_eq!(
        error,
        ServiceConfigErrorEnum::WrongKindVariable {
            variable: "ULTRAGRAPHDB_RUNTIME_PIN_CORES".to_string(),
            value: "yes".to_string(),
            expected: "a boolean",
        }
    );
    Ok(())
}

/// A value must fit its field, at each boundary and in each field's width.
#[test]
fn test_a_value_must_fit_its_field() -> TestResult {
    let max_u32 = u64::from(u32::MAX);
    let fits = ServiceConfig::parse(
        None,
        [variable(
            "ULTRAGRAPHDB_STORAGE_PARTITION_COUNT",
            &max_u32.to_string(),
        )],
    )?;
    assert_eq!(fits.storage().partition_count(), Some(u32::MAX));
    let past = max_u32 + 1;
    let too_large = refusal(ServiceConfig::parse(
        None,
        [variable(
            "ULTRAGRAPHDB_STORAGE_PARTITION_COUNT",
            &past.to_string(),
        )],
    ))?;
    assert_eq!(
        too_large,
        ServiceConfigErrorEnum::OutOfRange {
            setting: "storage.partition_count".to_string(),
            value: i64::try_from(past)?,
            max: max_u32,
        }
    );
    let negative = refusal(ServiceConfig::parse(
        None,
        [variable("ULTRAGRAPHDB_SEQUENCER_EPOCH_MS_MIN", "-1")],
    ))?;
    assert_eq!(
        negative,
        ServiceConfigErrorEnum::OutOfRange {
            setting: "sequencer.epoch_ms_min".to_string(),
            value: -1,
            max: i64::MAX.unsigned_abs(),
        }
    );
    let widest = ServiceConfig::parse(
        None,
        [variable(
            "ULTRAGRAPHDB_SEQUENCER_EPOCH_MS_MIN",
            &i64::MAX.to_string(),
        )],
    )?;
    assert_eq!(
        widest.sequencer().epoch_ms_min(),
        Some(i64::MAX.unsigned_abs())
    );
    Ok(())
}

/// A file named and readable is parsed; malformed TOML is refused as TOML.
#[test]
fn test_load_reads_the_named_file() -> TestResult {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let file = Path::new(&root).join(format!("service-config-{}.toml", std::process::id()));
    std::fs::write(&file, "[storage]\npartition_count = 4\n")?;
    let config = ServiceConfig::load(Some(&file), [])?;
    assert_eq!(config.storage().partition_count(), Some(4));
    std::fs::write(&file, "not toml\n")?;
    let error = refusal(ServiceConfig::load(Some(&file), []))?;
    assert!(
        matches!(error, ServiceConfigErrorEnum::Toml(_)),
        "{error:?}"
    );
    std::fs::remove_file(&file)?;
    assert_eq!(ServiceConfig::load(None, [])?, ServiceConfig::default());
    Ok(())
}

/// Every problem of a file is listed, not the first; a good file has none.
#[test]
fn test_problems_lists_every_refusal() {
    assert!(ServiceConfig::problems(b"[storage]\npartition_count = 2\n").is_empty());
    let problems = ServiceConfig::problems(
        b"[storage]\npartiton_count = 2\n[compute]\nrole_addres = \"tcp://c:1\"\n[nothing]\nx = 1\n",
    );
    let text = problems
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(problems.len() >= 3, "{text}");
    for word in ["partiton_count", "role_addres", "nothing"] {
        assert!(text.contains(word), "{word} in: {text}");
    }
    assert!(!ServiceConfig::problems(b"[broken\n").is_empty());
}

/// The service's own table and a role's admin table read back what they hold, and follow the endpoint's security rule.
#[test]
fn test_the_config_and_admin_tables_read_their_settings() -> TestResult {
    let read = ServiceConfig::parse(
        Some(
            br#"[config]
address = "tcp://c:1"
plaintext = true
data_dir = "/d"
read_key_file = "/r"
admin_key_file = "/a"
targets_file = "/t"
[admin]
address = "tcp://a:2"
tls_cert_file = "/cert"
tls_key_file = "/key"
key_sha256 = "abc"
"#,
        ),
        [],
    )?;
    let config = read.config();
    assert_eq!(config.address(), Some("tcp://c:1"));
    assert_eq!(config.data_dir(), Some("/d"));
    assert_eq!(config.read_key_file(), Some("/r"));
    assert_eq!(config.admin_key_file(), Some("/a"));
    assert_eq!(config.targets_file(), Some("/t"));
    assert!(matches!(
        config.security()?,
        ultragraphdb_common_service_config::EndpointSecurity::Plaintext
    ));
    let admin = read.admin();
    assert_eq!(admin.address(), Some("tcp://a:2"));
    assert_eq!(admin.key_sha256(), Some("abc"));
    assert!(matches!(
        admin.security()?,
        ultragraphdb_common_service_config::EndpointSecurity::Tls { .. }
    ));
    Ok(())
}
