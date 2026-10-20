// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_facets: FacetFor's inferred type, binary value, term tokens and error text
// for facet values as the RDF lexer hands them over, ValFor's decoding of each facet (marshalled to its string form), ToBinary for typed values and
// SortAndValidate's sorting and refusal.

import (
	"encoding/hex"
	"fmt"
	"math"
	"time"

	"github.com/dgraph-io/dgo/v250/protos/api"
	"github.com/dgraph-io/dgraph/v25/types"
	"github.com/dgraph-io/dgraph/v25/types/facets"
)

func init() {
	register("facets_facet_for", "ultragraphdb_core_facets", genFacetFor)
	register("facets_to_binary", "ultragraphdb_core_facets", genFacetsToBinary)
	register("facets_sort_and_validate", "ultragraphdb_core_facets", genFacetsSortAndValidate)
}

type facetJSON struct {
	Key     string   `json:"key"`
	Value   string   `json:"value"`
	ValType int      `json:"val_type"`
	Tokens  []string `json:"tokens"`
}

func facetToJSON(f *api.Facet) *facetJSON {
	out := &facetJSON{Key: f.Key, Value: hex.EncodeToString(f.Value), ValType: int(f.ValType), Tokens: []string{}}
	for _, t := range f.Tokens {
		out.Tokens = append(out.Tokens, hex.EncodeToString([]byte(t)))
	}
	return out
}

type valForJSON struct {
	Tid int    `json:"tid"`
	Str string `json:"str"`
	Err string `json:"err"`
}

type facetForCase struct {
	Key    string      `json:"key"`
	Val    string      `json:"val"`
	Err    string      `json:"err"`
	Facet  *facetJSON  `json:"facet"`
	ValFor *valForJSON `json:"val_for"`
}

// The facet values, as the RDF lexer emits them: quoted strings with their quotes, everything else raw.
var facetValues = []string{
	// Strings.
	``, `""`, `"x"`, `"hello world"`, `"READ WRITE"`, `"\"hello world\""`, `"LineA\nLineB"`, `"hatter E"`,
	`"\x41\x42"`, `"\101"`, `"\xff"`, `"a\xffb c"`, `"\377"`, `"\a\b\f\v\t\r"`, `"\U0001F600 smile"`, `"Ünïcödé Straße"`, `"The quick, brown fox!"`,
	`"名前 です"`, `"a b a b"`, `"'single'"`,
	// Strings Go's Unquote refuses.
	`"\'"`, `"a"b"`, `"\q"`, `"\u12"`, `"\ud800"`, `"\400"`, "\"raw\nnewline\"", `"\"`,
	// Ints, base prefixes and underscores included.
	`13`, `-13`, `+13`, `0`, `-0`, `0x10`, `0X1F`, `-0x10`, `0o17`, `0O17`, `0b101`, `017`, `08`, `1_000`, `0x_1f`,
	`1__0`, `_1`, `1_`, `0x`, `0b`, `9223372036854775807`, `-9223372036854775808`, `0x7fffffffffffffff`,
	// Ints out of range: digits only are refused, anything else is tried as a float.
	`9223372036854775808`, `-9223372036854775809`, `99999999999999999999`, `111111111111111111888888`,
	`0x1787586C4FA8A0284FF8`, `0xFFFFFFFFFFFFFFFFF`, `1_000_000_000_000_000_000_000`,
	// Floats.
	`1.5`, `-1.5`, `.5`, `5.`, `1.2`, `0.7`, `1e10`, `1E-5`, `1e+3`, `111111111111111111888888.23`, `1_000.5`,
	`1.5_0`, `1e1_0`, `4.9e-324`, `2e-324`, `1e-400`, `1.7976931348623157e308`, `1.7976931348623159e308`,
	`1e400`, `-1e400`, `inf`, `+Inf`, `-infinity`, `Infinity`, `infin`, `NaN`, `nan`, `-nan`, `0x1p-2`, `0x1.8p1`,
	`0X1P4`, `0x1.fffffffffffff8p1023`, `0x1p1024`, `0x1p-1074`, `0x1p-1075`, `0x1.0000000000000801p0`,
	`0x1.00000000000008p0`, `0x1.00000000000018p0`, `0x123456789abcdef0123p0`, `0x1.8`, `1e`, `1e+`, `.`, `-`,
	// Bools: only the lower-case words.
	`true`, `false`, `True`, `TRUE`, `t`, `f`,
	// Datetimes.
	`2006-01-02T15:04:05`, `2006-01-02T15:04:05Z`, `2006-01-02T00:00:00Z`, `2002-10-02T15:00:00.05Z`,
	`2006-01-02T15:04:05+07:00`, `2006-01-02T15:04:05.999999999-08:00`, `2006-01-02`, `2006`, `2006-01`,
	`2006-01-02 15:04:05 -0700 MST`, `2006-13-01`, `2006-01-02T25:00:00`,
	// Refused.
	`abc`, `hatter`, `'x'`, `"`, `"unterminated`, `١٢٣`, `0x1g`, `1.2.3`,
}

