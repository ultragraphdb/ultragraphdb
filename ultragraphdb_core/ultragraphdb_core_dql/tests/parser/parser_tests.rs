/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_dql::{Request, VarType, parse, parse_mutation};
use ultragraphdb_core_types::{TypeId, Value};

use crate::test_helpers::{child_attrs, contains, no_err, split, vars};

#[test]
fn test_parse_math_subs() {
    let q = r#"query test($a: int) {
	q(func: uid(0x1)) {
		x as count(uid)
		p : math(x + $a)
	}
}"#;
    let r = vars(q, &[("$a", "3")]);
    let (gq, err) = split(parse(&r));
    no_err(&err);
    let val = &gq.query()[0].children()[1].math_exp().unwrap().child()[1].const_val();
    let _ = &val;
    assert_eq!(val.tid(), TypeId::INT);
    assert_eq!(val.value(), Some(&Value::Int(3)));
    let r = vars(q, &[("$a", "3.3")]);
    let err = parse(&r).err();
    assert!(err.is_some());
    contains(&err, "Expected an int but got 3.3");
}

#[test]
fn test_parse_count_val_error() {
    let query = r#"
{
  me(func: uid(1)) {
    Upvote {
       u as Author
    }
    count(val(u))
  }
}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Count of a variable is not allowed");
}

#[test]
fn test_parse_query_named_query() {
    let query = r#"
query works() {
  q(func: has(name)) {
    name
  }
}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_query_name_query_without_brackers() {
    let query = r#"
query works {
  q(func: has(name)) {
    name
  }
}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_var_error() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			a as friends
		}

		me(func: uid(a)) {
			uid(a)
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Cannot do uid() of a variable");
}

#[test]
fn test_duplicate_query_aliases_error() {
    let query = r#"
{
  find_michael(func: eq(name@., "Michael")) {
    uid
    name@.
    age
  }
    find_michael(func: eq(name@., "Amit")) {
      uid
      name@.
    }
}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    let query_in_op_type = r#"
{
  find_michael(func: eq(name@., "Michael")) {
    uid
    name@.
    age
  }
}
query {find_michael(func: eq(name@., "Amit")) {
      uid
      name@.
    }
}
"#;
    let err = parse(&Request::new(query_in_op_type)).err();
    assert!(err.is_some());
    let query_with_duplicate_shortest_paths = r#"
{
 path as shortest(from: 0x1, to: 0x4) {
  friend
 }
 path2 as shortest(from: 0x2, to: 0x3) {
    friend
 }
 pathQuery1(func: uid(path)) {
   name
 }
 pathQuery2(func: uid(path2)) {
   name
 }

}"#;
    let err = parse(&Request::new(query_with_duplicate_shortest_paths)).err();
    no_err(&err);
}

#[test]
fn test_parse_query_list_pred1() {
    let query = r#"
	{
		var(func: uid( 0x0a)) {
			friends {
				expand(_all_)
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_query_expand_forward() {
    let query = r#"
	{
		var(func: uid( 0x0a)) {
			friends {
				expand(_forward_)
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Argument _forward_ has been deprecated");
}

#[test]
fn test_parse_query_expand_reverse() {
    let query = r#"
	{
		var(func: uid( 0x0a)) {
			friends {
				expand(_reverse_)
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Argument _reverse_ has been deprecated");
}

#[test]
fn test_parse_query_expand_type() {
    let query = r#"
	{
		var(func: uid( 0x0a)) {
			friends {
				expand(Person)
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_query_expand_multiple_types() {
    let query = r#"
	{
		var(func: uid( 0x0a)) {
			friends {
				expand(Person, Relative)
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_query_alias_list_pred() {
    let query = r#"
	{
		me(func: uid(0x0a)) {
			pred: some_pred
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!("pred", res.query()[0].children()[0].alias());
    assert_eq!("some_pred", res.query()[0].children()[0].attr());
}

#[test]
fn test_parse_query_count_list_pred() {
    let query = r#"
	{
		me(func: uid(0x0a)) {
			count(some_pred)
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert!(res.query()[0].children()[0].is_count());
    assert_eq!("some_pred", res.query()[0].children()[0].attr());
}

#[test]
fn test_parse_query_list_pred2() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			f as friends
		}

		var(func: uid(f)) {
			l as some_pred
		}

		var(func: uid( 0x0a)) {
			friends {
				expand(val(l))
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_query_list_pred_multi_var_error() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			f as friends
		}

		var(func: uid(f)) {
			l as some_pred
			friend {
				g as some_pred
			}
		}

		var(func: uid( 0x0a)) {
			friends {
				expand(val(l, g))
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    // Only one variable allowed in expand.
    assert!(err.is_some());
    contains(&err, "Exactly one variable expected");
}

#[test]
fn test_parse_query_with_no_var_val_error() {
    let query = r#"
	{
		me(func: uid(), orderasc: val(n)) {
			name
		}

		var(func: uid(0x0a)) {
			friends {
				n AS name
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_query_agg_child() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			min(friends) {
				name
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Only variables allowed in aggregate functions");
}

#[test]
fn test_parse_query_with_xid_error() {
    let query = r#"
{
      me(func: uid(aliceInWonderland)) {
        type
        writtenIn
        name
        character {
                name
        }
        author {
                name
                born
                died
        }
      }
    }"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Some variables are used but not defined");
    contains(&err, "Used:[aliceInWonderland]");
}

#[test]
fn test_parse_query_with_multi_var_val_error() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(n, d)) {
			name
		}

		var(func: uid(0x0a)) {
			L AS friends {
				n AS name
				d as age
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected only one variable but got: 2");
}

#[test]
fn test_parse_query_with_var_val_agg_err() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(c)) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				c as sumvar()
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected argument but got ')'");
}

#[test]
fn test_parse_query_with_var_val_agg_error1() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d)) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(a + b*c + exp())
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Empty () not allowed in math block");
}

#[test]
/// Go: TestParseQueryWithVarValAgg_Error2, beside TestParseQueryWithVarValAggError2.
fn test_parse_query_with_var_val_agg_error2_underscore() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d)) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(a + b*c+ log())
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unknown math function: log");
}

#[test]
fn test_parse_query_with_var_val_agg_error3() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d)) {
			name
			val(f)
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(a + b*c)
				f as math()
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Empty () not allowed in math block");
}

#[test]
fn test_parse_query_with_var_val_agg_nested() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d)) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(a + b*c)
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    assert_eq!(
        "(+ a (* b c))",
        res.query()[1].children()[0].children()[3]
            .math_exp()
            .unwrap()
            .debug_string()
    );
    no_err(&err);
}

#[test]
fn test_parse_query_with_var_val_agg_nested2() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d)) {
			name
			val(q)
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(exp(a + b + 1.0) - ln(c))
				q as math(c*-1.0+-b+(-b*c))
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "(- (exp (+ (+ a b) 1E+00)) (ln c))",
        res.query()[1].children()[0].children()[3]
            .math_exp()
            .unwrap()
            .debug_string()
    );
    assert_eq!(
        "(+ (+ (* c (u- 1E+00)) (u- b)) (* (u- b) c))",
        res.query()[1].children()[0].children()[4]
            .math_exp()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_query_with_var_val_agg_nested4() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d) ) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(exp(a + b + 1.0) - max(c,ln(c)) + sqrt(a%b))
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "(+ (- (exp (+ (+ a b) 1E+00)) (max c (ln c))) (sqrt (% a b)))",
        res.query()[1].children()[0].children()[3]
            .math_exp()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_query_with_var_val_agg_nested5() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d) ) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(a * b / c)
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "(/ (* a b) c)",
        res.query()[1].children()[0].children()[3]
            .math_exp()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_query_with_var_val_agg_log_sqrt() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d) ) {
			name
			val(e)
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				d as math(ln(sqrt(a)))
				e as math(sqrt(ln(a)))
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "(ln (sqrt a))",
        res.query()[1].children()[0].children()[1]
            .math_exp()
            .unwrap()
            .debug_string()
    );
    assert_eq!(
        "(sqrt (ln a))",
        res.query()[1].children()[0].children()[2]
            .math_exp()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_query_with_var_val_dot_product() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d) ) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as vfloat
				b as vfloat
				c as count(relatives)
				d as math(a dot b * c)
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "(dot a (* b c))",
        res.query()[1].children()[0].children()[3]
            .math_exp()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_query_with_var_val_dot_product2() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d) ) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as vfloat
				b as vfloat
				c as count(relatives)
				d as math(a dot b / c)
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "(dot a (/ b c))",
        res.query()[1].children()[0].children()[3]
            .math_exp()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_query_with_var_val_agg_nested_conditional() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d) ) {
			name
			val(f)
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(cond(a <= 10.0, exp(a + b + 1.0), ln(c)) + 10*a)
				e as math(cond(a!=10.0, exp(a + b + 1.0), ln(d)))
				f as math(cond(a==10.0, exp(a + b + 1.0), ln(e)))
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "(+ (cond (<= a 1E+01) (exp (+ (+ a b) 1E+00)) (ln c)) (* 10 a))",
        res.query()[1].children()[0].children()[3]
            .math_exp()
            .unwrap()
            .debug_string()
    );
    assert_eq!(
        "(cond (!= a 1E+01) (exp (+ (+ a b) 1E+00)) (ln d))",
        res.query()[1].children()[0].children()[4]
            .math_exp()
            .unwrap()
            .debug_string()
    );
    assert_eq!(
        "(cond (== a 1E+01) (exp (+ (+ a b) 1E+00)) (ln e))",
        res.query()[1].children()[0].children()[5]
            .math_exp()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_query_with_var_val_agg_nested3() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d) ) {
			name
	}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(a + b * c / a + exp(a + b + 1.0) - ln(c))
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "(+ (+ a (/ (* b c) a)) (- (exp (+ (+ a b) 1E+00)) (ln c)))",
        res.query()[1].children()[0].children()[3]
            .math_exp()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_query_with_var_val_agg_nested_error1() {
    // No args to mulvar.
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d)) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				d as math(a + *)
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected 2 operands");
}

#[test]
fn test_parse_query_with_var_val_agg_nested_error2() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(d)) {
			name
		}

		var(func: uid(0x0a)) {
			L as friends {
				a as age
				b as count(friends)
				c as count(relatives)
				d as math(a +b*c -)
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected 2 operands");
}

#[test]
fn test_parse_query_with_level_agg() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			friends {
				a as count(age)
			}
			s as sum(val(a))
		}

		sumage(func: uid( 0x0a)) {
			val(s)
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
    assert_eq!("a", res.query()[0].children()[0].children()[0].var());
    assert!(res.query()[0].children()[1].is_internal());
    assert_eq!("a", res.query()[0].children()[1].needs_var()[0].name());
    assert_eq!(
        VarType::Value,
        res.query()[0].children()[1].needs_var()[0].typ()
    );
    assert_eq!("s", res.query()[0].children()[1].var());
}

#[test]
fn test_parse_query_with_var_val_agg_combination() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(c) ) {
			name
			val(c)
		}

		var(func: uid(0x0a)) {
			L as friends {
				x as age
			}
			a as min(val(x))
			b as max(val(x))
			c as math(a + b)
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
    assert_eq!("L", res.query()[0].needs_var()[0].name());
    assert_eq!(VarType::Uid, res.query()[0].needs_var()[0].typ());
    assert_eq!("c", res.query()[0].needs_var()[1].name());
    assert_eq!(VarType::Value, res.query()[0].needs_var()[1].typ());
    assert_eq!("c", res.query()[0].order()[0].attr);
    assert_eq!("name", res.query()[0].children()[0].attr());
    assert_eq!("val", res.query()[0].children()[1].attr());
    assert_eq!(1, res.query()[0].children()[1].needs_var().len());
    assert_eq!("c", res.query()[0].children()[1].needs_var()[0].name());
    assert_eq!("L", res.query()[1].children()[0].var());
    assert_eq!("a", res.query()[1].children()[1].var());
    assert_eq!("b", res.query()[1].children()[2].var());
    assert_eq!("c", res.query()[1].children()[3].var());
    let _ = &res.query()[1].children()[3].math_exp().unwrap();
    assert_eq!(
        "+",
        res.query()[1].children()[3].math_exp().unwrap().fn_name()
    );
    assert_eq!(
        "a",
        res.query()[1].children()[3].math_exp().unwrap().child()[0].var()
    );
    assert_eq!(
        "b",
        res.query()[1].children()[3].math_exp().unwrap().child()[1].var()
    );
}

#[test]
fn test_parse_query_with_var_val_agg() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(n) ) {
			name
		}

		var(func: uid(0x0a)) {
			L AS friends {
				na as name
			}
			n as min(val(na))
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
    assert_eq!("L", res.query()[0].needs_var()[0].name());
    assert_eq!(VarType::Uid, res.query()[0].needs_var()[0].typ());
    assert_eq!("n", res.query()[0].needs_var()[1].name());
    assert_eq!(VarType::Value, res.query()[0].needs_var()[1].typ());
    assert_eq!("n", res.query()[0].order()[0].attr);
    assert_eq!("name", res.query()[0].children()[0].attr());
    assert_eq!("L", res.query()[1].children()[0].var());
    assert_eq!("na", res.query()[1].children()[0].children()[0].var());
    assert_eq!("n", res.query()[1].children()[1].var());
    assert_eq!("min", res.query()[1].children()[1].func().unwrap().name());
}

