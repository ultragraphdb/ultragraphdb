// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_dql: every string literal of Dgraph's dql tests, parsed as a query, as a
// mutation and as a DQL request, the query trees written as JSON by a walker both sides implement, or the error
// text; and N-Quads converted into directed edges.

import (
	"encoding/hex"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"math"
	"os"
	"path/filepath"
	"reflect"
	"strconv"

	"github.com/dgraph-io/dgo/v250/protos/api"

	"github.com/dgraph-io/dgraph/v25/dql"
	"github.com/dgraph-io/dgraph/v25/protos/pb"
	"github.com/dgraph-io/dgraph/v25/types"
)

func init() {
	register("dql_parse", "ultragraphdb_core_dql", genDqlParse)
	register("dql_edges", "ultragraphdb_core_dql", genDqlEdges)
	register("dql_uids", "ultragraphdb_core_dql", genDqlUids)
}

func dqlForkDir() (string, error) {
	if fork := os.Getenv("DGRAPH_SRC"); fork != "" {
		return fork, nil
	}
	root, err := filepath.Abs("../..")
	if err != nil {
		return "", err
	}
	return filepath.Join(root, "..", "..", "fork", "dgraph"), nil
}

// dqlTestLiterals returns every string literal of the dql package's tests, in order and without repeats, and
// every map[string]string literal, the request variables the tests pass.
func dqlTestLiterals() ([]string, []map[string]string, error) {
	fork, err := dqlForkDir()
	if err != nil {
		return nil, nil, err
	}
	var strs []string
	var maps []map[string]string
	seen := map[string]bool{}
	for _, name := range []string{"parser_test.go", "upsert_test.go", "dql_test.go", "state_test.go"} {
		fset := token.NewFileSet()
		f, err := parser.ParseFile(fset, filepath.Join(fork, "dql", name), nil, 0)
		if err != nil {
			return nil, nil, err
		}
		ast.Inspect(f, func(n ast.Node) bool {
			switch x := n.(type) {
			case *ast.BasicLit:
				if x.Kind == token.STRING {
					if s, err := strconv.Unquote(x.Value); err == nil && !seen[s] {
						seen[s] = true
						strs = append(strs, s)
					}
				}
			case *ast.CompositeLit:
				mt, ok := x.Type.(*ast.MapType)
				if !ok {
					return true
				}
				if k, ok := mt.Key.(*ast.Ident); !ok || k.Name != "string" {
					return true
				}
				if v, ok := mt.Value.(*ast.Ident); !ok || v.Name != "string" {
					return true
				}
				m := map[string]string{}
				for _, e := range x.Elts {
					kv := e.(*ast.KeyValueExpr)
					k, _ := strconv.Unquote(kv.Key.(*ast.BasicLit).Value)
					v, _ := strconv.Unquote(kv.Value.(*ast.BasicLit).Value)
					m[k] = v
				}
				maps = append(maps, m)
			}
			return true
		})
	}
	return strs, maps, nil
}

// ---- The JSON form of the query tree. Slices are never null, maps are objects, absent pointers are null.

func dqlStrings(s []string) []string {
	if s == nil {
		return []string{}
	}
	return s
}

func dqlUids(u []uint64) []uint64 {
	if u == nil {
		return []uint64{}
	}
	return u
}

func dqlStringMap(m map[string]string) map[string]string {
	if m == nil {
		return map[string]string{}
	}
	return m
}

func dqlNeedsVar(vs []dql.VarContext) []any {
	out := []any{}
	for _, v := range vs {
		out = append(out, map[string]any{"name": v.Name, "typ": v.Typ})
	}
	return out
}

func dqlFunction(f *dql.Function) any {
	if f == nil {
		return nil
	}
	args := []any{}
	for _, a := range f.Args {
		args = append(args, map[string]any{"value": a.Value, "is_value_var": a.IsValueVar, "is_dql_var": a.IsDQLVar})
	}
	return map[string]any{
		"attr": f.Attr, "lang": f.Lang, "name": f.Name, "args": args, "uid": dqlUids(f.UID),
		"needs_var": dqlNeedsVar(f.NeedsVar), "is_count": f.IsCount, "is_value_var": f.IsValueVar,
		"is_len_var": f.IsLenVar,
	}
}

