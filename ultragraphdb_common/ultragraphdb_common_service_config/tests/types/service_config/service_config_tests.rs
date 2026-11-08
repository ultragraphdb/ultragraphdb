/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service_config::{
    EndpointTable, ServiceConfig, ServiceConfigError, ServiceConfigErrorEnum, StorageTable,
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
held_bytes = 24
refusal_interval_ms = 25
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
scan_stall_ms = 22
read_retain_ms = 23
"#;

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
    let c = ServiceConfig::parse(EVERY_SETTING.as_bytes())?;
    assert_eq!(c.cluster().role_secret_file(), Some("/secret"));
    assert_eq!(c.cluster().frontend_address(), Some("tcp://f:1"));
    assert_eq!(c.cluster().partition_count(), Some(12));
    assert_eq!(c.compute().role_address(), Some("tcp://c:2"));
    assert_eq!(c.endpoint().address(), Some("tcp://e:3"));
    assert_eq!(c.endpoint().tls_cert_file(), Some("/cert"));
    assert_eq!(c.endpoint().tls_key_file(), Some("/key"));
    assert_eq!(c.endpoint().plaintext(), Some(false));
    assert_eq!(c.endpoint().connections_max(), Some(13));
    assert_eq!(c.endpoint().held_bytes(), Some(24));
    assert_eq!(c.endpoint().refusal_interval_ms(), Some(25));
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
    assert_eq!(c.storage().scan_stall_ms(), Some(22));
    assert_eq!(c.storage().read_retain_ms(), Some(23));
    Ok(())
}

/// A retention and a held-bytes bound the file does not name read as absent, and the protocol's limits table holds what
/// each role takes in their place (`PROTOCOL.md` §11.9).
#[test]
fn test_the_limits_a_file_leaves_out_take_their_defaults() -> TestResult {
    let c = ServiceConfig::parse(b"# nothing\n")?;
    assert_eq!(c.storage().read_retain_ms(), None);
    assert_eq!(c.endpoint().held_bytes(), None);
    assert_eq!(StorageTable::READ_RETAIN_MS_DEFAULT, 60_000);
    assert_eq!(EndpointTable::HELD_BYTES_DEFAULT, 4_194_304);
    Ok(())
}

/// A misspelled key stops the start, naming the key.
#[test]
fn test_an_unknown_setting_is_refused() -> TestResult {
    let text = "[storage]\npartiton_count = 4\n";
    let error = refusal(ServiceConfig::parse(text.as_bytes()))?;
    let ServiceConfigErrorEnum::Toml(toml) = error else {
        return Err(TestError::new(format!("refused as {error:?}")));
    };
    assert!(matches!(toml.0, TomlErrorEnum::UnreadKey { ref key, .. } if key == "partiton_count"));
    Ok(())
}

/// A key in the root table is mirrored by no field, and a file holding none is fine.
#[test]
fn test_a_root_key_is_refused_and_an_empty_root_is_not() -> TestResult {
    let error = refusal(ServiceConfig::parse(b"name = \"dev\"\n"))?;
    assert!(
        matches!(error, ServiceConfigErrorEnum::Toml(_)),
        "{error:?}"
    );
    assert_eq!(
        ServiceConfig::parse(b"# nothing\n")?,
        ServiceConfig::default()
    );
    Ok(())
}

/// A value of the wrong kind is refused, naming its setting.
#[test]
fn test_a_value_of_the_wrong_kind_is_refused() -> TestResult {
    let error = refusal(ServiceConfig::parse(b"[runtime]\npin_cores = \"yes\"\n"))?;
    assert!(
        matches!(error, ServiceConfigErrorEnum::Toml(_)),
        "{error:?}"
    );
    assert!(ServiceConfig::parse(b"[storage]\npartition_count = \"four\"\n").is_err());
    Ok(())
}

/// A value must fit its field, at each boundary and in each field's width.
#[test]
fn test_a_value_must_fit_its_field() -> TestResult {
    let read = |table: &str, key: &str, value: String| {
        ServiceConfig::parse(format!("[{table}]\n{key} = {value}\n").as_bytes())
    };
    let max_u32 = u64::from(u32::MAX);
    let fits = read("storage", "partition_count", max_u32.to_string())?;
    assert_eq!(fits.storage().partition_count(), Some(u32::MAX));
    let past = max_u32 + 1;
    let too_large = refusal(read("storage", "partition_count", past.to_string()))?;
    assert_eq!(
        too_large,
        ServiceConfigErrorEnum::OutOfRange {
            setting: "storage.partition_count".to_string(),
            value: i64::try_from(past)?,
            max: max_u32,
        }
    );
    let negative = refusal(read("sequencer", "epoch_ms_min", "-1".to_string()))?;
    assert_eq!(
        negative,
        ServiceConfigErrorEnum::OutOfRange {
            setting: "sequencer.epoch_ms_min".to_string(),
            value: -1,
            max: i64::MAX.unsigned_abs(),
        }
    );
    let widest = read("sequencer", "epoch_ms_min", i64::MAX.to_string())?;
    assert_eq!(
        widest.sequencer().epoch_ms_min(),
        Some(i64::MAX.unsigned_abs())
    );
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

/// The settings [`EVERY_SETTING`] leaves out, and its two booleans named the other way.
const THE_REST: &str = r#"
[admin]
address = "tcp://a:7"
tls_cert_file = "/admin-cert"
tls_key_file = "/admin-key"
plaintext = false
key_sha256 = "ab"
[cluster]
id = 26
join_deadline_ms = 27
[endpoint]
plaintext = true
[metadata]
catalog_check_ms = 28
bootstrap_secret_file = "/bootstrap"
[runtime]
pin_cores = false
[storage]
held_bytes = 29
"#;

/// One to one for the rest: every key reaches the field of its name.
#[test]
fn test_the_remaining_settings_reach_their_fields() -> TestResult {
    let c = ServiceConfig::parse(THE_REST.as_bytes())?;
    assert_eq!(c.admin().address(), Some("tcp://a:7"));
    assert_eq!(c.admin().tls_cert_file(), Some("/admin-cert"));
    assert_eq!(c.admin().tls_key_file(), Some("/admin-key"));
    assert_eq!(c.admin().key_sha256(), Some("ab"));
    assert_eq!(c.cluster().id(), Some(26));
    assert_eq!(c.cluster().join_deadline_ms(), Some(27));
    assert_eq!(c.endpoint().plaintext(), Some(true));
    assert_eq!(c.metadata().catalog_check_ms(), Some(28));
    assert_eq!(c.metadata().bootstrap_secret_file(), Some("/bootstrap"));
    assert_eq!(c.runtime().pin_cores(), Some(false));
    assert_eq!(c.storage().held_bytes(), Some(29));
    Ok(())
}