#[test]
fn test_parse_query_with_var_val_agg_error() {
    let query = r#"
	{
		me(func: uid(L), orderasc: uid(n)) {
			name
		}

		var(func: uid(0x0a)) {
			L AS friends {
				na as name
			}
			n as min(val(na))
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected val(). Got uid() with order.");
}

#[test]
fn test_parse_query_with_var_val_agg_error2() {
    let query = r#"
	{
		me(func: val(L), orderasc: val(n)) {
			name
		}

		var(func: uid(0x0a)) {
			L AS friends {
				na as name
			}
			n as min(val(na))
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Function name: val is not valid.");
}

#[test]
fn test_parse_query_with_var_val_count() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(n) ) {
			name
		}

		var(func: uid(0x0a)) {
			L AS friends {
				n AS count(friend)
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
    assert_eq!("L", res.query()[0].needs_var()[0].name());
    assert_eq!(VarType::Uid, res.query()[0].needs_var()[0].typ());
    assert_eq!("n", res.query()[0].needs_var()[1].name());
    assert_eq!(VarType::Value, res.query()[0].needs_var()[1].typ());
    assert_eq!("n", res.query()[0].order()[0].attr);
    assert_eq!("name", res.query()[0].children()[0].attr());
    assert_eq!("L", res.query()[1].children()[0].var());
    assert!(res.query()[1].children()[0].children()[0].is_count());
}

#[test]
fn test_parse_query_with_var_val() {
    let query = r#"
	{
		me(func: uid(L), orderasc: val(n) ) {
			name
		}

		var(func: uid(0x0a)) {
			L AS friends {
				n AS name
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
    assert_eq!("L", res.query()[0].needs_var()[0].name());
    assert_eq!(VarType::Uid, res.query()[0].needs_var()[0].typ());
    assert_eq!("n", res.query()[0].needs_var()[1].name());
    assert_eq!(VarType::Value, res.query()[0].needs_var()[1].typ());
    assert_eq!("n", res.query()[0].order()[0].attr);
    assert_eq!("name", res.query()[0].children()[0].attr());
    assert_eq!("L", res.query()[1].children()[0].var());
    assert_eq!("n", res.query()[1].children()[0].children()[0].var());
}

#[test]
fn test_parse_query_with_var_multi_root() {
    let query = r#"
	{
		me(func: uid( L, J, K)) {name}
		var(func: uid(0x0a)) {L AS friends}
		var(func: uid(0x0a)) {J AS friends}
		var(func: uid(0x0a)) {K AS friends}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(4, res.query().len());
    assert_eq!("L", res.query()[0].needs_var()[0].name());
    assert_eq!("J", res.query()[0].needs_var()[1].name());
    assert_eq!("K", res.query()[0].needs_var()[2].name());
    assert_eq!(VarType::Uid, res.query()[0].needs_var()[0].typ());
    assert_eq!(VarType::Uid, res.query()[0].needs_var()[1].typ());
    assert_eq!(VarType::Uid, res.query()[0].needs_var()[2].typ());
    assert_eq!("L", res.query()[1].children()[0].var());
    assert_eq!("J", res.query()[2].children()[0].var());
    assert_eq!("K", res.query()[3].children()[0].var());
}

#[test]
fn test_parse_query_with_var() {
    let query = r#"
	{
		me(func: uid(L)) {name}
		him(func: uid(J)) {name}
		you(func: uid(K)) {name}
		var(func: uid(0x0a)) {L AS friends}
		var(func: uid(0x0a)) {J AS friends}
		var(func: uid(0x0a)) {K AS friends}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(6, res.query().len());
    assert_eq!("L", res.query()[0].needs_var()[0].name());
    assert_eq!("J", res.query()[1].needs_var()[0].name());
    assert_eq!("K", res.query()[2].needs_var()[0].name());
    assert_eq!(VarType::Uid, res.query()[0].needs_var()[0].typ());
    assert_eq!(VarType::Uid, res.query()[1].needs_var()[0].typ());
    assert_eq!(VarType::Uid, res.query()[2].needs_var()[0].typ());
    assert_eq!("L", res.query()[3].children()[0].var());
    assert_eq!("J", res.query()[4].children()[0].var());
    assert_eq!("K", res.query()[5].children()[0].var());
}

#[test]
fn test_parse_query_with_var_error1() {
    let query = r#"
	{
		him(func: uid(J)) {name}
		you(func: uid(K)) {name}
		var(func: uid(0x0a)) {L AS friends}
		var(func: uid(0x0a)) {J AS friends}
		var(func: uid(0x0a)) {K AS friends}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Some variables are defined but not used");
}

#[test]
fn test_parse_query_with_var_error2() {
    let query = r#"
	{
		me(func: uid(L)) {name}
		him(func: uid(J)) {name}
		you(func: uid(K)) {name}
		var(func: uid(0x0a)) {L AS friends}
		var(func: uid(0x0a)) {K AS friends}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Some variables are used but not defined");
}

#[test]
fn test_parse_query_filter_error1_a() {
    let query = r#"
	{
		me(func: uid(1) @filter(anyof(name, "alice"))) {
		 name
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "\"@\"");
}

#[test]
fn test_parse_query_filter_error1_b() {
    let query = r#"
	{
		me(func: uid(1)) @filter(anyofterms(name"alice")) {
		 name
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected comma or language but got: \"alice\"");
}

#[test]
fn test_parse_query_filter_error2() {
    let query = r#"
	{
		me(func: uid(1)) @filter(anyofterms(name "alice")) {
		 name
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected comma or language but got: \"alice\"");
}

#[test]
fn test_parse_query_with_var_at_root_filter_id() {
    let query = r#"
	{
		K as var(func: uid(0x0a)) {
			L AS friends
		}
		me(func: uid(K)) @filter(uid(L)) {
		 name
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
    assert_eq!("K", res.query()[0].var());
    assert_eq!("L", res.query()[0].children()[0].var());
    assert_eq!(
        "L",
        res.query()[1].filter().unwrap().func().unwrap().needs_var()[0].name()
    );
    assert_eq!(
        VarType::Uid,
        res.query()[1].filter().unwrap().func().unwrap().needs_var()[0].typ()
    );
    assert_eq!(vec!["K", "L"], res.query_vars()[0].defines());
}

#[test]
fn test_parse_query_with_var_at_root() {
    let query = r#"
	{
		K AS var(func: uid(0x0a)) {
			fr as friends
		}
		me(func: uid(fr)) @filter(uid(K)) {
		 name	@filter(uid(fr))
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
    assert_eq!("K", res.query()[0].var());
    assert_eq!("fr", res.query()[0].children()[0].var());
    assert_eq!("fr", res.query()[1].needs_var()[0].name());
    assert_eq!(VarType::Uid, res.query()[1].needs_var()[0].typ());
    assert_eq!(vec!["K", "fr"], res.query_vars()[0].defines());
}

#[test]
fn test_parse_query_with_var_in_ineq_error() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			fr as friends {
				a as age
			}
		}

		me(func: uid(fr)) @filter(gt(val(a, b), 10)) {
		 name
		}
	}
"#;
    // Multiple vars not allowed.
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Multiple variables not allowed in a function");
}

#[test]
fn test_len_function_with_multiple_variable_error() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			fr as friends {
				a as age
			}
		}

		me(func: uid(fr)) @filter(gt(len(a, b), 10)) {
		 name
		}
	}
"#;
    // Multiple vars not allowed.
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Multiple variables not allowed in len function");
}

#[test]
fn test_len_function_inside_uid_error() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			fr as friends {
				a as age
			}
		}

		me(func: uid(fr)) @filter(uid(len(a), 10)) {
			name
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "len function only allowed inside inequality");
}

#[test]
fn test_len_function_with_no_variable() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			fr as friends {
				a as age
			}
		}

		me(func: uid(fr)) @filter(len(), 10) {
			name
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Got empty attr for function");
}

#[test]
fn test_len_as_second_argument_error() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			fr as friends {
				a as age
			}
		}

		me(func: uid(fr)) @filter(10, len(fr)) {
			name
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    // TODO(pawan) - Error message can be improved. We should validate function names from a
    // whitelist.
    assert!(err.is_some());
}

#[test]
fn test_count_with_len_function_error() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			fr as friends {
				a as age
			}
		}

		me(func: uid(fr)) @filter(count(name), len(fr)) {
			name
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    // TODO(pawan) - Error message can be improved.
    assert!(err.is_some());
}

#[test]
fn test_parse_query_with_var_in_ineq() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			fr as friends {
				a as age
			}
		}

		me(func: uid(fr)) @filter(gt(val(a), 10)) {
		 name
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
    assert_eq!("fr", res.query()[0].children()[0].var());
    assert_eq!("fr", res.query()[1].needs_var()[0].name());
    assert_eq!(VarType::Uid, res.query()[1].needs_var()[0].typ());
    assert_eq!(
        VarType::Value,
        res.query()[1].filter().unwrap().func().unwrap().needs_var()[0].typ()
    );
    assert_eq!(
        1,
        res.query()[1]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()
            .len()
    );
    assert_eq!("a", res.query()[1].filter().unwrap().func().unwrap().attr());
    assert!(
        res.query()[1]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .is_value_var()
    );
    assert_eq!(
        "10",
        res.query()[1].filter().unwrap().func().unwrap().args()[0].value()
    );
    assert!(!res.query()[1].filter().unwrap().func().unwrap().args()[0].is_value_var());
    assert_eq!(
        "gt",
        res.query()[1].filter().unwrap().func().unwrap().name()
    );
}

#[test]
fn test_parse_query_with_var1() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			L AS friends
		}

		me(func: uid(L)) {
		 name
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
    assert_eq!("L", res.query()[0].children()[0].var());
    assert_eq!("L", res.query()[1].needs_var()[0].name());
    assert_eq!(VarType::Uid, res.query()[1].needs_var()[0].typ());
}

#[test]
fn test_parse_query_with_multiple_var() {
    let query = r#"
	{
		var(func: uid(0x0a)) {
			L AS friends {
				B AS relatives
			}
		}

		me(func: uid(L)) {
		 name
		}

		relatives(func: uid(B)) {
			name
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(3, res.query().len());
    assert_eq!("L", res.query()[0].children()[0].var());
    assert_eq!("B", res.query()[0].children()[0].children()[0].var());
    assert_eq!("L", res.query()[1].needs_var()[0].name());
    assert_eq!("B", res.query()[2].needs_var()[0].name());
    assert_eq!(VarType::Uid, res.query()[1].needs_var()[0].typ());
    assert_eq!(VarType::Uid, res.query()[2].needs_var()[0].typ());
    assert_eq!(vec!["L", "B"], res.query_vars()[0].defines());
    assert_eq!(vec!["L"], res.query_vars()[1].needs());
    assert_eq!(vec!["B"], res.query_vars()[2].needs());
}

#[test]
fn test_parse_shortest_path() {
    let query = r#"
	{
		shortest(from:0x0a, to:0x0b, numpaths: 3, minweight: 3, maxweight: 6, maxfrontiersize: 1) {
			friends
			name
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    assert_eq!(
        0xau64,
        res.query()[0].shortest_path_args().from().unwrap().uid()[0]
    );
    assert_eq!(
        0xbu64,
        res.query()[0].shortest_path_args().to().unwrap().uid()[0]
    );
    assert_eq!(
        "3",
        res.query()[0]
            .args()
            .get("numpaths")
            .map_or("", String::as_str)
    );
    assert_eq!(
        "3",
        res.query()[0]
            .args()
            .get("minweight")
            .map_or("", String::as_str)
    );
    assert_eq!(
        "6",
        res.query()[0]
            .args()
            .get("maxweight")
            .map_or("", String::as_str)
    );
    assert_eq!(
        "1",
        res.query()[0]
            .args()
            .get("maxfrontiersize")
            .map_or("", String::as_str)
    );
}

#[test]
fn test_parse_shortest_path_with_uid_vars() {
    let query = r#"{
		a as var(func: uid(0x01))
		b as var(func: uid(0x02))

		shortest(from: uid(a), to: uid(b)) {
			password
			friend
		}

	}"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let q = &res.query()[2];
    let _ = &q.shortest_path_args().from().unwrap();
    assert_eq!(1, q.shortest_path_args().from().unwrap().needs_var().len());
    assert_eq!(
        "a",
        q.shortest_path_args().from().unwrap().needs_var()[0].name()
    );
    assert_eq!("uid", q.shortest_path_args().from().unwrap().name());
    let _ = &q.shortest_path_args().to().unwrap();
    assert_eq!(1, q.shortest_path_args().to().unwrap().needs_var().len());
}

#[test]
fn test_parse_shortest_path_invalid_fn_error() {
    let query = r#"{
		shortest(from: eq(a), to: uid(b)) {
			password
			friend
		}

	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
}

#[test]
fn test_parse_multiple_queries() {
    let query = r#"
	{
		you(func: uid(0x0a)) {
			name
		}

		me(func: uid(0x0b)) {
		 friends
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(2, res.query().len());
}

#[test]
fn test_parse_root_args1() {
    let query = r#"
	query {
		me(func: uid(0x0a), first: -4, offset: +1) {
			friends {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    assert_eq!(2, res.query()[0].args().len());
    assert_eq!(
        "-4",
        res.query()[0]
            .args()
            .get("first")
            .map_or("", String::as_str)
    );
    assert_eq!(
        "+1",
        res.query()[0]
            .args()
            .get("offset")
            .map_or("", String::as_str)
    );
    assert_eq!(
        child_attrs(&res.query()[0]),
        vec!["friends", "gender", "age", "hometown"]
    );
    assert_eq!(child_attrs(&res.query()[0].children()[0]), vec!["name"]);
}

#[test]
fn test_parse_root_args2() {
    let query = r#"
	query {
		me(func: uid(0x0a), first: 1, offset:0) {
			friends {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    assert_eq!(2, res.query()[0].args().len());
    assert_eq!(
        "1",
        res.query()[0]
            .args()
            .get("first")
            .map_or("", String::as_str)
    );
    assert_eq!(
        "0",
        res.query()[0]
            .args()
            .get("offset")
            .map_or("", String::as_str)
    );
    assert_eq!(
        child_attrs(&res.query()[0]),
        vec!["friends", "gender", "age", "hometown"]
    );
    assert_eq!(child_attrs(&res.query()[0].children()[0]), vec!["name"]);
}

#[test]
fn test_parse() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    assert_eq!(
        child_attrs(&res.query()[0]),
        vec!["friends", "gender", "age", "hometown"]
    );
    assert_eq!(child_attrs(&res.query()[0].children()[0]), vec!["name"]);
}

#[test]
fn test_parse_error() {
    let query = r#"
		me(func: uid(0x0a)) {
			friends {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Invalid operation type: me");
}

#[test]
fn test_parse_xid() {
    let query = r#"
	query {
		user(func: uid( 0x11)) {
			type.object.name
		}
	}"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(child_attrs(&res.query()[0]), vec!["type.object.name"]);
}

#[test]
fn test_parse_id_list() {
    let query = r#"
	query {
		user(func: uid(0x1)) {
			type.object.name
		}
	}"#;
    let (r, err) = split(parse(&Request::new(query)));
    let gq = &r.query()[0];
    no_err(&err);
    let _ = &gq;
    assert_eq!(vec!["type.object.name"], child_attrs(gq));
    //	require.Equal(t, []uint64{0x1}, gq.UID)
}

#[test]
fn test_parse_id_list1() {
    let query = r#"
	query {
		user(func: uid(0x1, 0x34)) {
			type.object.name
		}
	}"#;
    let (r, err) = split(parse(&Request::new(query)));
    let gq = &r.query()[0];
    no_err(&err);
    let _ = &gq;
    assert_eq!(vec!["type.object.name"], child_attrs(gq));
    assert_eq!(vec![0x1, 0x34], gq.uid());
    assert_eq!(2, gq.uid().len());
}

#[test]
fn test_parse_id_list_error() {
    let query = r#"
	query {
		user(func: uid( [0x1, 0x1, abc, ade, 0x34))] {
			type.object.name
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unrecognized character in lexText: U+005D ']'");
}

#[test]
fn test_parse_id_list_error2() {
    let query = r#"
	query {
		user(func: uid( [0x1, 0x1, 2, 3, 0x34])) {
			type.object.name
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unexpected character [ while parsing request.");
}

#[test]
fn test_parse_first() {
    let query = r#"
	query {
		user(func: uid( 0x1)) {
			type.object.name
			friends (first: 10) {
			}
		}
	}"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["type.object.name", "friends"],
        child_attrs(&res.query()[0])
    );
    assert_eq!(
        "10",
        res.query()[0].children()[1]
            .args()
            .get("first")
            .map_or("", String::as_str)
    );
}

#[test]
fn test_parse_first_error() {
    let query = r#"
	query {
		user(func: uid( 0x1)) {
			type.object.name
			friends (first: ) {
			}
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expecting argument value");
    contains(&err, "\")\"");
}

#[test]
fn test_parse_after() {
    let query = r#"
	query {
		user(func: uid( 0x1)) {
			type.object.name
			friends (first: 10, after: 3) {
			}
		}
	}"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        child_attrs(&res.query()[0]),
        vec!["type.object.name", "friends"]
    );
    assert_eq!(
        res.query()[0].children()[1]
            .args()
            .get("first")
            .map_or("", String::as_str),
        "10"
    );
    assert_eq!(
        res.query()[0].children()[1]
            .args()
            .get("after")
            .map_or("", String::as_str),
        "3"
    );
}

#[test]
fn test_parse_offset() {
    let query = r#"
	query {
		user(func: uid( 0x1)) {
			type.object.name
			friends (first: 10, offset: 3) {
			}
		}
	}"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        child_attrs(&res.query()[0]),
        vec!["type.object.name", "friends"]
    );
    assert_eq!(
        res.query()[0].children()[1]
            .args()
            .get("first")
            .map_or("", String::as_str),
        "10"
    );
    assert_eq!(
        res.query()[0].children()[1]
            .args()
            .get("offset")
            .map_or("", String::as_str),
        "3"
    );
}

#[test]
fn test_parse_offset_error() {
    let query = r#"
	query {
		user(func: uid( 0x1)) {
			type.object.name
			friends (first: 10, offset: ) {
			}
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expecting argument value");
    contains(&err, "\")\"");
}

#[test]
fn test_parse_error2() {
    let query = r#"
		query {
			me {
				name
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected Left round brackets");
    contains(&err, "\"{\"");
}

#[test]
fn test_parse_pass1() {
    let query = r#"
		{
			me(func: uid(0x0a)) {
				name,
				friends(xid:what) {  # xid would be ignored.
				}
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Got invalid keyword: xid");
}

#[test]
fn test_parse_alias_count() {
    let query = r#"
		{
			me(func: uid(0x0a)) {
				name,
				bestFriend: friends(first: 10) {
					nameCount: count(name)
				}
			}
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(child_attrs(&res.query()[0]), vec!["name", "friends"]);
    assert_eq!(res.query()[0].children()[1].alias(), "bestFriend");
    assert_eq!(child_attrs(&res.query()[0].children()[1]), vec!["name"]);
    assert_eq!(
        "nameCount",
        res.query()[0].children()[1].children()[0].alias()
    );
}

#[test]
fn test_parse_alias_var() {
    let query = r#"
		{
			me(func: uid(0x0a)) {
				name,
				f as bestFriend: friends(first: 10) {
					c as count(friend)
				}
			}

			friend(func: uid(f)) {
				name
				fcount: val(c)
			}
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(child_attrs(&res.query()[0]), vec!["name", "friends"]);
    assert_eq!(res.query()[0].children()[1].alias(), "bestFriend");
    assert_eq!("fcount", res.query()[1].children()[1].alias());
}

#[test]
fn test_parse_alias_max() {
    let query = r#"
		{
			me(func: uid(0x0a)) {
				name,
				bestFriend: friends(first: 10) {
					x as count(friends)
				}
				maxfriendcount: max(val(x))
			}
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(res.query()[0].children()[1].alias(), "bestFriend");
    assert_eq!("maxfriendcount", res.query()[0].children()[2].alias());
}

#[test]
fn test_parse_alias() {
    let query = r#"
		{
			me(func: uid(0x0a)) {
				name,
				bestFriend: friends(first: 10) {
					name
				}
			}
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(child_attrs(&res.query()[0]), vec!["name", "friends"]);
    assert_eq!(res.query()[0].children()[1].alias(), "bestFriend");
    assert_eq!(child_attrs(&res.query()[0].children()[1]), vec!["name"]);
}

#[test]
fn test_parse_alias1() {
    let query = r#"
		{
			me(func: uid(0x0a)) {
				name: type.object.name.en
				bestFriend: friends(first: 10) {
					name: type.object.name.hi
				}
			}
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        child_attrs(&res.query()[0]),
        vec!["type.object.name.en", "friends"]
    );
    assert_eq!(res.query()[0].children()[1].alias(), "bestFriend");
    assert_eq!(res.query()[0].children()[1].children()[0].alias(), "name");
    assert_eq!(
        child_attrs(&res.query()[0].children()[1]),
        vec!["type.object.name.hi"]
    );
}

#[test]
fn test_parse_bad_alias() {
    let query = r#"
		{
			me(func: uid(0x0a)) {
				name: type.object.name.en: after_colon
				bestFriend: friends(first: 10) {
					name: type.object.name.hi
				}
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Invalid colon after alias declaration");
}

#[test]
fn test_parse_block() {
    let query = r#"
		{
			root(func: uid( 0x0a)) {
				type.object.name.es.419
			}
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        child_attrs(&res.query()[0]),
        vec!["type.object.name.es.419"]
    );
}

#[test]
fn test_parse_schema() {
    let query = r#"
		schema (pred : name) {
			pred
			type
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(res.schema().unwrap().predicates[0], "name");
    assert_eq!(res.schema().unwrap().fields.len(), 2);
    assert_eq!(res.schema().unwrap().fields[0], "pred");
    assert_eq!(res.schema().unwrap().fields[1], "type");
}

#[test]
fn test_parse_schema_multi() {
    let query = r#"
		schema (pred : [name,hi]) {
			pred
			type
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(res.schema().unwrap().predicates.len(), 2);
    assert_eq!(res.schema().unwrap().predicates[0], "name");
    assert_eq!(res.schema().unwrap().predicates[1], "hi");
    assert_eq!(res.schema().unwrap().fields.len(), 2);
    assert_eq!(res.schema().unwrap().fields[0], "pred");
    assert_eq!(res.schema().unwrap().fields[1], "type");
}

#[test]
fn test_parse_schema_all() {
    let query = r#"
		schema {
			pred
			type
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(res.schema().unwrap().predicates.len(), 0);
    assert_eq!(res.schema().unwrap().fields.len(), 2);
    assert_eq!(res.schema().unwrap().fields[0], "pred");
    assert_eq!(res.schema().unwrap().fields[1], "type");
}

#[test]
fn test_parse_schema_with_comments() {
    let query = r#"
		schema (pred : name) {
			#hi
			pred #bye
			type
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(res.schema().unwrap().predicates[0], "name");
    assert_eq!(res.schema().unwrap().fields.len(), 2);
    assert_eq!(res.schema().unwrap().fields[0], "pred");
    assert_eq!(res.schema().unwrap().fields[1], "type");
}

#[test]
fn test_parse_schema_and_query() {
    let query1 = r#"
		schema {
			pred
			type
		}
		query {
			me(func: uid( tomhanks)) {
				name
				hometown
			}
		}
	"#;
    let query2 = r#"
		query {
			me(func: uid( tomhanks)) {
				name
				hometown
			}
		}
		schema {
			pred
			type
		}
	"#;
    let err = parse(&Request::new(query1)).err();
    assert!(err.is_some());
    contains(&err, "Schema block is not allowed with query block");
    let err = parse(&Request::new(query2)).err();
    assert!(err.is_some());
    contains(&err, "Schema block is not allowed with query block");
}

#[test]
fn test_parse_schema_type() {
    let query = r#"
		schema (type: Person) {
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(res.schema().unwrap().predicates.len(), 0);
    assert_eq!(res.schema().unwrap().types.len(), 1);
    assert_eq!(res.schema().unwrap().types[0], "Person");
    assert_eq!(res.schema().unwrap().fields.len(), 0);
}

#[test]
fn test_parse_schema_type_multi() {
    let query = r#"
		schema (type: [Person, Animal]) {
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(res.schema().unwrap().predicates.len(), 0);
    assert_eq!(res.schema().unwrap().types.len(), 2);
    assert_eq!(res.schema().unwrap().types[0], "Person");
    assert_eq!(res.schema().unwrap().types[1], "Animal");
    assert_eq!(res.schema().unwrap().fields.len(), 0);
}

#[test]
fn test_parse_schema_special_chars() {
    let query = r#"
		schema (pred: [Person, <人物>]) {
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(res.schema().unwrap().predicates.len(), 2);
    assert_eq!(res.schema().unwrap().types.len(), 0);
    assert_eq!(res.schema().unwrap().predicates[0], "Person");
    assert_eq!(res.schema().unwrap().predicates[1], "人物");
    assert_eq!(res.schema().unwrap().fields.len(), 0);
}

#[test]
fn test_parse_schema_type_special_chars() {
    let query = r#"
		schema (type: [Person, <人物>]) {
		}
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(res.schema().unwrap().predicates.len(), 0);
    assert_eq!(res.schema().unwrap().types.len(), 2);
    assert_eq!(res.schema().unwrap().types[0], "Person");
    assert_eq!(res.schema().unwrap().types[1], "人物");
    assert_eq!(res.schema().unwrap().fields.len(), 0);
}

#[test]
fn test_parse_schema_error() {
    let query = r#"
		schema () {
			pred
			type
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Invalid schema block");
}

#[test]
fn test_parse_schema_error_multi() {
    let query = r#"
		schema {
			pred
			type
		}
		schema {
			pred
			type
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Only one schema block allowed");
}

#[test]
fn test_parse_mutation_error() {
    let query = r#"
		mutation {
			set {
				<name> <is> <something> .
				<hometown> <is> <san/francisco> .
			}
			delete {
				<name> <is> <something-else> .
			}
		}
	"#;
    let err = parse_mutation(query).err();
    assert!(err.is_some());
    contains(&err, r#"Unexpected token: [mutation]"#);
}

#[test]
fn test_parse_mutation_error2() {
    let query = r#"
			set {
				<name> <is> <something> .
				<hometown> <is> <san/francisco> .
			}
			delete {
				<name> <is> <something-else> .
			}
	"#;
    let err = parse_mutation(query).err();
    assert!(err.is_some());
    contains(&err, r#"Invalid operation type: set"#);
}

#[test]
fn test_parse_mutation_and_query_with_comments() {
    let query = r#"
	# Mutation
		mutation {
			# Set block
			set {
				<name> <is> <something> .
				<hometown> <is> <san/francisco> .
			}
			# Delete block
			delete {
				<name> <is> <something-else> .
			}
		}
		# Query starts here.
		query {
			me(func: uid( 0x5)) { # now mention children
				name		# Name
				hometown # hometown of the person
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
}

#[test]
fn test_parse_fragment_multi_query() {
    let query = r#"
	{
		user(func: uid(0x0a)) {
			...fragmenta,...fragmentb
			friends {
				name
			}
			...fragmentc
			hobbies
			...fragmentd
		}

		me(func: uid(0x01)) {
			...fragmenta
			...fragmentb
		}
	}

	fragment fragmenta {
		name
	}

	fragment fragmentb {
		id
	}

	fragment fragmentc {
		name
	}

	fragment fragmentd {
		id
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["name", "id", "friends", "name", "hobbies", "id"],
        child_attrs(&res.query()[0])
    );
    assert_eq!(vec!["name", "id"], child_attrs(&res.query()[1]));
}

#[test]
fn test_parse_fragment_no_nesting() {
    let query = r#"
	query {
		user(func: uid(0x0a)) {
			...fragmenta,...fragmentb
			friends {
				name
			}
			...fragmentc
			hobbies
			...fragmentd
		}
	}

	fragment fragmenta {
		name
	}

	fragment fragmentb {
		id
	}

	fragment fragmentc {
		name
	}

	fragment fragmentd {
		id
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        child_attrs(&res.query()[0]),
        vec!["name", "id", "friends", "name", "hobbies", "id"]
    );
}

#[test]
fn test_parse_fragment_nest1() {
    let query = r#"
	query {
		user(func: uid(0x0a)) {
			...fragmenta
			friends {
				name
			}
		}
	}

	fragment fragmenta {
		id
		...fragmentb
	}

	fragment fragmentb {
		hobbies
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        child_attrs(&res.query()[0]),
        vec!["id", "hobbies", "friends"]
    );
}

#[test]
fn test_parse_fragment_nest2() {
    let query = r#"
	query {
		user(func: uid(0x0a)) {
			friends {
				...fragmenta
			}
		}
	}
	fragment fragmenta {
		name
		...fragmentb
	}
	fragment fragmentb {
		nickname
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(child_attrs(&res.query()[0]), vec!["friends"]);
    assert_eq!(
        child_attrs(&res.query()[0].children()[0]),
        vec!["name", "nickname"]
    );
}

#[test]
fn test_parse_fragment_cycle() {
    let query = r#"
	query {
		user(func: uid(0x0a)) {
			...fragmenta
		}
	}
	fragment fragmenta {
		name
		...fragmentb
	}
	fragment fragmentb {
		...fragmentc
	}
	fragment fragmentc {
		id
		...fragmenta
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Cycle detected");
}

#[test]
fn test_parse_fragment_missing() {
    let query = r#"
	query {
		user(func: uid(0x0a)) {
			...fragmenta
		}
	}
	fragment fragmentb {
		...fragmentc
	}
	fragment fragmentc {
		id
		...fragmenta
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Missing fragment: fragmenta");
}

#[test]
fn test_parse_string_var_in_filter() {
    let query = r#"
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
	"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        "v0.7.3/beta",
        res.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[0]
            .value()
    );
}

#[test]
fn test_parse_var_in_facet() {
    let query = r#"
query works($since: string = "2018") {
  q(func: has(works_in)) @cascade {
    name
    works_in @facets @facets(gt(since, $since)) {
      name
    }
  }
}"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        "2018",
        res.query()[0].children()[1]
            .facets_filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[0]
            .value()
    );
}

#[test]
fn test_parse_variables_error1() {
    let query = r#"
	query testQuery($a: string, $b: int!){
			root(func: uid( 0x0a)) {
				type.object.name.es-419
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unrecognized character in lexText: U+0034 '4'");
}

#[test]
fn test_parse_filter_root() {
    let query = r#"
	query {
		me(func:anyofterms(abc, "Abc")) @filter(allofterms(name, "alice")) {
			friends @filter() {
				name @filter(namefilter(name, "a"))
			}
			gender @filter(eq(g, "a")),age @filter(neq(a, "b"))
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    let _ = &res.query()[0].filter().unwrap();
    assert_eq!(
        r#"(allofterms name "alice")"#,
        res.query()[0].filter().unwrap().debug_string()
    );
    assert_eq!(
        vec!["friends", "gender", "age", "hometown"],
        child_attrs(&res.query()[0])
    );
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert!(res.query()[0].children()[0].filter().is_none());
    assert_eq!(
        r#"(eq g "a")"#,
        res.query()[0].children()[1]
            .filter()
            .unwrap()
            .debug_string()
    );
    assert_eq!(
        r#"(neq a "b")"#,
        res.query()[0].children()[2]
            .filter()
            .unwrap()
            .debug_string()
    );
    assert_eq!(
        r#"(namefilter name "a")"#,
        res.query()[0].children()[0].children()[0]
            .filter()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_func_nested() {
    let query = r#"
	query {
		me(func: gt(count(friend), 10)) {
			friends @filter() {
				name
			}
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    let _ = &res.query()[0].func().unwrap();
    assert_eq!(res.query()[0].func().unwrap().name(), "gt");
    assert_eq!(res.query()[0].func().unwrap().args()[0].value(), "10");
    assert!(res.query()[0].func().unwrap().is_count());
}

#[test]
fn test_parse_func_nested2() {
    let query = r#"
	query {
		var(func:uid(1)) {
			a as name
		}
		me(func: eq(name, val(a))) {
			friends @filter() {
				name
			}
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[1];
    let _ = &res.query()[1].func().unwrap();
    assert_eq!(res.query()[1].func().unwrap().name(), "eq");
    assert_eq!(res.query()[1].func().unwrap().args()[0].value(), "a");
    assert!(res.query()[1].func().unwrap().args()[0].is_value_var());
    assert!(!res.query()[1].func().unwrap().is_count());
}

#[test]
fn test_parse_filter_root2() {
    let query = r#"
	query {
		me(func:anyofterms(abc, "Abc")) @filter(gt(count(friends), 10)) {
			friends @filter() {
				name
			}
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    let _ = &res.query()[0].filter().unwrap();
    assert_eq!(
        r#"(gt count(friends) "10")"#,
        res.query()[0].filter().unwrap().debug_string()
    );
    assert_eq!(vec!["friends", "hometown"], child_attrs(&res.query()[0]));
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert!(res.query()[0].children()[0].filter().is_none());
}

#[test]
fn test_parse_filter_root_error2() {
    // filter-by-count only support first argument as function
    let query = r#"
	query {
		me(func:anyofterms(abc, "Abc")) @filter(gt(count(friends), sum(friends))) {
			friends @filter() {
				name
			}
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Multiple functions as arguments not allowed");
}

#[test]
fn test_parse_filter_simplest() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter() {
				name @filter(namefilter(name, "a"))
			}
			gender @filter(eq(g, "a")),age @filter(neq(a, "b"))
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "gender", "age", "hometown"],
        child_attrs(&res.query()[0])
    );
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert!(res.query()[0].children()[0].filter().is_none());
    assert_eq!(
        r#"(eq g "a")"#,
        res.query()[0].children()[1]
            .filter()
            .unwrap()
            .debug_string()
    );
    assert_eq!(
        r#"(neq a "b")"#,
        res.query()[0].children()[2]
            .filter()
            .unwrap()
            .debug_string()
    );
    assert_eq!(
        r#"(namefilter name "a")"#,
        res.query()[0].children()[0].children()[0]
            .filter()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_filter_op() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(a(aa, "aaa") or b(bb, "bbb")
			and c(cc, "ccc")) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "gender", "age", "hometown"],
        child_attrs(&res.query()[0])
    );
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert_eq!(
        r#"(OR (a aa "aaa") (AND (b bb "bbb") (c cc "ccc")))"#,
        res.query()[0].children()[0]
            .filter()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_filter_op_error1() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(a(aa "aaa") or b(b "bbb")) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected comma or language but got: \"aaa\"");
}

#[test]
fn test_parse_filter_op_no_error2() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(a(aa, "aaa") or b(b, "bbb")) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
    // It's valid.  Makes sure TestParseFilter_opError3 fails for the expected reason.
}

#[test]
fn test_parse_filter_op_error3() {
    let query = r#"
		query {
		me(func: uid(0x0a)) {
			friends @filter(a(aa, "aaa") or b(b, "bbb") and) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Invalid filter statement");
}

#[test]
fn test_parse_filter_op_not1() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(not a(aa, "aaa")) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "gender", "age", "hometown"],
        child_attrs(&res.query()[0])
    );
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert_eq!(
        r#"(NOT (a aa "aaa"))"#,
        res.query()[0].children()[0]
            .filter()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_filter_op_not2() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(not(a(aa, "aaa") or (b(bb, "bbb"))) and c(cc, "ccc")) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "gender", "age", "hometown"],
        child_attrs(&res.query()[0])
    );
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert_eq!(
        r#"(AND (NOT (OR (a aa "aaa") (b bb "bbb"))) (c cc "ccc"))"#,
        res.query()[0].children()[0]
            .filter()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_filter_op2() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter((a(aa, "aaa") Or b(bb, "bbb"))
			 and c(cc, "ccc")) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "gender", "age", "hometown"],
        child_attrs(&res.query()[0])
    );
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert_eq!(
        r#"(AND (OR (a aa "aaa") (b bb "bbb")) (c cc "ccc"))"#,
        res.query()[0].children()[0]
            .filter()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_filter_brac() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(  a(name, "hello") or b(name, "world", "is") and (c(aa, "aaa") or (d(dd, "haha") or e(ee, "aaa"))) and f(ff, "aaa")){
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "gender", "age", "hometown"],
        child_attrs(&res.query()[0])
    );
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert_eq!(
        r#"(OR (a name "hello") (AND (AND (b name "world" "is") (OR (c aa "aaa") (OR (d dd "haha") (e ee "aaa")))) (f ff "aaa")))"#,
        res.query()[0].children()[0]
            .filter()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_parse_filter_unbalancedbrac() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(  () {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unclosed Brackets");
}

#[test]
fn test_parse_similar_to_named_params() {
    let query = r#"{
		q(func: similar_to(voptions, 4, "[0,0]", distance_threshold: 1.5, ef: 12)) {
			uid
		}
	}"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(res.query().len(), 1);
    let _ = &res.query()[0];
    let _ = &res.query()[0].func().unwrap();
    assert_eq!("similar_to", res.query()[0].func().unwrap().name());
    assert_eq!("voptions", res.query()[0].func().unwrap().attr());
    assert_eq!("4", res.query()[0].func().unwrap().args()[0].value());
    assert_eq!("[0,0]", res.query()[0].func().unwrap().args()[1].value());
    // Options are appended as (key, value) pairs after k and vec.
    assert_eq!(res.query()[0].func().unwrap().args().len(), 6);
    assert_eq!(
        "distance_threshold",
        res.query()[0].func().unwrap().args()[2].value()
    );
    assert_eq!("1.5", res.query()[0].func().unwrap().args()[3].value());
    assert_eq!("ef", res.query()[0].func().unwrap().args()[4].value());
    assert_eq!("12", res.query()[0].func().unwrap().args()[5].value());
}

#[test]
fn test_parse_similar_to_three_args() {
    // Test three-arg form (no options)
    let query = r#"{
		q(func: similar_to(voptions, 4, "[0,0]")) {
			uid
		}
	}"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!("similar_to", res.query()[0].func().unwrap().name());
    assert_eq!(res.query()[0].func().unwrap().args().len(), 2);
}

#[test]
fn test_parse_similar_to_rejects_object_literal_syntax() {
    let query = r#"{
		q(func: similar_to(voptions, 4, "[0,0]", {ef: 12})) {
			uid
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unrecognized character inside a func: U+007B '{'");
}

#[test]
fn test_parse_similar_to_with_query_variable() {
    let query = r#"query test($eff: int) {
		q(func: similar_to(voptions, 4, "[0,0]", ef: $eff)) {
			uid
		}
	}"#;
    let (res, err) = split(parse(&vars(query, &[("$eff", "64")])));
    no_err(&err);
    assert_eq!("similar_to", res.query()[0].func().unwrap().name());
    assert_eq!(res.query()[0].func().unwrap().args().len(), 4);
    assert_eq!("ef", res.query()[0].func().unwrap().args()[2].value());
    assert_eq!("64", res.query()[0].func().unwrap().args()[3].value());
}

#[test]
fn test_parse_similar_to_rejects_legacy_string_options_syntax() {
    let query = r#"{
		q(func: similar_to(voptions, 4, "[0,0]", "ef=64,distance_threshold=0.45")) {
			uid
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected named parameter in similar_to options");
}

#[test]
fn test_parse_similar_to_unknown_option() {
    let query = r#"{
		q(func: similar_to(voptions, 4, "[0,0]", foo: 5)) {
			uid
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unknown option");
    contains(&err, "foo");
}

#[test]
fn test_parse_similar_to_duplicate_option() {
    let query = r#"{
		q(func: similar_to(voptions, 4, "[0,0]", ef: 10, ef: 20)) {
			uid
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Duplicate key");
    contains(&err, "ef");
}

#[test]
fn test_parse_non_similar_to_with_brace() {
    // Braces in non-similar_to functions should be rejected
    let query = r#"{
		q(func: eq(name, {value: "test"})) {
			uid
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unrecognized character inside a func: U+007B '{'");
}

#[test]
fn test_parse_filter_geo1() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(near(loc, [-1.12 , 2.0123 ], 100.123 )) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (resp, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "[-1.12,2.0123]",
        resp.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[0]
            .value()
    );
    assert_eq!(
        "100.123",
        resp.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[1]
            .value()
    );
    assert!(
        !resp.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[0]
            .is_value_var()
    );
    assert!(
        !resp.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[1]
            .is_value_var()
    );
}

#[test]
fn test_parse_filter_geo2() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(within(loc, [[11.2 , -2.234 ], [ -31.23, 4.3214] , [5.312, 6.53]] )) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (resp, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "[[11.2,-2.234],[-31.23,4.3214],[5.312,6.53]]",
        resp.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[0]
            .value()
    );
}

#[test]
fn test_parse_filter_geo3() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(near(loc, [[1 , 2 ], [[3, 4] , [5, 6]] )) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Found invalid item");
}

#[test]
fn test_parse_filter_geo4() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(near(loc, [[1 , 2 ], [3, 4] , [5, 6]]] )) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected right round or comma");
    contains(&err, "\"]\"");
}

#[test]
fn test_parse_filter_emptyargument() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(allofterms(name,,)) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Consecutive commas not allowed");
}

#[test]
fn test_parse_filter_unknowndirective_error1() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filtererr {
				name
			}
			gender,age
			hometown
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    // We can't differentiate between @filtererr being a directive or a language. As we don't
    // see a () after it we assume its a language but attr which specify a language can't have
    // children.
    // The test below tests for unknown directive.
    contains(
        &err,
        "Cannot have children for attr: friends with lang tags:",
    );
}

#[test]
fn test_parse_filter_unknowndirective_error2() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filtererr ()
			gender,age
			hometown
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unknown directive [filtererr]");
}

#[test]
fn test_parse_generator_error1() {
    let query = r#"{
		me(allofterms(name, "barack")) {
			friends {
				name
			}
			gender,age
			hometown
			count(friends)
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Got invalid keyword: allofterms");
}

#[test]
fn test_parse_generator_error2() {
    let query = r#"{
		me(func: allofterms(name, "barack")) {
			friends(all: 5) {
				name
			}
			gender,age
			hometown
			count(friends)
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Got invalid keyword: all");
}

#[test]
fn test_parse_quoted_function_attribute_error() {
    let query = r#"{
		me(func: allofterms("name", "barack")) {
			friends {
				name
			}
			gender,age
			hometown
			count(friends)
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Attribute in function must not be quoted");
}

#[test]
fn test_parse_count_as_func_multiple() {
    let query = r#"{
		me(func: uid(1)) {
			count(friends), count(relatives)
			count(classmates)
			gender,age
			hometown
		}
	}
"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(6, gq.query()[0].children().len());
    assert!(gq.query()[0].children()[0].is_count());
    assert_eq!("friends", gq.query()[0].children()[0].attr());
    assert!(gq.query()[0].children()[1].is_count());
    assert_eq!("relatives", gq.query()[0].children()[1].attr());
    assert!(gq.query()[0].children()[2].is_count());
    assert_eq!("classmates", gq.query()[0].children()[2].attr());
}

#[test]
fn test_parse_count_as_func_multiple_error() {
    let query = r#"{
		me(func: uid(1)) {
			count(friends, relatives
			classmates)
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Multiple predicates not allowed in single count");
}

#[test]
fn test_parse_count_as_func() {
    let query = r#"{
		me(func: uid(1)) {
			count(friends)
			gender,age
			hometown
		}
	}
"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert!(gq.query()[0].children()[0].is_count());
    assert_eq!(4, gq.query()[0].children().len());
}

#[test]
fn test_parse_count_error1() {
    let query = r#"{
		me(func: uid(1)) {
			count(friends
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unclosed Brackets");
}

#[test]
fn test_parse_count_error2() {
    let query = r#"{
		me(func: uid(1)) {
			count((friends)
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unclosed Brackets");
}

#[test]
fn test_parse_check_pwd() {
    let query = r#"{
		me(func: uid(1)) {
			checkpwd(password, "123456")
			hometown
		}
	}
"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        "checkpwd",
        gq.query()[0].children()[0].func().unwrap().name()
    );
    assert_eq!(
        "123456",
        gq.query()[0].children()[0].func().unwrap().args()[0].value()
    );
    assert_eq!("password", gq.query()[0].children()[0].attr());
}

#[test]
fn test_parse_comments() {
    let query = r#"
	# Something
	{
		me(func:allofterms(name, "barack")) {
			friends {
				name
			} # Something
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_comments1() {
    let query = r#"{
		#Something
		me(func:allofterms(name, "barack")) {
			friends {
				name  # Name of my friend
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_generator() {
    let query = r#"{
		me(func:allofterms(name, "barack")) {
			friends {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_iri_ref() {
    let query = r#"{
		me(func: uid( 0x1)) {
			<http://verygood.com/what/about/you>
			friends @filter(allofterms(<http://verygood.com/what/about/you>,
				"good better bad")){
				name
			}
			gender,age
			hometown
		}
	}"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(5, gq.query()[0].children().len());
    assert_eq!(
        "http://verygood.com/what/about/you",
        gq.query()[0].children()[0].attr()
    );
    assert_eq!(
        r#"(allofterms http://verygood.com/what/about/you "good better bad")"#,
        gq.query()[0].children()[1].filter().unwrap().debug_string()
    );
}

#[test]
fn test_parse_iri_ref2() {
    let query = r#"{
		me(func:anyofterms(<http://helloworld.com/how/are/you>, "good better bad")) {
			<http://verygood.com/what/about/you>
			friends @filter(allofterms(<http://verygood.com/what/about/you>,
				"good better bad")){
				name
			}
		}
	}"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(2, gq.query()[0].children().len());
    assert_eq!(
        "http://verygood.com/what/about/you",
        gq.query()[0].children()[0].attr()
    );
    assert_eq!(
        r#"(allofterms http://verygood.com/what/about/you "good better bad")"#,
        gq.query()[0].children()[1].filter().unwrap().debug_string()
    );
    assert_eq!(
        "http://helloworld.com/how/are/you",
        gq.query()[0].func().unwrap().attr()
    );
}

#[test]
fn test_parse_iri_ref_space() {
    let query = r#"{
		me(func: uid( <http://helloworld.com/how/are/ you>)) {
		}
	      }"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unexpected character ' ' while parsing IRI");
}

#[test]
fn test_parse_iri_ref_invalid_char() {
    let query = r#"{
		me(func: uid( <http://helloworld.com/how/are/^you>)) {
		}
	      }"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unexpected character '^' while parsing IRI");
}

#[test]
fn test_langs() {
    let query = r#"
	query {
		me(func: uid(1)) {
			name@en,name@en:ru:hu
		}
	}
	"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(2, gq.query()[0].children().len());
    assert_eq!("name", gq.query()[0].children()[0].attr());
    assert_eq!(vec!["en"], gq.query()[0].children()[0].langs());
    assert_eq!("name", gq.query()[0].children()[1].attr());
    assert_eq!(vec!["en", "ru", "hu"], gq.query()[0].children()[1].langs());
}

#[test]
fn test_all_langs() {
    let query = r#"
	query {
		me(func: uid(1)) {
			name@*
		}
	}
	"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(1, gq.query()[0].children().len());
    assert_eq!("name", gq.query()[0].children()[0].attr());
    assert_eq!(vec!["*"], gq.query()[0].children()[0].langs());
}

#[test]
fn test_langs_invalid1() {
    let query = r#"
	query {
		me(func: uid(1)) {
			name@en@ru
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected directive or language list, got @ru");
}

#[test]
fn test_langs_invalid2() {
    let query = r#"
	query {
		me(func: uid(1)) {
			@en:ru
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Invalid use of directive.");
}

#[test]
fn test_langs_invalid3() {
    let query = r#"
	query {
		me(func: uid(1)) {
			name@en:ru, @en:ru
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected directive or language list, got @en");
}

#[test]
fn test_langs_invalid4() {
    let query = r#"
	query {
		me(func: uid(1)) {
			name@
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unrecognized character in lexDirective: U+000A");
}

#[test]
fn test_langs_invalid5() {
    let query = r#"
	query {
		me(func: uid(1)) {
			name@<something.wrong>
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unrecognized character in lexDirective: U+003C '<'");
}

#[test]
fn test_langs_invalid6() {
    let query = r#"
		{
			me(func: uid(0x1004)) {
				name@hi:cn:...
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Expected only one dot(.) while parsing language list.",
    );
}

#[test]
fn test_langs_invalid7() {
    let query = r#"
		{
			me(func: uid(0x1004)) {
				name@...
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Expected only one dot(.) while parsing language list.",
    );
}

#[test]
fn test_langs_invalid8() {
    let query = r#"
	query {
		me(func: uid(1)) {
			name@*:en
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "If * is used, no other languages are allowed in the language list",
    );
}

#[test]
fn test_langs_invalid9() {
    let query = r#"
	query {
		me(func: eqs(name@*, "Amir")) {
			name@en
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "The * symbol cannot be used as a valid language inside functions",
    );
}

#[test]
fn test_langs_invalid10() {
    let query = r#"
	query {
		me(func: uid(1)) {
			name@.:*
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "If * is used, no other languages are allowed in the language list",
    );
}

#[test]
fn test_langs_filter() {
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(alloftext(descr@en, "something")) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    let _ = &res.query()[0].children()[0];
    let _ = &res.query()[0].children()[0].filter().unwrap();
    let _ = &res.query()[0].children()[0]
        .filter()
        .unwrap()
        .func()
        .unwrap();
    assert_eq!(
        "descr",
        res.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .attr()
    );
    assert_eq!(
        "en",
        res.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .lang()
    );
}

#[test]
fn test_langs_filter_error1() {
    // this query should fail, because '@lang' is used twice (and only one appearance is allowed)
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(alloftext(descr@en@de, "something")) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Invalid usage of '@' in function argument");
}

#[test]
fn test_langs_filter_error2() {
    // this query should fail, because there is no lang after '@'
    let query = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(alloftext(descr@, "something")) {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unrecognized character in lexDirective: U+002C ','");
}

#[test]
fn test_langs_function() {
    let query = r#"
	query {
		me(func:alloftext(descr@en, "something")) {
			friends {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    let _ = &res.query()[0].func().unwrap();
    assert_eq!("descr", res.query()[0].func().unwrap().attr());
    assert_eq!("en", res.query()[0].func().unwrap().lang());
}

#[test]
fn test_langs_function_multiple_langs() {
    let query = r#"
	query {
		me(func:alloftext(descr@hi:en, "something")) {
			friends {
				name
			}
			gender,age
			hometown
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected arg after func [alloftext]");
    contains(&err, "\":\"");
}

#[test]
fn test_parse_normalize() {
    let query = r#"
	query {
		me(func: uid( 0x3)) @normalize {
			friends {
				name
			}
			gender
			hometown
		}
}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert!(res.query()[0].normalize());
}

#[test]
fn test_parse_groupby_root() {
    let query = r#"
	query {
		me(func: uid(1, 2, 3)) @groupby(friends) {
				a as count(uid)
		}

		groups(func: uid(a)) {
			uid
			val(a)
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(1, res.query()[0].groupby_attrs().len());
    assert_eq!("friends", res.query()[0].groupby_attrs()[0].attr());
    assert_eq!("a", res.query()[0].children()[0].var());
}

#[test]
fn test_parse_groupby_with_count_var() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @groupby(friends) {
				a as count(uid)
			}
			hometown
			age
		}

		groups(func: uid(a)) {
			uid
			val(a)
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(1, res.query()[0].children()[0].groupby_attrs().len());
    assert_eq!(
        "friends",
        res.query()[0].children()[0].groupby_attrs()[0].attr()
    );
    assert_eq!("a", res.query()[0].children()[0].children()[0].var());
}

#[test]
fn test_parse_groupby_with_max_var() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @groupby(friends) {
				a as max(first-name@en:ta)
			}
			hometown
			age
		}

		groups(func: uid(a)) {
			uid
			val(a)
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(1, res.query()[0].children()[0].groupby_attrs().len());
    assert_eq!(
        "friends",
        res.query()[0].children()[0].groupby_attrs()[0].attr()
    );
    assert_eq!(
        "first-name",
        res.query()[0].children()[0].children()[0].attr()
    );
    assert_eq!(
        vec!["en", "ta"],
        res.query()[0].children()[0].children()[0].langs()
    );
    assert_eq!("a", res.query()[0].children()[0].children()[0].var());
}

#[test]
fn test_parse_groupby() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @groupby(name@en) {
				count(uid)
			}
			hometown
			age
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(1, res.query()[0].children()[0].groupby_attrs().len());
    assert_eq!(
        "name",
        res.query()[0].children()[0].groupby_attrs()[0].attr()
    );
    assert_eq!(
        "en",
        res.query()[0].children()[0].groupby_attrs()[0].langs()[0]
    );
}

#[test]
fn test_parse_groupby_with_alias() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @groupby(name) {
				GroupCount: count(uid)
			}
			hometown
			age
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(1, res.query()[0].children()[0].groupby_attrs().len());
    assert_eq!(
        "name",
        res.query()[0].children()[0].groupby_attrs()[0].attr()
    );
    assert_eq!(
        "GroupCount",
        res.query()[0].children()[0].children()[0].alias()
    );
}

#[test]
fn test_parse_groupby_with_alias_for_key() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @groupby(Name: name, SchooL: school) {
				count(uid)
			}
			hometown
			age
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(2, res.query()[0].children()[0].groupby_attrs().len());
    assert_eq!(
        "Name",
        res.query()[0].children()[0].groupby_attrs()[0].alias()
    );
    assert_eq!(
        "SchooL",
        res.query()[0].children()[0].groupby_attrs()[1].alias()
    );
}

#[test]
fn test_parse_groupby_with_alias_for_error() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @groupby(first: 10, SchooL: school) {
				count(uid)
			}
			hometown
			age
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    contains(&err, "Can't use keyword first as alias in groupby");
}

#[test]
fn test_parse_groupby_error() {
    // predicates not allowed inside groupby.
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @groupby(name) {
				name
				count(uid)
			}
			hometown
			age
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Only aggregator/count functions allowed inside @groupby",
    );
}

#[test]
fn test_parse_facets_error1() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets {
				name @facets(facet1,, facet2)
			}
			hometown
			age
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Consecutive commas not allowed.");
}

#[test]
fn test_parse_facets_var_error() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets {
				name @facets(facet1, b as)
			}
			hometown
			age
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected name in facet list");
}

#[test]
fn test_parse_facets_error2() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets {
				name @facets(facet1 facet2)
			}
			hometown
			age
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected ( after func name [facet1]");
}

#[test]
fn test_parse_facets_order_error1() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(orderdesc: orderdesc: closeness) {
				name
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected ( after func name [orderdesc]");
}

#[test]
fn test_parse_facets_order_error2() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(a as b as closeness) {
				name
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected ( after func name [a]");
}

#[test]
fn test_parse_facets_order_with_alias() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(orderdesc: closeness, b as some, order: abc, key, key1: val, abcd) {
				val(b)
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let node = &res.query()[0].children()[0].facets().unwrap();
    assert_eq!(6, node.param.len());
    assert_eq!("order", node.param[0].alias);
    assert_eq!("abc", node.param[0].key);
    assert_eq!("abcd", node.param[1].key);
    assert_eq!("val", node.param[5].key);
    assert_eq!("key1", node.param[5].alias);
}

#[test]
fn test_parse_facets_duplicate_var_error() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(a as closeness, b as closeness) {
				name
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Duplicate variable mappings");
}

#[test]
fn test_parse_facets_order_var() {
    let query = r#"
	query {
		me1(func: uid(0x1)) {
			friends @facets(orderdesc: a as b) {
				name
			}
		}
		me2(func: uid(a)) { }
	}
"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_parse_facets_order_var2() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(a as orderdesc: b) {
				name
			}
		}
		me(func: uid(a)) {

		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Expected ( after func name [a]");
}

#[test]
fn test_parse_facets() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(orderdesc: closeness) {
				name
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(vec!["friends"], child_attrs(&res.query()[0]));
    let _ = &res.query()[0].children()[0].facets().unwrap();
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert_eq!(
        "closeness",
        res.query()[0].children()[0].facets_order()[0].key()
    );
    assert!(res.query()[0].children()[0].facets_order()[0].desc());
}

#[test]
fn test_parse_orderby_multiple_facets() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(orderdesc: closeness, orderasc: since) {
				name
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(vec!["friends"], child_attrs(&res.query()[0]));
    let _ = &res.query()[0].children()[0].facets().unwrap();
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert_eq!(2, res.query()[0].children()[0].facets_order().len());
    assert_eq!(
        "closeness",
        res.query()[0].children()[0].facets_order()[0].key()
    );
    assert!(res.query()[0].children()[0].facets_order()[0].desc());
    assert_eq!(
        "since",
        res.query()[0].children()[0].facets_order()[1].key()
    );
    assert!(!res.query()[0].children()[0].facets_order()[1].desc());
}

#[test]
fn test_parse_orderby_multiple_facets_with_alias() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(orderdesc: closeness, orderasc: since, score, location:from) {
				name
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(vec!["friends"], child_attrs(&res.query()[0]));
    let _ = &res.query()[0].children()[0].facets().unwrap();
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    assert_eq!(2, res.query()[0].children()[0].facets_order().len());
    assert_eq!(
        "closeness",
        res.query()[0].children()[0].facets_order()[0].key()
    );
    assert!(res.query()[0].children()[0].facets_order()[0].desc());
    assert_eq!(
        "since",
        res.query()[0].children()[0].facets_order()[1].key()
    );
    assert!(!res.query()[0].children()[0].facets_order()[1].desc());
    assert_eq!(
        4,
        res.query()[0].children()[0].facets().unwrap().param.len()
    );
    assert!(res.query()[0].children()[0].facets_filter().is_none());
    assert!(res.query()[0].children()[0].facet_var().unwrap().is_empty());
    if let Some(param) = res.query()[0].children()[0]
        .facets()
        .unwrap()
        .param
        .iter()
        .find(|p| p.key == "from")
    {
        assert_eq!("location", param.alias);
    }
}

#[test]
fn test_parse_orderby_same_facets_multiple_times() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(orderdesc: closeness, orderasc: closeness) {
				name
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    contains(&err, "Sorting by facet: [closeness] can only be done once");
}

#[test]
fn test_parse_orderby_facet() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets {
				name @facets(facet1)
			}
			hometown
			age
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "hometown", "age"],
        child_attrs(&res.query()[0])
    );
    let _ = &res.query()[0].children()[0].facets().unwrap();
    assert!(res.query()[0].children()[0].facets().unwrap().all_keys);
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    let _ = &res.query()[0].children()[0].children()[0].facets().unwrap();
    assert!(
        !res.query()[0].children()[0].children()[0]
            .facets()
            .unwrap()
            .all_keys
    );
    assert_eq!(
        "facet1",
        res.query()[0].children()[0].children()[0]
            .facets()
            .unwrap()
            .param[0]
            .key
    );
}

#[test]
fn test_parse_facets_multiple() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets {
				name @facets(key1, key2, key3)
			}
			hometown
			age
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "hometown", "age"],
        child_attrs(&res.query()[0])
    );
    let _ = &res.query()[0].children()[0].facets().unwrap();
    assert!(res.query()[0].children()[0].facets().unwrap().all_keys);
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    let _ = &res.query()[0].children()[0].children()[0].facets().unwrap();
    assert!(
        !res.query()[0].children()[0].children()[0]
            .facets()
            .unwrap()
            .all_keys
    );
    assert_eq!(
        3,
        res.query()[0].children()[0].children()[0]
            .facets()
            .unwrap()
            .param
            .len()
    );
}

#[test]
fn test_parse_facets_alias() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets {
				name @facets(a1: key1, a2: key2, a3: key3)
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    let _ = &res.query()[0].children()[0].facets().unwrap();
    assert!(res.query()[0].children()[0].facets().unwrap().all_keys);
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    let _ = &res.query()[0].children()[0].children()[0].facets().unwrap();
    let node = &res.query()[0].children()[0].children()[0].facets().unwrap();
    assert!(!node.all_keys);
    assert_eq!(3, node.param.len());
    assert_eq!("a1", node.param[0].alias);
    assert_eq!("key1", node.param[0].key);
    assert_eq!("a3", node.param[2].alias);
    assert_eq!("key3", node.param[2].key);
}

#[test]
fn test_parse_facets_multiple_var() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets {
				name @facets(a as key1, key2, b as key3)
			}
			hometown
			age
		}
		h(func: uid(a, b)) {
			uid
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "hometown", "age"],
        child_attrs(&res.query()[0])
    );
    let _ = &res.query()[0].children()[0].facets().unwrap();
    assert!(res.query()[0].children()[0].facets().unwrap().all_keys);
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    let _ = &res.query()[0].children()[0].children()[0].facets().unwrap();
    assert!(
        !res.query()[0].children()[0].children()[0]
            .facets()
            .unwrap()
            .all_keys
    );
    assert_eq!(
        3,
        res.query()[0].children()[0].children()[0]
            .facets()
            .unwrap()
            .param
            .len()
    );
    assert_eq!(
        "a",
        res.query()[0].children()[0].children()[0]
            .facet_var()
            .unwrap()
            .get("key1")
            .map_or("", String::as_str)
    );
    assert_eq!(
        "",
        res.query()[0].children()[0].children()[0]
            .facet_var()
            .unwrap()
            .get("key2")
            .map_or("", String::as_str)
    );
    assert_eq!(
        "b",
        res.query()[0].children()[0].children()[0]
            .facet_var()
            .unwrap()
            .get("key3")
            .map_or("", String::as_str)
    );
}

#[test]
fn test_parse_facets_multiple_repeat() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets {
				name @facets(key1, key2, key3, key1)
			}
			hometown
			age
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "hometown", "age"],
        child_attrs(&res.query()[0])
    );
    let _ = &res.query()[0].children()[0].facets().unwrap();
    assert!(res.query()[0].children()[0].facets().unwrap().all_keys);
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    let _ = &res.query()[0].children()[0].children()[0].facets().unwrap();
    assert!(
        !res.query()[0].children()[0].children()[0]
            .facets()
            .unwrap()
            .all_keys
    );
    assert_eq!(
        3,
        res.query()[0].children()[0].children()[0]
            .facets()
            .unwrap()
            .param
            .len()
    );
}

#[test]
fn test_parse_facets_empty() {
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets() {
			}
			hometown
			age
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(
        vec!["friends", "hometown", "age"],
        child_attrs(&res.query()[0])
    );
    let _ = &res.query()[0].children()[0].facets().unwrap();
    assert!(!res.query()[0].children()[0].facets().unwrap().all_keys);
    assert_eq!(
        0,
        res.query()[0].children()[0].facets().unwrap().param.len()
    );
}

#[test]
fn test_parse_facets_fail1() {
    // key can not be empty..
    let query = r#"
	query {
		me(func: uid(0x1)) {
			friends @facets(key1,, key2) {
			}
			hometown
			age
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Consecutive commas not allowed.");
}

#[test]
fn test_parse_repeat_args_error1() {
    // key can not be empty..
    let query = r#"
	{
  		me(func: anyoftext(Text, "biology"), func: anyoftext(Text, "science")) {
			Text
  		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Only one function allowed at root");
}

#[test]
fn test_parse_repeat_args_error2() {
    // key can not be empty..
    let query = r#"
	{
  		me(func: anyoftext(Text, "science")) {
			Text(first: 1, first: 4)
  		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Got repeated key \"first\"");
}

#[test]
fn test_facets_filter_simple() {
    // all friends of 0x1 who are close to him
    let query = r#"
	       {
		me(func: uid(0x1)) {
			name
			friend @facets(eq(close, true)) {
				name
				gender
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(vec!["name", "friend"], child_attrs(&res.query()[0]));
    assert!(res.query()[0].children()[1].facets().is_none());
    let _ = &res.query()[0].children()[1].facets_filter().unwrap();
    assert_eq!(
        r#"(eq close "true")"#,
        res.query()[0].children()[1]
            .facets_filter()
            .unwrap()
            .debug_string()
    );
}

#[test]
fn test_facets_filter_all() {
    // all friends of 0x1 who are close to him or are in his family
    let query = r#"
	       {
		me(func: uid(0x1)) {
			name
			friend @facets(eq(close, true) or eq(family, true)) @facets(close, family, since) {
				name @facets
				gender
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query()[0];
    assert_eq!(vec!["name", "friend"], child_attrs(&res.query()[0]));
    let _ = &res.query()[0].children()[1].facets().unwrap();
    assert_eq!(
        "close",
        res.query()[0].children()[1].facets().unwrap().param[0].key
    );
    assert_eq!(
        "family",
        res.query()[0].children()[1].facets().unwrap().param[1].key
    );
    assert_eq!(
        "since",
        res.query()[0].children()[1].facets().unwrap().param[2].key
    );
    let _ = &res.query()[0].children()[1].facets_filter().unwrap();
    assert_eq!(
        r#"(OR (eq close "true") (eq family "true"))"#,
        res.query()[0].children()[1]
            .facets_filter()
            .unwrap()
            .debug_string()
    );
    assert_eq!(
        vec!["name", "gender"],
        child_attrs(&res.query()[0].children()[1])
    );
    let name_child = &res.query()[0].children()[1].children()[0];
    let _ = &name_child;
    let _ = &name_child.facets().unwrap();
    assert!(name_child.facets_filter().is_none());
    let gender_child = &res.query()[0].children()[1].children()[1];
    let _ = &gender_child;
    assert!(gender_child.facets().is_none());
    assert!(gender_child.facets_filter().is_none());
}

#[test]
fn test_facets_filter_fail() {
    // multiple @facets and @facets(close, since) are not allowed.
    let query = r#"
	       {
		me(func: uid(0x1)) {
			name
			friend @facets @facets(close, since) {
				name
				gender
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Only one facets allowed");
}

#[test]
fn test_facets_filter_fail2() {
    // multiple facets-filter not allowed
    let query = r#"
	       {
		me(func: uid(0x1)) {
			name
			friend @facets(eq(close, true)) @facets(eq(family, true)) {
				name
				gender
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Only one facets filter allowed");
}

#[test]
fn test_facets_filter_fail3() {
    // vars are not allowed in facets filtering.
    let query = r#"
	{
		K as var(func: uid(0x0a)) {
			L AS friends
		}
		me(func: uid(K)) {
			friend @facets(uid(L)) {
				name
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "variables are not allowed in facets filter");
}

#[test]
fn test_facets_filter_fail_root() {
    let query = r#"
	{
		me(func: uid(0x1)) @facets(eq(some-facet, true)) {
			friend	{
				name
			}
		}
	}
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unknown directive [facets]");
}

#[test]
fn test_facets_filter_at_value() {
    // dql parses facets at value level as well.
    let query = r#"
	{
		me(func: uid(0x1)) {
			friend	{
				name @facets(eq(some.facet, true))
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(vec!["name"], child_attrs(&res.query()[0].children()[0]));
    let name_child = &res.query()[0].children()[0].children()[0];
    let _ = &name_child;
    let _ = &name_child.facets_filter().unwrap();
    assert_eq!(
        r#"(eq some.facet "true")"#,
        name_child.facets_filter().unwrap().debug_string()
    );
}

#[test]
fn test_parse_query_with_attr_lang() {
    let query = r#"
	{
		me(func: uid(0x1)) {
			name
			friend(first:5, orderasc: name@en) {
				name@en
			}
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    assert_eq!("name", res.query()[0].children()[1].order()[0].attr);
    assert_eq!(vec!["en"], res.query()[0].children()[1].order()[0].langs);
}

#[test]
fn test_parse_query_with_attr_lang2() {
    let query = r#"
	{
	  me(func:regexp(name, /^[a-zA-z]*[^Kk ]?[Nn]ight/), orderasc: name@en, first:5) {
		name@en
		name@de
		name@it
	  }
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    assert_eq!("name", res.query()[0].order()[0].attr);
    assert_eq!(vec!["en"], res.query()[0].order()[0].langs);
}

#[test]
fn test_parse_regexp1() {
    let query = r#"
	{
	  me(func: uid(0x1)) {
	    name
		friend @filter(regexp(name@en, /case INSENSITIVE regexp with \/ escaped value/i)) {
	      name@en
	    }
	  }
    }
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    assert_eq!(
        "case INSENSITIVE regexp with / escaped value",
        res.query()[0].children()[1]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[0]
            .value()
    );
    assert_eq!(
        "i",
        res.query()[0].children()[1]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[1]
            .value()
    );
}

#[test]
fn test_parse_regexp2() {
    let query = r#"
	{
	  me(func:regexp(name@en, /another\/compilicated ("") regexp('')/)) {
	    name
	  }
    }
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    assert_eq!(
        "another/compilicated (\"\") regexp('')",
        res.query()[0].func().unwrap().args()[0].value()
    );
    assert_eq!("", res.query()[0].func().unwrap().args()[1].value());
}

#[test]
fn test_parse_regexp3() {
    let query = r#"
	{
	  me(func:allofterms(name, "barack")) @filter(regexp(secret, /whitehouse[0-9]{1,4}/fLaGs)) {
	    name
	  }
    }
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    assert_eq!(
        "whitehouse[0-9]{1,4}",
        res.query()[0].filter().unwrap().func().unwrap().args()[0].value()
    );
    assert_eq!(
        "fLaGs",
        res.query()[0].filter().unwrap().func().unwrap().args()[1].value()
    );
}

#[test]
fn test_parse_regexp4() {
    let query = r#"
	{
	  me(func:regexp(name@en, /pattern/123)) {
	    name
	  }
    }
"#;
    let err = parse(&Request::new(query)).err();
    // only [a-zA-Z] characters can be used as flags
    assert!(err.is_some());
    contains(&err, "Expected comma or language but got: 123");
}

#[test]
fn test_parse_regexp5() {
    let query = r#"
	{
	  me(func:regexp(name@en, /pattern/flag123)) {
	    name
	  }
    }
"#;
    let err = parse(&Request::new(query)).err();
    // only [a-zA-Z] characters can be used as flags
    assert!(err.is_some());
    contains(&err, "Expected comma or language but got: 123");
}

#[test]
fn test_parse_regexp6() {
    let query = r#"
	{
	  me(func:regexp(name@en, /pattern\/)) {
	    name
	  }
    }
"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unclosed regexp");
}

#[test]
fn test_count_at_root() {
    let query = r#"{
		me(func: uid( 1)) {
			count(uid)
			count(enemy)
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_count_at_root_err() {
    let query = r#"{
		me(func: uid( 1)) {
			count(enemy) {
				name
			}
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Cannot have children attributes when asking for count",
    );
}

#[test]
fn test_count_at_root_err2() {
    let query = r#"{
		me(func: uid( 1)) {
			count()
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Cannot use count(), please use count(uid)");
}

#[test]
fn test_has_func_at_root() {
    let query = r#"{
		me(func: has(name@en)) {
			name
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_has_filter_at_root() {
    let query = r#"{
		me(func: allofterms(name, "Steven Tom")) @filter(has(director.film)) {
			name
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_has_filter_at_child() {
    let query = r#"{
		me(func: anyofterms(name, "Steven Tom")) {
			name
			director.film @filter(has(genre)) {
			}
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_dots_eof() {
    let query = r#"{
		me(func: uid( 0x1)) {
			name
			.."#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unclosed action");
}

#[test]
fn test_math_without_var_alias() {
    let query = r#"{
			f(func: anyofterms(name, "Rick Michonne Andrea")) {
				ageVar as age
				math(ageVar *2)
			}
		}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Function math should be used with a variable or have an alias",
    );
}

#[test]
fn test_math_div0() {
    let tests = [
        (r#"{f(func: uid(1)){x:math(1+1)}}"#, false),
        (r#"{f(func: uid(1)){x:math(1/0)}}"#, true),
        (r#"{f(func: uid(1)){x:math(1/-0)}}"#, true),
        (r#"{f(func: uid(1)){x:math(1/ln(1))}}"#, true),
        (r#"{f(func: uid(1)){x:math(1/sqrt(0))}}"#, true),
        (r#"{f(func: uid(1)){x:math(1/floor(0))}}"#, true),
        (r#"{f(func: uid(1)){x:math(1/floor(0.5))}}"#, true),
        (r#"{f(func: uid(1)){x:math(1/floor(1.01))}}"#, false),
        (r#"{f(func: uid(1)){x:math(1/ceil(0))}}"#, true),
        (r#"{f(func: uid(1)){x:math(1%0}}"#, true),
        (r#"{f(func: uid(1)){x:math(1%floor(0)}}"#, true),
        (r#"{f(func: uid(1)){x:math(1 + 0)}}"#, false),
    ];
    for (input, has_error) in tests {
        let err = parse(&Request::new(input)).err();
        if has_error {
            assert!(err.is_some(), "Expected an error for {input:?}");
        } else {
            assert!(err.is_none(), "Unexpected error for {input:?}: {err:?}");
        }
    }
}

#[test]
fn test_multiple_equal() {
    let query = r#"{
		me(func: eq(name,["Steven Spielberg", "Tom Hanks"])) {
			name
		}
	}"#;
    let (dql, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(2, dql.query()[0].func().unwrap().args().len());
    assert_eq!(
        "Tom Hanks",
        dql.query()[0].func().unwrap().args()[1].value()
    );
}

#[test]
fn test_parse_eq_arg() {
    let query = r#"
	{
		me(func: uid(1, 20)) @filter(eq(name, ["And\"rea", "Bob"])) {
		 name
		}
	}
"#;
    let (dql, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        2,
        dql.query()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()
            .len()
    );
}

#[test]
fn test_parser_fuzz() {
    let tests = [
        ("test001", "{e(){@filter(q(/"),
        ("test002", "{e(){@filter(p(%"),
        ("test003", "{e(){@filter(0(/\\0"),
        ("test004", "query #"),
        ("test005", "{e(){{@filter(q(/"),
        ("test006", "{e(func:0(0(0,/"),
        ("test007", "{e(func:uid()){@filter(0(/\\"),
        ("test008", "{e(){@filter(0(/\\0/"),
        ("test009", "{e(){@filter(p(/\\"),
        ("test010", "{m(func:0(0(0(0,/"),
        ("test011", "{e(){@filter(0(%0"),
        ("test012", "{e(func:uid()){@filter(p(%"),
        ("test013", "{e(orderasc:val(0)){{min(val(0)0("),
        ("test014", "query{e(func:uid(val(0)"),
        ("test015", "{e(){@filter(0(%000"),
        ("test016", "{e(){@filter(0(%"),
        ("test017", "{e(){{@filter(0(/"),
        ("test018", "{e(func:uid(0))@filter(p(/\\"),
        ("test019", "{e(func:uid()){@filter(p(/\\"),
        ("test020", "{e(){@filter(p(/\\00"),
        ("test021", "{e(func:uid(0)){@filter(0(%"),
        ("test022", "{e(){@filter(0(/"),
        ("test023", "{e(func:uid()){@filter(p(/"),
        ("test024", "{s(func:uid(val(0)"),
        ("test025", "{e()@filter(0(%"),
        ("test026", "{e(func:uid(0)){@filter(0(/\\"),
        ("test027", "{e(){@filter(0(%"),
        ("test028", "{e(){@filter(0(%00"),
        ("test029", "{e(func:uid(0)){@filter(p(/\\e/i)){e}}}"),
        ("test030", "{e(func:uid(0)){@filter(p(%//i))00"),
        ("test031", "{e()@filter(p(/�Is))}"),
        ("test032", "{e(){@filter(0(0,/"),
        ("test033", "{e(func:uid()){@filter(0(/"),
        ("test034", "{e()@filter(0(/"),
        ("test035", "{e(func:uid(0)){@filter(p(/\\"),
        ("test036", "{e(func:uid())@filter(0(%"),
        ("test037", "{e(func:uid(0)){@filter(0(%"),
        ("test038", "{e(func:uid(0))@filter(p(/\\0/"),
        ("test039", "{e(func:uid(0)){@filter(p(/\\e/i)){e}00"),
        ("test040", "{e(){@filter(0(-"),
        ("test041", "{e(func:uid(0)){@filter(0(%0"),
        ("test042", "{e()@filter(q(/"),
        ("test043", "{e(func:uid(0)){@filter(p(%"),
        ("test044", "{e()@filter(p(/"),
        ("test045", "{e(func:uid())@filter(0(/"),
        ("test046", "{e(func:uid(0)){@filter(p(/\\e/"),
        ("test047", "{e(func:uid()){@filter(0(%"),
        ("test048", "{e()@filter(0(0,/"),
        ("test049", "{e(){{@filter(0(0,/"),
        ("test050", "{e(func:uid(0)){@filter(p(/"),
        ("test051", "{e()@filter(0(-"),
        ("test052", "{e(func:uid(0)){@filter(0(/"),
        ("test053", "{e(func:uid())@filter(p(%"),
        ("test054", "{e(orderasc:val(0)){min(val(0)0("),
        ("test055", "{e(){@filter(p(/"),
        ("test056", "{e(func:uid())@filter(p(/"),
        ("test057", "a<><\\�"),
        ("test058", "L<\\𝌀"),
        ("test059", "{d(after:<>0)}"),
        ("test060", "{e(orderasc:#"),
    ];
    for (name, input) in tests {
        // The parser must not panic, whatever it answers.
        let res = std::panic::catch_unwind(|| parse(&Request::new(input)));
        assert!(
            res.is_ok(),
            "parser panic caused by test: '{name}', input: '{input}'"
        );
    }
}

#[test]
fn test_parse_eq_arg2() {
    let query = r#"
	{
		me(func: eq(age, [1, 20])) @filter(eq(name, ["And\"rea", "Bob"])) {
		 name
		}
	}
"#;
    let (dql, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        2,
        dql.query()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()
            .len()
    );
    assert_eq!(2, dql.query()[0].func().unwrap().args().len());
}

#[test]
fn test_filter_error() {
    let query = r#"
	{
		me(func: uid(1, 3 , 5, 7)) { @filter(uid(3, 7))
			name
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
}

#[test]
fn test_filter_error2() {
    let query = r#"
	{
		me(func: uid(1, 3 , 5, 7)) {
			name @filter(eq(name, 	"abc")) @filter(eq(name2, 	"abc"))
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
}

#[test]
fn test_double_group_by_error() {
    let query = r#"
	{
		me(func: uid(1, 3 , 5, 7)) {
			name @groupby(abc) @groupby(bcd)
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
}

#[test]
fn test_filter_error3() {
    let query = r#"
	{
		me(func: uid(1, 3 , 5, 7)) {
			expand(_all_) @filter(eq(name, "abc"))
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
}

#[test]
fn test_filter_uid() {
    let query = r#"
	{
		me(func: uid(1, 3 , 5, 7)) @filter(uid(3, 7)) {
			name
		}
	}
	"#;
    let (dql, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(vec![1, 3, 5, 7], dql.query()[0].uid());
    assert_eq!(
        vec![3, 7],
        dql.query()[0].filter().unwrap().func().unwrap().uid()
    );
}

#[test]
fn test_id_err() {
    let query = r#"
	{
		me(id: [1, 3 , 5, 7]) @filter(uid(3, 7)) {
			name
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Got invalid keyword: id");
}

#[test]
fn test_filter_var_err() {
    let query = r#"
	{
		x as m(func: allofterms(name, "Pawan Rawal"))
	}
	{
		me(func: uid(1, 3 , 5, 7)) @filter(var(x)) {
			name
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Unexpected var()");
}

#[test]
fn test_agg_root1() {
    let query = r#"
		{
			var(func: anyofterms(name, "Rick Michonne Andrea")) {
				a as age
			}

			me() {
				sum(val(a))
				avg(val(a))
			}
		}
	"#;
    let (dql, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!("me", dql.query()[1].alias());
    assert!(dql.query()[1].is_empty());
}

#[test]
fn test_agg_root_error() {
    let query = r#"
		{
			var(func: anyofterms(name, "Rick Michonne Andrea")) {
				a as age
			}

			me() {
				sum(val(a))
				avg(val(a))
				friend @filter(anyofterms(name, "Hey")) {
					name
				}
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Only aggregation/math functions allowed inside empty blocks. Got: friend",
    );
}

#[test]
fn test_agg_root_error2() {
    let query = r#"
		{
			var(func: anyofterms(name, "Rick Michonne Andrea")) {
				a as age
			}

			me() {
				avg(val(a))
				name
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Only aggregation/math functions allowed inside empty blocks. Got: name",
    );
}

#[test]
fn test_agg_root_error3() {
    let query = r#"
		{
			var(func: anyofterms(name, "Rick Michonne Andrea")) {
				a as age
			}

			me() {
				avg
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Only aggregation/math functions allowed inside empty blocks. Got: avg",
    );
}

#[test]
fn test_empty_function() {
    let query = r#"
		{
		  director(func:allofterms()) {
		    name@en
		    director.film (orderdesc: initial_release_date) {
		      name@en
		      initial_release_date
		    }
		  }
		}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Got empty attr for function: [allofterms]");
}

#[test]
fn test_order1() {
    let query = r#"
		{
			me(func: uid(1), orderdesc: name, orderasc: age) {
				name
			}
		}
	"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(2, gq.query()[0].order().len());
    assert_eq!("name", gq.query()[0].order()[0].attr);
    assert!(gq.query()[0].order()[0].desc);
    assert_eq!("age", gq.query()[0].order()[1].attr);
    assert!(!gq.query()[0].order()[1].desc);
}

#[test]
fn test_order2() {
    let query = r#"
		{
			me(func: uid(0x01)) {
				friend(orderasc: alias, orderdesc: name) @filter(lt(alias, "Pat")) {
					alias
				}
			}
		}
	"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let curp = &gq.query()[0].children()[0];
    assert_eq!(2, curp.order().len());
    assert_eq!("alias", curp.order()[0].attr);
    assert!(!curp.order()[0].desc);
    assert_eq!("name", curp.order()[1].attr);
    assert!(curp.order()[1].desc);
}

#[test]
fn test_multiple_order_error() {
    let query = r#"
		{
			me(func: uid(0x01)) {
				friend(orderasc: alias, orderdesc: alias) {
					alias
				}
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Sorting by an attribute: [alias] can only be done once",
    );
}

#[test]
fn test_multiple_order_error2() {
    let query = r#"
		{
			me(func: uid(0x01),orderasc: alias, orderdesc: alias) {
				friend {
					alias
				}
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Sorting by an attribute: [alias] can only be done once",
    );
}

#[test]
fn test_eq_arg_with_dollar() {
    // This is a fix for #1444.
    let query = r#"
	{
		ab(func: eq(name@en, "$pringfield (or, How)")) {
			uid
		}
	}
	"#;
    let (dql, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        dql.query()[0].func().unwrap().args()[0].value(),
        r#"$pringfield (or, How)"#
    );
}

#[test]
fn test_lang_with_dash() {
    let query = r#"{
		q(func: uid(1)) {
			text@en-us
		}
	}"#;
    let (dql, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(vec!["en-us"], dql.query()[0].children()[0].langs());
}

#[test]
fn test_order_by_var_and_pred() {
    let query = r#"{
		q(func: uid(1), orderasc: name, orderdesc: val(n)) {
		}

		var(func: uid(0x0a)) {
			friends {
				n AS name
			}
		}

	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Multiple sorting only allowed by predicates.");
    let query = r#"{
		q(func: uid(1)) {
		}

		var(func: uid(0x0a)) {
			friends (orderasc: name, orderdesc: val(n)) {
				n AS name
			}
		}

	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Multiple sorting only allowed by predicates.");
    let query = r#"{
		q(func: uid(1)) {
		}

		var(func: uid(0x0a)) {
			friends (orderasc: name, orderdesc: genre) {
				name
			}
		}

	}"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_invalid_val_usage() {
    let query = r#"
		{
			me(func: uid(0x01)) {
				val(uid) {
					nope
				}
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Query syntax invalid.");
}

#[test]
fn test_order_with_multiple_lang_fail() {
    let query = r#"
	{
		me(func: uid(0x1), orderasc: name@en:fr, orderdesc: lastname@ci, orderasc: salary) {
			name
		}
	}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "Sorting by an attribute: [name@en:fr] can only be done on one language",
    );
}

#[test]
fn test_order_with_lang() {
    let query = r#"
	{
		me(func: uid(0x1), orderasc: name@en, orderdesc: lastname@ci, orderasc: salary) {
			name
		}
	}
"#;
    let (res, err) = split(parse(&Request::new(query)));
    no_err(&err);
    let _ = &res.query();
    assert_eq!(1, res.query().len());
    let orders = &res.query()[0].order();
    assert_eq!("name", orders[0].attr);
    assert_eq!(vec!["en"], orders[0].langs);
    assert_eq!("lastname", orders[1].attr);
    assert_eq!(vec!["ci"], orders[1].langs);
    assert_eq!("salary", orders[2].attr);
    assert_eq!(0, orders[2].langs.len());
}

#[test]
fn test_parse_lang_tag_after_string_in_root() {
    // This is a fix for #1499.
    let query = r#"
		{
			q(func: anyofterms(name, "Hello"@en)) {
				uid
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Invalid usage of '@' in function argument");
}

#[test]
fn test_parse_lang_tag_after_string_in_filter() {
    // This is a fix for #1499.
    let query = r#"
		{
			q(func: uid(0x01)) @filter(eq(name, "Hello"@en)) {
				uid
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Invalid usage of '@' in function argument");
}

#[test]
fn test_parse_mutation() {
    let m = r#"
		{
			set {
				<name> <is> <something> .
				<hometown> <is> <san/francisco> .
			}
			delete {
				<name> <is> <something-else> .
			}
		}
	"#;
    let (req, err) = split(parse_mutation(m));
    no_err(&err);
    let mu = &req.mutations[0];
    // Dgraph's test parses the text with its RDF parser, the chunker, which this crate does not depend on; the
    // N-Quads are compared as the text they parse from.
    assert_eq!(
        nquad_lines(&mu.set_nquads),
        [
            "<name> <is> <something> .",
            "<hometown> <is> <san/francisco> ."
        ]
    );
    assert_eq!(
        nquad_lines(&mu.del_nquads),
        ["<name> <is> <something-else> ."]
    );
}

fn nquad_lines(b: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(b)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn test_parse_mutation_too_many_blocks() {
    let tests = [
        (
            r#"
         {
           set { _:a1 <reg> "a1 content" . }
         }{
		   set { _:b2 <reg> "b2 content" . }
		 }"#,
            "Unexpected { after the end of the block",
        ),
        (
            r#"{set { _:a1 <reg> "a1 content" . }} something"#,
            "Invalid operation type: something",
        ),
        (
            r#"
          # comments are ok
		  {
		    set { _:a1 <reg> "a1 content" . } # comments are ok
          } # comments are ok"#,
            "",
        ),
    ];
    for (m, err_str) in tests {
        let (mu, err) = split(parse_mutation(m));
        if err_str.is_empty() {
            no_err(&err);
        } else {
            contains(&err, err_str);
            assert_eq!(mu, Default::default());
        }
    }
}

#[test]
fn test_parse_missing_graph_ql_var() {
    let queries = [
        "{ q(func: eq(name, $a)) { name }}",
        "query { q(func: eq(name, $a)) { name }}",
        "query foo { q(func: eq(name, $a)) { name }}",
        "query foo () { q(func: eq(name, $a)) { name }}",
        "query foo ($b: string) { q(func: eq(name, $a)) { name }}",
        "query foo ($a: string) { q(func: eq(name, $b)) { name }}",
    ];
    for q in queries {
        let err = parse(&vars(q, &[("$a", "alice")])).err();
        assert!(err.is_some(), "{q}");
    }
}

#[test]
fn test_parse_graph_ql_var_pagination_root() {
    let queries = [
        "query test($a: int = 2){ q(func: uid(0x1), first: $a) { name }}",
        "query test($a: int = 2){ q(func: uid(0x1), offset: $a) { name }}",
        "query test($a: int = 2){ q(func: uid(0x1), orderdesc: name, first: $a) { name }}",
        "query test($a: int = 2){ q(func: uid(0x1), orderdesc: name, offset: $a) { name }}",
        "query test($a: int = 2){ q(func: eq(name, \"abc\"), orderdesc: name, first: $a) { name }}",
        "query test($a: int = 2){ q(func: eq(name, \"abc\"), orderdesc: name, offset: $a) { name }}",
    ];
    for q in queries {
        let (gq, err) = split(parse(&vars(q, &[("$a", "3")])));
        no_err(&err);
        let args = gq.query()[0].args();
        assert!(
            args.get("first").is_some_and(|v| v == "3")
                || args.get("offset").is_some_and(|v| v == "3"),
            "{q}"
        );
    }
}

#[test]
fn test_parse_graph_ql_var_pagination_child() {
    let queries = [
        "query test($a: int = 2){ q(func: uid(0x1)) { friend(first: $a) }}",
        "query test($a: int = 2){ q(func: uid(0x1)) { friend(offset: $a) }}",
        "query test($a: int = 2){ q(func: uid(0x1), orderdesc: name) { friend(first: $a) }}",
        "query test($a: int = 2){ q(func: uid(0x1), orderdesc: name) { friend(offset: $a) }}",
        "query test($a: int = 2){ q(func: eq(name, \"abc\"), orderdesc: name) { friend(first: $a) }}",
        "query test($a: int = 2){ q(func: eq(name, \"abc\"), orderdesc: name) { friend(offset: $a) }}",
    ];
    for q in queries {
        let (gq, err) = split(parse(&vars(q, &[("$a", "3")])));
        no_err(&err);
        let args = gq.query()[0].children()[0].args();
        assert!(
            args.get("first").is_some_and(|v| v == "3")
                || args.get("offset").is_some_and(|v| v == "3"),
            "{q}"
        );
    }
}

#[test]
fn test_parse_graph_ql_var_pagination_root_multiple() {
    let q = r#"query test($a: int, $b: int, $after: string){
		q(func: uid(0x1), first: $a, offset: $b, after: $after, orderasc: name) {
			friend
		}
	}"#;
    let r = vars(q, &[("$a", "3"), ("$b", "5"), ("$after", "0x123")]);
    let (gq, err) = split(parse(&r));
    no_err(&err);
    let args = &gq.query()[0].args();
    assert_eq!(args["first"], "3");
    assert_eq!(args["offset"], "5");
    assert_eq!(args["after"], "0x123");
    assert_eq!(gq.query()[0].order()[0].attr, "name");
}

#[test]
fn test_parse_graph_ql_var_array() {
    let tests = [
        (
            r#"query test($a: string){q(func: eq(name, [$a])) {name}}"#,
            &[("$a", "srfrog")][..],
            1,
        ),
        (
            r#"query test($a: string, $b: string){q(func: eq(name, [$a, $b])) {name}}"#,
            &[("$a", "srfrog"), ("$b", "horseman")][..],
            2,
        ),
        (
            r#"query test($a: string, $b: string, $c: string){q(func: eq(name, [$a, $b, $c])) {name}}"#,
            &[("$a", "srfrog"), ("$b", "horseman"), ("$c", "missbug")][..],
            3,
        ),
        (
            r#"query test($a: string){q(func: eq(name, [$a, "mrtrout"])) {name}}"#,
            &[("$a", "srfrog")][..],
            2,
        ),
        (
            r#"query test($a: string){q(func: eq(name, ["mrtrout", $a])) {name}}"#,
            &[("$a", "srfrog")][..],
            2,
        ),
    ];
    for (q, vs, nargs) in tests {
        let (gq, err) = split(parse(&vars(q, vs)));
        no_err(&err);
        assert_eq!(1, gq.query().len());
        let func = gq.query()[0].func().unwrap();
        assert_eq!("eq", func.name());
        assert_eq!(nargs, func.args().len());
        for (_, val) in vs {
            assert!(
                func.args().iter().any(|a| a.value() == *val),
                "vars not matched: {vs:?}"
            );
        }
    }
}

#[test]
fn test_parse_graph_ql_var_array_uid_in() {
    let tests = [
        (
            r#"query test($a: string){q(func: uid_in(director.film, [$a])) {name}}"#,
            &[("$a", "0x4e472a")][..],
            1,
        ),
        (
            r#"query test($a: string, $b: string){q(func: uid_in(director.film, [$a, $b])) {name}}"#,
            &[("$a", "0x4e472a"), ("$b", "0x4e9545")][..],
            2,
        ),
        (
            r#"query test($a: string){q(func: uid_in(name, [$a, "0x4e9545"])) {name}}"#,
            &[("$a", "0x4e472a")][..],
            2,
        ),
        (
            r#"query test($a: string){q(func: uid_in(name, ["0x4e9545", $a])) {name}}"#,
            &[("$a", "0x4e472a")][..],
            2,
        ),
    ];
    for (q, vs, nargs) in tests {
        let (gq, err) = split(parse(&vars(q, vs)));
        no_err(&err);
        assert_eq!(1, gq.query().len());
        let func = gq.query()[0].func().unwrap();
        assert_eq!("uid_in", func.name());
        assert_eq!(nargs, func.args().len());
        for (_, val) in vs {
            assert!(
                func.args().iter().any(|a| a.value() == *val),
                "vars not matched: {vs:?}"
            );
        }
    }
}

#[test]
fn test_uid_in_with_no_parse_errors() {
    let query = r#"{
		schoolVar as q(func: uid(5000))
		me(func: uid(1, 23, 24 )) {
			friend @filter(uid_in(school, uid(schoolVar))) {
				name
			}
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_uid_in_with_parse_errors() {
    let tests = [
        (
            "uid_in query with without argument",
            r#"{
				me(func: uid(1, 23, 24 )) {
					friend @filter(uid_in(school, )) {
						name
					}
				}
			}"#,
            "Empty Argument",
        ),
        (
            "uid_in query with without argument (2)",
            r#"{
				me(func: uid(1, 23, 24 )) {
					friend @filter(uid_in(school )) {
						name
					}
				}
			}"#,
            "uid_in function expects an argument, got none",
        ),
        (
            "query with nested uid without variable",
            r#"{
				me(func: uid(1, 23, 24 )) {
					friend @filter(uid_in(school, uid(5000))) {
						name
					}
				}
			}"#,
            "Nested uid fn expects 1 uid variable, got 0",
        ),
        (
            "query with nested uid with variable and constant",
            r#"{
				uidVar as q(func: uid( 5000))
				me(func: uid(1, 23, 24 )) {
					friend @filter(uid_in(school, uid(uidVar, 5001))) {
						name
					}
				}
			}"#,
            "Nested uid fn expects only uid variable, got UID",
        ),
        (
            "query with nested uid with two variables",
            r#"{
				uidVar1 as q(func: uid( 5000))
				uidVar2 as q(func: uid( 5000))
				me(func: uid(1, 23, 24 )) {
					friend @filter(uid_in(school, uid(uidVar1, uidVar2))) {
						name
					}
				}
			}"#,
            "Nested uid fn expects 1 uid variable, got 2",
        ),
        (
            "query with nested uid with dql variable",
            r#"query queryWithDQL($schoolUID: string = "5001"){
				me(func: uid(1, 23, 24 )){
					friend @filter(uid_in(school, uid( $schoolUID))) {
						name
					}
				}
			}"#,
            "Nested uid fn expects 1 uid variable, got 0",
        ),
    ];
    for (description, query, expected_err) in tests {
        let err = parse(&Request::new(query)).err();
        assert!(
            err.as_ref()
                .is_some_and(|e| e.to_string().contains(expected_err)),
            "{description}: {err:?}"
        );
    }
}

#[test]
fn test_parse_graph_ql_value_array() {
    let q = r#"
	{
		q(func: eq(name, ["srfrog", "horseman"])) {
			name
		}
	}"#;
    let (gq, err) = split(parse(&Request::new(q)));
    no_err(&err);
    assert_eq!(1, gq.query().len());
    assert_eq!("eq", gq.query()[0].func().unwrap().name());
    assert_eq!(2, gq.query()[0].func().unwrap().args().len());
    assert_eq!("srfrog", gq.query()[0].func().unwrap().args()[0].value());
    assert_eq!("horseman", gq.query()[0].func().unwrap().args()[1].value());
}

#[test]
fn test_parse_graph_ql_mixed_var_array() {
    let q = r#"
	query test($a: string, $b: string, $c: string){
		q(func: eq(name, ["uno", $a, $b, "cuatro", $c])) {
			name
		}
	}"#;
    let r = vars(q, &[("$a", "dos"), ("$b", "tres"), ("$c", "cinco")]);
    let (gq, err) = split(parse(&r));
    no_err(&err);
    assert_eq!(1, gq.query().len());
    assert_eq!("eq", gq.query()[0].func().unwrap().name());
    assert_eq!(5, gq.query()[0].func().unwrap().args().len());
    assert_eq!("uno", gq.query()[0].func().unwrap().args()[0].value());
    assert_eq!("dos", gq.query()[0].func().unwrap().args()[1].value());
    assert_eq!("tres", gq.query()[0].func().unwrap().args()[2].value());
    assert_eq!("cuatro", gq.query()[0].func().unwrap().args()[3].value());
    assert_eq!("cinco", gq.query()[0].func().unwrap().args()[4].value());
}

#[test]
fn test_line_and_column_number_in_error_output() {
    let q = r#"
	query {
		me(func: uid(0x0a)) {
			friends @filter(alloftext(descr@, "something")) {
				name
			}
			gender,age
			hometown
		}
	}"#;
    let err = parse(&Request::new(q)).err();
    assert!(err.is_some());
    contains(
        &err,
        "line 4 column 35: Unrecognized character in lexDirective: U+002C ','",
    );
}

#[test]
fn test_type_function() {
    let q = r#"
	query {
		me(func: type(Person)) {
			name
		}
	}"#;
    let (gq, err) = split(parse(&Request::new(q)));
    no_err(&err);
    assert_eq!(1, gq.query().len());
    assert_eq!("type", gq.query()[0].func().unwrap().name());
    assert_eq!(1, gq.query()[0].func().unwrap().args().len());
    assert_eq!("Person", gq.query()[0].func().unwrap().args()[0].value());
}

#[test]
fn test_type_function_error1() {
    let q = r#"
	query {
		me(func: type(Person, School)) {
			name
		}
	}"#;
    let err = parse(&Request::new(q)).err();
    assert!(err.is_some());
    contains(&err, "type function only supports one argument");
}

#[test]
fn test_type_in_filter() {
    let q = r#"
	query {
		me(func: uid(0x01)) @filter(type(Person)) {
			name
		}
	}"#;
    let (gq, err) = split(parse(&Request::new(q)));
    no_err(&err);
    assert_eq!(1, gq.query().len());
    assert_eq!("uid", gq.query()[0].func().unwrap().name());
    assert_eq!(1, gq.query()[0].children().len());
    assert_eq!("name", gq.query()[0].children()[0].attr());
    assert_eq!(
        "type",
        gq.query()[0].filter().unwrap().func().unwrap().name()
    );
    assert_eq!(
        1,
        gq.query()[0].filter().unwrap().func().unwrap().args().len()
    );
    assert_eq!(
        "Person",
        gq.query()[0].filter().unwrap().func().unwrap().args()[0].value()
    );
}

#[test]
fn test_type_filter_in_predicate() {
    let q = r#"
	query {
		me(func: uid(0x01)) {
			friend @filter(type(Person)) {
				name
			}
		}
	}"#;
    let (gq, err) = split(parse(&Request::new(q)));
    no_err(&err);
    assert_eq!(1, gq.query().len());
    assert_eq!("uid", gq.query()[0].func().unwrap().name());
    assert_eq!(1, gq.query()[0].children().len());
    assert_eq!("friend", gq.query()[0].children()[0].attr());
    assert_eq!(
        "type",
        gq.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .name()
    );
    assert_eq!(
        1,
        gq.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()
            .len()
    );
    assert_eq!(
        "Person",
        gq.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[0]
            .value()
    );
    assert_eq!(1, gq.query()[0].children()[0].children().len());
    assert_eq!("name", gq.query()[0].children()[0].children()[0].attr());
}

#[test]
fn test_parse_expand_type() {
    let query = r#"
	{
		var(func: has(name)) {
			expand(Person,Animal) {
				uid
			}
		}
	}
"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(1, gq.query().len());
    assert_eq!(1, gq.query()[0].children().len());
    assert_eq!("expand", gq.query()[0].children()[0].attr());
    assert_eq!("Person,Animal", gq.query()[0].children()[0].expand());
    assert_eq!(1, gq.query()[0].children()[0].children().len());
    assert_eq!("uid", gq.query()[0].children()[0].children()[0].attr());
}

#[test]
fn test_parse_var_after_count_qry() {
    let query = r#"
		{
			q(func: allofterms(name@en, "steven spielberg")) {
				director.film {
					u1 as count(uid)
					genre {
						u2 as math(1)
					}
			  	}
			}

			sum() {
				totalMovies: sum(val(u1))
				totalGenres: sum(val(u2))
			}
		}
	"#;
    let err = parse(&Request::new(query)).err();
    no_err(&err);
}

#[test]
fn test_recurse_with_args() {
    let query = r#"
	{
		me(func: eq(name, "sad")) @recurse(depth: $hello , loop: true) {
		}
	}"#;
    let (gq, err) = split(parse(&vars(query, &[("$hello", "1")])));
    no_err(&err);
    assert_eq!(gq.query()[0].recurse_args().depth(), 1u64);
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: 1 , loop: $hello) {
		}
	}"#;
    let (gq, err) = split(parse(&vars(query, &[("$hello", "true")])));
    no_err(&err);
    assert!(gq.query()[0].recurse_args().allow_loop());
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: $hello, loop: $hello1) {
		}
	}"#;
    let (gq, err) = split(parse(&vars(query, &[("$hello", "1"), ("$hello1", "true")])));
    no_err(&err);
    assert!(gq.query()[0].recurse_args().allow_loop());
    assert_eq!(gq.query()[0].recurse_args().depth(), 1u64);
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: $_hello_hello, loop: $hello1_heelo1) {
		}
	}"#;
    let (gq, err) = split(parse(&vars(
        query,
        &[("$_hello_hello", "1"), ("$hello1_heelo1", "true")],
    )));
    no_err(&err);
    assert!(gq.query()[0].recurse_args().allow_loop());
    assert_eq!(gq.query()[0].recurse_args().depth(), 1u64);
}

#[test]
fn test_recurse_with_args_with_error() {
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: $hello, loop: true) {
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "variable $hello not defined");
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: 1, loop: $hello) {
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "variable $hello not defined");
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: $hello, loop: $hello1) {
		}
	}"#;
    let err = parse(&vars(query, &[("$hello", "sd"), ("$hello1", "true")])).err();
    assert!(err.is_some());
    contains(&err, "should be type of integer");
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: $hello, loop: $hello1) {
		}
	}"#;
    let err = parse(&vars(query, &[("$hello", "1"), ("$hello1", "tre")])).err();
    assert!(err.is_some());
    contains(&err, "should be type of boolean");
}

#[test]
fn test_recurse() {
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: 1, loop: true) {
		}
	}"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(gq.query()[0].recurse_args().depth(), 1u64);
    assert!(gq.query()[0].recurse_args().allow_loop());
}

#[test]
fn test_recurse_with_error() {
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: hello, loop: true) {
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Value inside depth should be type of integer");
    let query = r#"
	{
		me(func: eq(name, "sad"))@recurse(depth: 1, loop: tre) {
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "Value inside loop should be type of boolean");
}

#[test]
fn test_parse_expand_filter() {
    let query = r#"
		{
			q(func: eq(name, "Frodo")) {
				expand(_all_) @filter(type(Person)) {
					uid
				}
			}
		}"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(1, gq.query().len());
    assert_eq!(1, gq.query()[0].children().len());
    assert_eq!(
        "type",
        gq.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .name()
    );
    assert_eq!(
        1,
        gq.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()
            .len()
    );
    assert_eq!(
        "Person",
        gq.query()[0].children()[0]
            .filter()
            .unwrap()
            .func()
            .unwrap()
            .args()[0]
            .value()
    );
}

#[test]
fn test_parse_expand_filter_err() {
    let query = r#"
		{
			q(func: eq(name, "Frodo")) {
				expand(_all_) @filter(has(Person)) {
					uid
				}
			}
		}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(&err, "expand is only compatible with type filters");
}

#[test]
fn test_filter_with_dollar() {
    let query = r#"
	{
		q(func: eq(name, "Bob"), first:5) @filter(eq(description, "$yo")) {
		  name
		  description
		}
	  }
	"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        gq.query()[0].filter().unwrap().func().unwrap().args()[0].value(),
        "$yo"
    );
}

#[test]
fn test_filter_with_dollar_error() {
    let query = r#"
	{
		q(func: eq(name, "Bob"), first:5) @filter(eq(description, $yo)) {
		  name
		  description
		}
	  }
	"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
}

#[test]
fn test_filter_with_eq_and_len_in_wrong_order_args() {
    let query = r#"
	{
		var(func: has(school), first: 3) {
			f as uid
		}

		me(func: uid(f)) @filter(eq(3, len(f))) {
			count(uid)
		}
	}"#;
    let err = parse(&Request::new(query)).err();
    assert!(err.is_some());
    contains(
        &err,
        "incorrect order, the first argument should be len function",
    );
}

#[test]
fn test_filter_with_var() {
    let query = r#"query data($a: string = "dgraph")
	{
		data(func: eq(name, "Bob"), first:5) @filter(eq(description, $a)) {
			name
			description
		  }
	}"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        gq.query()[0].filter().unwrap().func().unwrap().args()[0].value(),
        "dgraph"
    );
}

#[test]
fn test_filter_with_empty() {
    let query = r#"{
		names(func: has(name)) @filter(eq(name, "")) {
		  count(uid)
		}
	  }"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(
        gq.query()[0].filter().unwrap().func().unwrap().args()[0].value(),
        ""
    );
}

#[test]
fn test_cascade() {
    let query = r#"{
		names(func: has(name)) @cascade {
		  name
		}
	  }"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(gq.query()[0].cascade()[0], "__all__");
}

#[test]
fn test_cascade_parameterized() {
    let query = r#"{
		names(func: has(name)) @cascade(name, age) {
		  name
		  age
		  dob
		}
	  }"#;
    let (gq, err) = split(parse(&Request::new(query)));
    no_err(&err);
    assert_eq!(gq.query()[0].cascade()[0], "name");
    assert_eq!(gq.query()[0].cascade()[1], "age");
}

#[test]
fn test_bad_cascade_parameterized() {
    let bad_queries = [
        r#"{
			names(func: has(name)) @cascade( {
			  name
			  age
			  dob
			}
		  }"#,
        r#"{
			names(func: has(name)) @cascade) {
			  name
			  age
			  dob
			}
		 }"#,
        r#"{
			names(func: has(name)) @cascade() {
			  name
			  age
			  dob
			}
		  }"#,
        r#"{
			names(func: has(name)) @cascade(,) {
			  name
			  age
			  dob
			}
		  }"#,
        r#"{
			names(func: has(name)) @cascade(name,) {
			  name
			  age
			  dob
			}
		  }"#,
        r#"{
			names(func: has(name)) @cascade(,name) {
			  name
			  age
			  dob
			}
		  }"#,
    ];
    for query in bad_queries {
        let err = parse(&Request::new(query)).err();
        assert!(err.is_some(), "{query}");
    }
}

#[test]
fn test_empty_id() {
    let q = "query me($a: string) {q(func: uid($a)) {name }}";
    let r = vars(q, &[("$a", " ")]);
    let err = parse(&r).err();
    assert!(err.is_some());
}