func dqlFilter(f *dql.FilterTree) any {
	if f == nil {
		return nil
	}
	child := []any{}
	for _, c := range f.Child {
		child = append(child, dqlFilter(c))
	}
	return map[string]any{"op": f.Op, "child": child, "func": dqlFunction(f.Func)}
}

// dqlConst writes a math constant as its type and its value as text: ints in decimal, floats as FormatFloat 'E'.
func dqlConst(v types.Val) any {
	var value any
	switch x := v.Value.(type) {
	case nil:
		value = nil
	case int64:
		value = strconv.FormatInt(x, 10)
	case float64:
		value = strconv.FormatFloat(x, 'E', -1, 64)
	case string:
		value = x
	case bool:
		value = strconv.FormatBool(x)
	default:
		value = fmt.Sprint(x)
	}
	return map[string]any{"tid": int(v.Tid), "value": value}
}

func dqlMath(t *dql.MathTree) any {
	if t == nil {
		return nil
	}
	child := []any{}
	for _, c := range t.Child {
		child = append(child, dqlMath(c))
	}
	return map[string]any{"fn": t.Fn, "var": t.Var, "const": dqlConst(t.Const), "child": child}
}

func dqlFacets(f *pb.FacetParams) any {
	if f == nil {
		return nil
	}
	param := []any{}
	for _, p := range f.Param {
		param = append(param, map[string]any{"key": p.Key, "alias": p.Alias})
	}
	return map[string]any{"all_keys": f.AllKeys, "param": param}
}

// dqlRecurseVarMap reads RecurseArgs' unexported varMap.
func dqlRecurseVarMap(r dql.RecurseArgs) any {
	m := reflect.ValueOf(r).FieldByName("varMap")
	if m.IsNil() {
		return nil
	}
	out := map[string]string{}
	for _, k := range m.MapKeys() {
		out[k.String()] = m.MapIndex(k).String()
	}
	return out
}

func dqlGraphQuery(gq *dql.GraphQuery) any {
	order := []any{}
	for _, o := range gq.Order {
		order = append(order, map[string]any{"attr": o.Attr, "desc": o.Desc, "langs": dqlStrings(o.Langs)})
	}
	children := []any{}
	for _, c := range gq.Children {
		children = append(children, dqlGraphQuery(c))
	}
	groupby := []any{}
	for _, g := range gq.GroupbyAttrs {
		groupby = append(groupby, map[string]any{"attr": g.Attr, "alias": g.Alias, "langs": dqlStrings(g.Langs)})
	}
	var facetVar any
	if gq.FacetVar != nil {
		facetVar = gq.FacetVar
	}
	facetsOrder := []any{}
	for _, f := range gq.FacetsOrder {
		facetsOrder = append(facetsOrder, map[string]any{"key": f.Key, "desc": f.Desc})
	}
	return map[string]any{
		"uid": dqlUids(gq.UID), "attr": gq.Attr, "langs": dqlStrings(gq.Langs), "alias": gq.Alias,
		"is_count": gq.IsCount, "is_internal": gq.IsInternal, "is_groupby": gq.IsGroupby, "var": gq.Var,
		"needs_var": dqlNeedsVar(gq.NeedsVar), "func": dqlFunction(gq.Func), "expand": gq.Expand,
		"args": dqlStringMap(gq.Args), "order": order, "children": children, "filter": dqlFilter(gq.Filter),
		"math_exp": dqlMath(gq.MathExp), "normalize": gq.Normalize, "recurse": gq.Recurse,
		"recurse_args": map[string]any{
			"depth": gq.RecurseArgs.Depth, "allow_loop": gq.RecurseArgs.AllowLoop,
			"var_map": dqlRecurseVarMap(gq.RecurseArgs),
		},
		"shortest_path_args": map[string]any{
			"from": dqlFunction(gq.ShortestPathArgs.From), "to": dqlFunction(gq.ShortestPathArgs.To),
		},
		"cascade": dqlStrings(gq.Cascade), "ignore_reflex": gq.IgnoreReflex, "facets": dqlFacets(gq.Facets),
		"facets_filter": dqlFilter(gq.FacetsFilter), "groupby_attrs": groupby, "facet_var": facetVar,
		"facets_order": facetsOrder, "allowed_preds": dqlStrings(gq.AllowedPreds), "is_empty": gq.IsEmpty,
	}
}

