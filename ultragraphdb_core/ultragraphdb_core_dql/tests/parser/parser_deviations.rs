/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The golden parse cases of `dql_parse.json` on which the port's outcome is none of Go's.

/// One deviating case of an input: the variables it is parsed with, and the port's error.
pub(crate) type Deviation = (&'static [(&'static str, &'static str)], &'static str);

/// The deviating cases by input: the input, the entry of `openspec/notes/dgraph-bugs-found-in-port.md`, and for
/// each case the variables it is parsed with and the port's error.
///
/// Entry 12: where several variables fail their checks, Go reports one of them, chosen by map iteration order, and
/// its runs report different ones; the port reports each, in the order of their names, joined by newlines as Go's
/// `errors.Join` joins errors. Each error below is the name-ordered join of the errors Go's runs gave.
#[rustfmt::skip]
pub(crate) const DEVIATIONS: &[(&str, u32, &[Deviation])] = &[
    (
        r#"query test($a: int) {
	q(func: uid(0x1)) {
		x as count(uid)
		p : math(x + $a)
	}
}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"
		query versions($version: string = "v0.7.3/beta")
		{
			versions(func:eq(type, "version"))
			{
				versions @filter(eq(version_number, $version))
				{
					version_number
				}
			}
		}
	"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $a not specified\nType of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "0x4e472a"), ("$b", "0x4e9545")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"
query works($since: string = "2018") {
  q(func: has(works_in)) @cascade {
    name
    works_in @facets @facets(gt(since, $since)) {
      name
    }
  }
}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $a not specified\nType of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "0x4e472a"), ("$b", "0x4e9545")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($eff: int) {
		q(func: similar_to(voptions, 4, "[0,0]", ef: $eff)) {
			uid
		}
	}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $a not specified\nType of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "0x4e472a"), ("$b", "0x4e9545")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query foo () { q(func: eq(name, $a)) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $a not specified\nType of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "0x4e472a"), ("$b", "0x4e9545")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query foo ($b: string) { q(func: eq(name, $a)) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $a not specified\nType of variable $after not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $a not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $a not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query foo ($a: string) { q(func: eq(name, $b)) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: uid(0x1), first: $a) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: uid(0x1), offset: $a) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: uid(0x1), orderdesc: name, first: $a) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: uid(0x1), orderdesc: name, offset: $a) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: eq(name, "abc"), orderdesc: name, first: $a) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: eq(name, "abc"), orderdesc: name, offset: $a) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: uid(0x1)) { friend(first: $a) }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: uid(0x1)) { friend(offset: $a) }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: uid(0x1), orderdesc: name) { friend(first: $a) }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: uid(0x1), orderdesc: name) { friend(offset: $a) }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: eq(name, "abc"), orderdesc: name) { friend(first: $a) }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int = 2){ q(func: eq(name, "abc"), orderdesc: name) { friend(offset: $a) }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: int, $b: int, $after: string){
		q(func: uid(0x1), first: $a, offset: $b, after: $after, orderasc: name) {
			friend
		}
	}"#,
        12,
        &[
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nExpected an int but got horseman: strconv.ParseInt: parsing \"horseman\": invalid syntax"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nExpected an int but got horseman: strconv.ParseInt: parsing \"horseman\": invalid syntax\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nExpected an int but got tres: strconv.ParseInt: parsing \"tres\": invalid syntax\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: string){q(func: eq(name, [$a])) {name}}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: string, $b: string){q(func: eq(name, [$a, $b])) {name}}"#,
        12,
        &[
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: string, $b: string, $c: string){q(func: eq(name, [$a, $b, $c])) {name}}"#,
        12,
        &[
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: string){q(func: eq(name, [$a, "mrtrout"])) {name}}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: string){q(func: eq(name, ["mrtrout", $a])) {name}}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: string){q(func: uid_in(director.film, [$a])) {name}}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: string, $b: string){q(func: uid_in(director.film, [$a, $b])) {name}}"#,
        12,
        &[
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: string){q(func: uid_in(name, [$a, "0x4e9545"])) {name}}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query test($a: string){q(func: uid_in(name, ["0x4e9545", $a])) {name}}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query queryWithDQL($schoolUID: string = "5001"){
				me(func: uid(1, 23, 24 )){
					friend @filter(uid_in(school, uid( $schoolUID))) {
						name
					}
				}
			}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $a not specified\nType of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "0x4e472a"), ("$b", "0x4e9545")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"
	query test($a: string, $b: string, $c: string){
		q(func: eq(name, ["uno", $a, $b, "cuatro", $c])) {
			name
		}
	}"#,
        12,
        &[
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query data($a: string = "dgraph")
	{
		data(func: eq(name, "Bob"), first:5) @filter(eq(description, $a)) {
			name
			description
		  }
	}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"query me($a: string) { q(func: uid($a)) { name }}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#" query test($a: int) {
			q(func: uid(0x1)) {
				x as count(uid)
				p : math(x + $a)
			}
		}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Expected an int but got srfrog: strconv.ParseInt: parsing \"srfrog\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Expected an int but got dos: strconv.ParseInt: parsing \"dos\": invalid syntax\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"
	query testQuery($username: String!) {
		me(_xid_: rick) {
			_city
		}
	}"#,
        12,
        &[
            (&[("$a", "3")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "3.3")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$eff", "64")], "Type of variable $eff not specified\nVariable $username should be initialised"),
            (&[("$a", "alice")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "3")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "3")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $a not specified\nType of variable $after not specified\nType of variable $b not specified\nVariable $username should be initialised"),
            (&[("$a", "srfrog")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Type of variable $a not specified\nType of variable $b not specified\nVariable $username should be initialised"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified\nVariable $username should be initialised"),
            (&[("$a", "srfrog")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "srfrog")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "0x4e472a")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "0x4e472a"), ("$b", "0x4e9545")], "Type of variable $a not specified\nType of variable $b not specified\nVariable $username should be initialised"),
            (&[("$a", "0x4e472a")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "0x4e472a")], "Type of variable $a not specified\nVariable $username should be initialised"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified\nVariable $username should be initialised"),
            (&[("$hello", "1")], "Type of variable $hello not specified\nVariable $username should be initialised"),
            (&[("$hello", "true")], "Type of variable $hello not specified\nVariable $username should be initialised"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified\nVariable $username should be initialised"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified\nVariable $username should be initialised"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified\nVariable $username should be initialised"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified\nVariable $username should be initialised"),
            (&[("$a", "   ")], "Type of variable $a not specified\nVariable $username should be initialised"),
        ],
    ),
    (
        r#"
	query testQuery ($username: String, $id: int, $email: string) {
		me(_xid_: rick) {
			_city
		}
	}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $a not specified\nType of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "0x4e472a"), ("$b", "0x4e9545")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
    (
        r#"
	query testQuery ($username: string = abc, $id: int = 5, $email: string) {
		me(_xid_: rick) {
			_city
		}
	}"#,
        12,
        &[
            (&[("$a", "3"), ("$after", "0x123"), ("$b", "5")], "Type of variable $a not specified\nType of variable $after not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$a", "0x4e472a"), ("$b", "0x4e9545")], "Type of variable $a not specified\nType of variable $b not specified"),
            (&[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")], "Type of variable $a not specified\nType of variable $b not specified\nType of variable $c not specified"),
            (&[("$hello", "1"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$_hello_hello", "1"), ("$hello1_heelo1", "true")], "Type of variable $_hello_hello not specified\nType of variable $hello1_heelo1 not specified"),
            (&[("$hello", "sd"), ("$hello1", "true")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
            (&[("$hello", "1"), ("$hello1", "tre")], "Type of variable $hello not specified\nType of variable $hello1 not specified"),
        ],
    ),
];