func genFacetFor() (any, error) {
	var cases []facetForCase
	for _, v := range facetValues {
		c := facetForCase{Key: "k", Val: v}
		f, err := facets.FacetFor("k", v)
		if err != nil {
			c.Err = err.Error()
		} else {
			c.Facet = facetToJSON(f)
			vf := &valForJSON{}
			val, err := facets.ValFor(f)
			if err != nil {
				vf.Err = err.Error()
			} else {
				vf.Tid = int(val.Tid)
				s := types.Val{Tid: types.StringID}
				if err := types.Marshal(val, &s); err != nil {
					vf.Err = err.Error()
				} else {
					vf.Str = s.Value.(string)
				}
			}
			c.ValFor = vf
		}
		cases = append(cases, c)
	}
	return cases, nil
}

type toBinaryCase struct {
	Name    string     `json:"name"`
	ValType int        `json:"val_type"`
	Err     string     `json:"err"`
	Panic   string     `json:"panic"`
	Facet   *facetJSON `json:"facet"`
}

// toBinary calls ToBinary, recording the panic Go's unchecked type assertion raises for a value of another type.
func toBinary(c *toBinaryCase, v interface{}, vt api.Facet_ValType) {
	defer func() {
		if r := recover(); r != nil {
			c.Panic = fmt.Sprint(r)
		}
	}()
	f, err := facets.ToBinary("k", v, vt)
	if err != nil {
		c.Err = err.Error()
	} else {
		c.Facet = facetToJSON(f)
	}
}

func genFacetsToBinary() (any, error) {
	t, err := types.ParseTime("2006-01-02T15:04:05+07:00")
	if err != nil {
		return nil, err
	}
	inputs := []struct {
		name string
		v    interface{}
		vt   api.Facet_ValType
	}{
		{"string", "READ WRITE", api.Facet_STRING},
		{"empty string", "", api.Facet_STRING},
		{"int", int64(3), api.Facet_INT},
		{"negative int", int64(-1), api.Facet_INT},
		{"float", 30000.56, api.Facet_FLOAT},
		{"inf", math.Inf(1), api.Facet_FLOAT},
		{"true", true, api.Facet_BOOL},
		{"false", false, api.Facet_BOOL},
		{"datetime", t, api.Facet_DATETIME},
		{"utc datetime", time.Date(2006, 1, 2, 15, 4, 5, 0, time.UTC), api.Facet_DATETIME},
		{"int as string", int64(3), api.Facet_STRING},
		{"string as int", "3", api.Facet_INT},
		{"float as int", 1.5, api.Facet_INT},
		{"bool as float", true, api.Facet_FLOAT},
		{"int as bool", int64(1), api.Facet_BOOL},
		{"string as datetime", "2006", api.Facet_DATETIME},
		{"unknown type", int64(1), api.Facet_ValType(7)},
	}
	var cases []toBinaryCase
	for _, in := range inputs {
		c := toBinaryCase{Name: in.name, ValType: int(in.vt)}
		toBinary(&c, in.v, in.vt)
		cases = append(cases, c)
	}
	return cases, nil
}

type sortAndValidateCase struct {
	Keys   []string `json:"keys"`
	Sorted []string `json:"sorted"`
	Err    string   `json:"err"`
}

func genFacetsSortAndValidate() (any, error) {
	inputs := [][]string{
		{},
		{"a"},
		{"weight", "close", "since"},
		{"b", "a", "c", "B", "A", "_", "1"},
		{"weight", "close", "weight"},
		{"z", "y", "z", "y"},
		{"é", "e", "f"},
		{"same", "same"},
	}
	var cases []sortAndValidateCase
	for _, keys := range inputs {
		var fs []*api.Facet
		for _, k := range keys {
			fs = append(fs, &api.Facet{Key: k})
		}
		c := sortAndValidateCase{Keys: keys, Sorted: []string{}}
		if err := facets.SortAndValidate(fs); err != nil {
			c.Err = err.Error()
		} else {
			for _, f := range fs {
				c.Sorted = append(c.Sorted, f.Key)
			}
		}
		cases = append(cases, c)
	}
	return cases, nil
}