func dqlResult(res dql.Result) any {
	query := []any{}
	for _, q := range res.Query {
		query = append(query, dqlGraphQuery(q))
	}
	vars := []any{}
	for _, v := range res.QueryVars {
		vars = append(vars, map[string]any{"defines": dqlStrings(v.Defines), "needs": dqlStrings(v.Needs)})
	}
	var schema any
	if res.Schema != nil {
		schema = map[string]any{
			"predicates": dqlStrings(res.Schema.Predicates), "fields": dqlStrings(res.Schema.Fields),
			"types": dqlStrings(res.Schema.Types),
		}
	}
	return map[string]any{"query": query, "query_vars": vars, "schema": schema}
}

func dqlRequest(req *api.Request) any {
	mutations := []any{}
	for _, m := range req.Mutations {
		mutations = append(mutations, map[string]any{
			"set_nquads": string(m.SetNquads), "del_nquads": string(m.DelNquads), "cond": m.Cond,
		})
	}
	return map[string]any{"query": req.Query, "mutations": mutations}
}

// dqlRunResult is one parse: its JSON result or its error text.
type dqlRunResult struct {
	Ok    any    `json:"ok"`
	Err   string `json:"err"`
	IsErr bool   `json:"is_err"`
}

// dqlOutcome holds the distinct results of 41 runs of one parse, in the order first seen. There is more than one
// where Go's random map iteration decides which of several failing entries is reported.
type dqlOutcome struct {
	Outcomes []dqlRunResult `json:"outcomes"`
}

func dqlRun(f func() (any, error)) *dqlOutcome {
	run := func() (out dqlRunResult) {
		defer func() {
			if r := recover(); r != nil {
				out = dqlRunResult{Err: fmt.Sprintf("panic: %v", r), IsErr: true}
			}
		}()
		v, err := f()
		if err != nil {
			return dqlRunResult{Err: err.Error(), IsErr: true}
		}
		return dqlRunResult{Ok: v}
	}
	out := &dqlOutcome{}
	seen := map[string]bool{}
	for range 41 {
		r := run()
		key := fmt.Sprintf("%#v", r.Ok) + r.Err
		if !seen[key] {
			seen[key] = true
			out.Outcomes = append(out.Outcomes, r)
		}
	}
	return out
}

// dqlParseCase is one input. Mutation and Dql are null for an input given with variables, which those parsers do
// not take.
type dqlParseCase struct {
	Input     string            `json:"input"`
	Variables map[string]string `json:"variables"`
	Parse     *dqlOutcome       `json:"parse"`
	Mutation  *dqlOutcome       `json:"mutation"`
	Dql       *dqlOutcome       `json:"dql"`
}

func genDqlParse() (any, error) {
	strs, maps, err := dqlTestLiterals()
	if err != nil {
		return nil, err
	}
	extra := []string{
		"{ me(func: uid(0x1)) { name@en:ru:hu } }",
		"{ me(func: uid(0x1)) { name@en@ru } }",
		"{ me(func: uid(0x1)) { name@* } }",
		"{ me(func: uid(0x1)) { name@. } }",
		"{ me(func: uid(0x1)) { math(1 + 2) } }",
		"{ me(func: uid(0x1)) { x as count(uid) y : math(x * 1.5e3 - inf) } }",
		"{ me(func: uid(0x1), first: 99999999999999999999) { name } }",
		"{ me(func: uid(99999999999999999999)) { name } }",
		"{ me(func: uid(0x1)) @filter(eq(name, \"a\\'b\")) { name } }",
		"{ me(func: eq(name, \"\\u00e9\\x41\\101\")) { name } }",
		"{ me(func: uid(0x1)) { name @filter(regexp(name, /a\\/b/i)) } }",
		"mutation { set { _:a <name> \"x\" . } }",
		"upsert {}",
		"upsert {",
		"{ me(func: uid(1_000)) { name } }",
		"{ me(func: uid(0b101, 0o17, 017)) { name } }",
		"{ me(func: eq(name, \"é\")) { 名前 } }",
		"{ me(func: uid(0x1)) { a: name b: name } }",
		"{ me(func: uid(0x1)) @facets { name } }",
		"{ me(func: uid(0x1)) { name @facets(b, a, b as c) } }",
	}
	strs = append(strs, extra...)
	var cases []dqlParseCase
	add := func(s string, vars map[string]string) {
		c := dqlParseCase{Input: s, Variables: vars}
		c.Parse = dqlRun(func() (any, error) {
			res, err := dql.Parse(dql.Request{Str: s, Variables: vars})
			if err != nil {
				return nil, err
			}
			return dqlResult(res), nil
		})
		if vars == nil {
			c.Mutation = dqlRun(func() (any, error) {
				req, err := dql.ParseMutation(s)
				if err != nil {
					return nil, err
				}
				return dqlRequest(req), nil
			})
			c.Dql = dqlRun(func() (any, error) {
				req, err := dql.ParseDQL(s)
				if err != nil {
					return nil, err
				}
				return dqlRequest(req), nil
			})
		}
		cases = append(cases, c)
	}
	for _, s := range strs {
		add(s, nil)
	}
	// Every string naming a parameter, with every variable map the tests pass.
	for _, s := range strs {
		hasDollar := false
		for i := range len(s) {
			if s[i] == '$' {
				hasDollar = true
				break
			}
		}
		if !hasDollar {
			continue
		}
		for _, m := range maps {
			add(s, m)
		}
	}
	return cases, nil
}

// ---- N-Quads to directed edges.

type dqlValue struct {
	Kind  string `json:"kind"`
	Str   string `json:"str"`
	Int   int64  `json:"int"`
	Bool  bool   `json:"bool"`
	Float string `json:"float"` // FormatFloat 'g', -1: JSON has no NaN or infinities.
	Bytes string `json:"bytes"` // hex
}

type dqlFacet struct {
	Key     string `json:"key"`
	Value   string `json:"value"` // hex
	ValType int32  `json:"val_type"`
	Alias   string `json:"alias"`
}

type dqlNQuad struct {
	Subject     string     `json:"subject"`
	Predicate   string     `json:"predicate"`
	ObjectId    string     `json:"object_id"`
	ObjectValue *dqlValue  `json:"object_value"`
	Lang        string     `json:"lang"`
	Facets      []dqlFacet `json:"facets"`
	Namespace   uint64     `json:"namespace"`
}

type dqlEdge struct {
	Entity    uint64     `json:"entity"`
	Attr      string     `json:"attr"`
	Value     string     `json:"value"` // hex
	ValueType int32      `json:"value_type"`
	ValueId   uint64     `json:"value_id"`
	Lang      string     `json:"lang"`
	Op        int32      `json:"op"`
	Facets    []dqlFacet `json:"facets"`
	Namespace uint64     `json:"namespace"`
}

type dqlEdgeOutcome struct {
	Edge *dqlEdge `json:"edge"`
	Err  string   `json:"err"`
}

type dqlEdgeCase struct {
	NQuad      dqlNQuad          `json:"nquad"`
	NewToUid   map[string]uint64 `json:"new_to_uid"`
	EdgeUsing  dqlEdgeOutcome    `json:"to_edge_using"`
	DeletePred dqlEdgeOutcome    `json:"to_delete_pred_edge"`
}

type dqlUidCase struct {
	Input string `json:"input"`
	Uid   uint64 `json:"uid"`
	Err   string `json:"err"`
}

func (v *dqlValue) api() *api.Value {
	if v == nil {
		return nil
	}
	b, _ := hex.DecodeString(v.Bytes)
	f, _ := strconv.ParseFloat(v.Float, 64)
	switch v.Kind {
	case "default":
		return &api.Value{Val: &api.Value_DefaultVal{DefaultVal: v.Str}}
	case "str":
		return &api.Value{Val: &api.Value_StrVal{StrVal: v.Str}}
	case "password":
		return &api.Value{Val: &api.Value_PasswordVal{PasswordVal: v.Str}}
	case "int":
		return &api.Value{Val: &api.Value_IntVal{IntVal: v.Int}}
	case "bool":
		return &api.Value{Val: &api.Value_BoolVal{BoolVal: v.Bool}}
	case "double":
		return &api.Value{Val: &api.Value_DoubleVal{DoubleVal: f}}
	case "bytes":
		return &api.Value{Val: &api.Value_BytesVal{BytesVal: b}}
	case "geo":
		return &api.Value{Val: &api.Value_GeoVal{GeoVal: b}}
	case "datetime":
		return &api.Value{Val: &api.Value_DatetimeVal{DatetimeVal: b}}
	case "bigfloat":
		return &api.Value{Val: &api.Value_BigfloatVal{BigfloatVal: b}}
	case "vfloat32":
		return &api.Value{Val: &api.Value_Vfloat32Val{Vfloat32Val: b}}
	case "date":
		return &api.Value{Val: &api.Value_DateVal{DateVal: b}}
	case "uidval":
		return &api.Value{Val: &api.Value_UidVal{UidVal: uint64(v.Int)}}
	}
	return &api.Value{}
}

func (n dqlNQuad) api() *api.NQuad {
	nq := &api.NQuad{
		Subject: n.Subject, Predicate: n.Predicate, ObjectId: n.ObjectId, ObjectValue: n.ObjectValue.api(),
		Lang: n.Lang, Namespace: n.Namespace,
	}
	for _, f := range n.Facets {
		b, _ := hex.DecodeString(f.Value)
		nq.Facets = append(nq.Facets, &api.Facet{Key: f.Key, Value: b, ValType: api.Facet_ValType(f.ValType), Alias: f.Alias})
	}
	return nq
}

func dqlEdgeOf(e *pb.DirectedEdge, err error) dqlEdgeOutcome {
	if err != nil {
		return dqlEdgeOutcome{Err: err.Error()}
	}
	out := &dqlEdge{
		Entity: e.Entity, Attr: e.Attr, Value: hex.EncodeToString(e.Value), ValueType: int32(e.ValueType),
		ValueId: e.ValueId, Lang: e.Lang, Op: int32(e.Op), Facets: []dqlFacet{}, Namespace: e.Namespace,
	}
	for _, f := range e.Facets {
		out.Facets = append(out.Facets, dqlFacet{Key: f.Key, Value: hex.EncodeToString(f.Value), ValType: int32(f.ValType), Alias: f.Alias})
	}
	return dqlEdgeOutcome{Edge: out}
}

// dqlEdgeRecover records a panic (Go dereferences a missing object value) as the error "panic".
func dqlEdgeRecover(f func() (*pb.DirectedEdge, error)) (out dqlEdgeOutcome) {
	defer func() {
		if r := recover(); r != nil {
			out = dqlEdgeOutcome{Err: "panic"}
		}
	}()
	return dqlEdgeOf(f())
}

func genDqlEdges() (any, error) {
	f32 := func(fs ...float32) string {
		b := make([]byte, 0, 4*len(fs))
		for _, f := range fs {
			u := math.Float32bits(f)
			b = append(b, byte(u), byte(u>>8), byte(u>>16), byte(u>>24))
		}
		return hex.EncodeToString(b)
	}
	values := []*dqlValue{
		nil,
		{Kind: "default", Str: "A"},
		{Kind: "default", Str: "_STAR_ALL"},
		{Kind: "str", Str: "hello"},
		{Kind: "str", Str: ""},
		{Kind: "password", Str: "secret123"},
		{Kind: "int", Int: 42},
		{Kind: "int", Int: -7},
		{Kind: "bool", Bool: true},
		{Kind: "bool", Bool: false},
		{Kind: "double", Float: "3.25"},
		{Kind: "double", Float: "-0.1"},
		{Kind: "bytes", Bytes: "00ff10"},
		{Kind: "geo", Bytes: "0101000000000000000000f03f0000000000000040"},
		{Kind: "datetime", Bytes: "010000000ed3c0a2d000000000ffff"},
		{Kind: "bigfloat", Bytes: "0a0b0c"},
		{Kind: "vfloat32", Bytes: f32(1, 2.5, -3)},
		{Kind: "date", Bytes: "0102"},
		{Kind: "uidval", Int: 5},
		{Kind: "empty"},
	}
	newToUid := map[string]uint64{"_:a": 10, "_:b": 11, "_:zero": 0, "xid1": 12}
	subjects := []string{"0x1", "1", "_:a", "_:zero", "_:missing", "0x0", "_STAR_ALL", "0xffffffffffffffff", "0x1_0"}
	var cases []dqlEdgeCase
	addCase := func(n dqlNQuad) {
		if n.Facets == nil {
			n.Facets = []dqlFacet{}
		}
		nq := dql.NQuad{NQuad: n.api()}
		c := dqlEdgeCase{NQuad: n, NewToUid: newToUid}
		c.EdgeUsing = dqlEdgeRecover(func() (*pb.DirectedEdge, error) { return nq.ToEdgeUsing(newToUid) })
		c.DeletePred = dqlEdgeRecover(nq.ToDeletePredEdge)
		cases = append(cases, c)
	}
	for _, s := range subjects {
		for _, v := range values {
			if v == nil {
				continue
			}
			addCase(dqlNQuad{Subject: s, Predicate: "name", ObjectValue: v})
		}
		for _, o := range []string{"0x2", "_:b", "_:zero", "_:missing", "0x0", "xid1"} {
			addCase(dqlNQuad{Subject: s, Predicate: "friend", ObjectId: o})
		}
	}
	for _, v := range values {
		if v == nil {
			continue
		}
		addCase(dqlNQuad{Subject: "0x1", Predicate: "name", ObjectValue: v, Lang: "en"})
	}
	addCase(dqlNQuad{Subject: "0x1", Predicate: "name", ObjectValue: &dqlValue{Kind: "default", Str: "A"}, Lang: "en",
		Namespace: 3, Facets: []dqlFacet{{Key: "since", Value: "0a", ValType: 1, Alias: "s"}}})
	addCase(dqlNQuad{Subject: "_STAR_ALL", Predicate: "name", ObjectValue: &dqlValue{Kind: "default", Str: "_STAR_ALL"}, Namespace: 2})
	addCase(dqlNQuad{Subject: "0x1", Predicate: "name"})
	addCase(dqlNQuad{Subject: "0x1", Predicate: "name", Lang: "en"})

	return cases, nil
}

func genDqlUids() (any, error) {
	var uids []dqlUidCase
	for _, in := range []string{"0x1", "1", "0", "0x0", "", "abc", "0x", "0b11", "0o17", "017", "08", "1_000", "_1",
		"0xffffffffffffffff", "0x10000000000000000", "18446744073709551616", "-1", "+1", " 1", "0X1F", "0x_1", "1__0"} {
		uid, err := dql.ParseUid(in)
		c := dqlUidCase{Input: in, Uid: uid}
		if err != nil {
			c.Err = err.Error()
		}
		uids = append(uids, c)
	}
	return uids, nil
}
