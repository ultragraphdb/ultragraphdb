// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_schema: every string literal of Dgraph's schema/parse_test.go and a corpus of
// variants over every type, directive, tokenizer, tokenizer pair, vector index option and namespace form, each
// parsed with schema.Parse and with schema.ParseWithNamespace(s, 5), as the updates or the error text; the initial
// schema and types of namespaces 0 and 5 with ACL on and off; and the pre-defined predicate and type checks.

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strconv"

	"github.com/dgraph-io/dgraph/v25/protos/pb"
	"github.com/dgraph-io/dgraph/v25/schema"
	"github.com/dgraph-io/dgraph/v25/x"
)

func init() {
	register("schema_parse", "ultragraphdb_core_schema", genSchemaParse)
	register("schema_initial", "ultragraphdb_core_schema", genSchemaInitial)
	register("schema_predefined", "ultragraphdb_core_schema", genSchemaPredefined)
}

// schemaOptionJSON is a pb.OptionPair.
type schemaOptionJSON struct {
	Key   string `json:"key"`
	Value string `json:"value"`
}

// schemaSpecJSON is a pb.VectorIndexSpec.
type schemaSpecJSON struct {
	Name    string             `json:"name"`
	Options []schemaOptionJSON `json:"options"`
}

// schemaUpdateJSON is a pb.SchemaUpdate, every field written; a nil list is written empty.
type schemaUpdateJSON struct {
	Predicate       string           `json:"predicate"`
	ValueType       int32            `json:"value_type"`
	Directive       int32            `json:"directive"`
	Tokenizer       []string         `json:"tokenizer"`
	Count           bool             `json:"count"`
	List            bool             `json:"list"`
	Upsert          bool             `json:"upsert"`
	Lang            bool             `json:"lang"`
	Unique          bool             `json:"unique"`
	NonNullable     bool             `json:"non_nullable"`
	NonNullableList bool             `json:"non_nullable_list"`
	ObjectTypeName  string           `json:"object_type_name"`
	NoConflict      bool             `json:"no_conflict"`
	IndexSpecs      []schemaSpecJSON `json:"index_specs"`
}

// schemaTypeJSON is a pb.TypeUpdate.
type schemaTypeJSON struct {
	TypeName string             `json:"type_name"`
	Fields   []schemaUpdateJSON `json:"fields"`
}

func schemaUpdateToJSON(u *pb.SchemaUpdate) schemaUpdateJSON {
	j := schemaUpdateJSON{
		Predicate:       u.Predicate,
		ValueType:       int32(u.ValueType),
		Directive:       int32(u.Directive),
		Tokenizer:       append([]string{}, u.Tokenizer...),
		Count:           u.Count,
		List:            u.List,
		Upsert:          u.Upsert,
		Lang:            u.Lang,
		Unique:          u.Unique,
		NonNullable:     u.NonNullable,
		NonNullableList: u.NonNullableList,
		ObjectTypeName:  u.ObjectTypeName,
		NoConflict:      u.NoConflict,
		IndexSpecs:      []schemaSpecJSON{},
	}
	for _, s := range u.IndexSpecs {
		spec := schemaSpecJSON{Name: s.Name, Options: []schemaOptionJSON{}}
		for _, o := range s.Options {
			spec.Options = append(spec.Options, schemaOptionJSON{Key: o.Key, Value: o.Value})
		}
		j.IndexSpecs = append(j.IndexSpecs, spec)
	}
	return j
}

func schemaUpdatesToJSON(us []*pb.SchemaUpdate) []schemaUpdateJSON {
	out := []schemaUpdateJSON{}
	for _, u := range us {
		out = append(out, schemaUpdateToJSON(u))
	}
	return out
}

func schemaTypesToJSON(ts []*pb.TypeUpdate) []schemaTypeJSON {
	out := []schemaTypeJSON{}
	for _, t := range ts {
		out = append(out, schemaTypeJSON{TypeName: t.TypeName, Fields: schemaUpdatesToJSON(t.Fields)})
	}
	return out
}

// schemaOutcome is the result of one parse: the updates, or the error text.
type schemaOutcome struct {
	Preds []schemaUpdateJSON `json:"preds"`
	Types []schemaTypeJSON   `json:"types"`
	Err   string             `json:"err"`
	IsErr bool               `json:"is_err"`
}

type schemaParseCase struct {
	Input string        `json:"input"`
	Parse schemaOutcome `json:"parse"`
	Ns5   schemaOutcome `json:"ns5"`
}

func schemaRun(f func() (*schema.ParsedSchema, error)) (out schemaOutcome) {
	defer func() {
		if r := recover(); r != nil {
			out = schemaOutcome{Err: fmt.Sprintf("panic: %v", r), IsErr: true}
		}
	}()
	res, err := f()
	if err != nil {
		return schemaOutcome{Err: err.Error(), IsErr: true}
	}
	return schemaOutcome{Preds: schemaUpdatesToJSON(res.Preds), Types: schemaTypesToJSON(res.Types)}
}

// schemaTestLiterals returns every string literal of schema/parse_test.go, in order and without repeats.
func schemaTestLiterals() ([]string, error) {
	fork := os.Getenv("DGRAPH_SRC")
	if fork == "" {
		root, err := filepath.Abs("../..")
		if err != nil {
			return nil, err
		}
		fork = filepath.Join(root, "..", "..", "fork", "dgraph")
	}
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, filepath.Join(fork, "schema", "parse_test.go"), nil, 0)
	if err != nil {
		return nil, err
	}
	var strs []string
	ast.Inspect(f, func(n ast.Node) bool {
		if lit, ok := n.(*ast.BasicLit); ok && lit.Kind == token.STRING {
			if s, err := strconv.Unquote(lit.Value); err == nil {
				strs = append(strs, s)
			}
		}
		return true
	})
	return strs, nil
}

var (
	schemaTypeNames = []string{"default", "binary", "int", "float", "bool", "datetime", "geo", "uid", "string",
		"password", "bigfloat", "float32vector", "dateTime", "STRING", "object", "Int"}
	schemaTokenizerNames = []string{"bigfloat", "geo", "int", "float", "year", "month", "day", "hour", "term",
		"exact", "bool", "trigram", "hash", "fulltext", "ngram", "sha256", "hnsw", "EXACT", "unknown"}
	schemaDirectives = []string{"@reverse", "@count", "@upsert", "@unique", "@noconflict", "@lang", "@index",
		"@foo", "@", "@reverse @count", "@count @upsert @lang", "@index(exact) @upsert @unique",
		"@index(term) @lang", "@lang @index(exact)", "@count @count"}
	schemaVectorIndexes = []string{
		`hnsw`, `hnsw()`, `HNSW`, `hnsw(metric:"euclidean")`, `hnsw(metric:"cosine")`,
		`hnsw(metric:"dotproduct")`, `hnsw(metric:"manhattan")`, `hnsw(metric:"")`, `hnsw(exponent:"4")`,
		`hnsw(exponent:"x")`, `hnsw(exponent:"-1")`, `hnsw(maxLevels:"3")`, `hnsw(efConstruction:"200")`,
		`hnsw(efSearch:"40")`, `hnsw(efSearch:"4.5")`, `hnsw(unknown:"1")`, `hnsw(metric:"euclidean", exponent:"4")`,
		`hnsw(metric:"cosine",exponent:"6",maxLevels:"3",efConstruction:"128",efSearch:"64")`,
		`hnsw(metric "cosine")`, `hnsw(metric:cosine)`, `hnsw(metric:"cosine" exponent:"4")`, `hnsw(:"cosine")`,
		`hnsw(metric:"cosine",)`, `hnsw(metric:"cosine"`, `hnsw(metric:"co\"sine")`, `hnsw, hnsw`,
		`hnsw(metric:"cosine"), hnsw(metric:"euclidean")`, `hnsw, exact`, `exact, hnsw`, `hnsw exact`,
		`hnsw[metric:"cosine"]`, `hnsw(metric:"cosine")(exponent:"4")`, `hnsw(metric:"euclidean", exponent:"4", metric:"cosine")`,
	}
	schemaNamespacePrefixes = []string{"[0x5]", "[10]", "[0]", "[0x0]", "[010]", "[089]", "[0xf2]", "[0XF2]",
		"[0xffffffffffffffff]", "[18446744073709551615]", "[0x1ffffffffffffffff]", "[99999999999999999999]",
		"[abc]", "[0x5", "[]", "[ 0x5 ]", "[0x5][0x6]", "[0x5]\n", "[-1]", "[0x]"}
	schemaMisc = []string{
		"", "\n", "   \n\t\n", ".", "name", "name:", "name: ", "name: string", "name: string .", "name: string .\n",
		"name:string.", "name string .", "name: .", "name: foo .", "name@en: string .", "<name@en>: string .",
		": string .", "name: string . extra", "name: string . # comment", "name: string .# comment",
		"name: string #comment\n.", "name: string @index(exact", "name: string @index(exact term) .",
		"name: string @index(exact,) .", "name: string @index() .", "name: string @index(exact)) .",
		"name: string @index((exact)) .", "name: string @index(<exact>) .", "name: string @index(\"exact\") .",
		"name: string @index(exact) @index(term) .", "name: string @index(exact, term, exact) .",
		"name: \"string\" .", "name: <string> .", "<name>: <string> .", "<name: string .", "<na me>: string .",
		"<a\\u0041>: string .", "<a\\x41>: string .", "\"name\": string .", "\"unclosed: string .",
		"name: string \t.", "name: string .\r\nage: int .\r\n", "name: string .\rage: int .", "name: string ! .",
		"name: string $ .", "名前: string .", "<名前>: string .", "_name: string .", "_: string .", "__: string .",
		"0name: string .", "1: string .", "0x1: string .", "name.first: string .", "name-first: string .",
		"name: [string] .", "name: [string .", "name: string] .", "name: [string]] .", "name: [[string]] .",
		"name: [] .", "name: [uid] @reverse @count .", "name: [string] @lang .", "name: [uid] @index(exact) .",
		"name: [password] .", "name: [bool] .", "name: [default] .", "name: [float32vector] .",
		"name: [geo] @index(geo) .", "name: [int] @index(int) @count @upsert .", "name: string @lang @lang .",
		"type: string .", "type: [uid] .", "type T: string .", "type T", "type T {", "type T {\n",
		"type T { }", "type T {}", "type T {}\n", "type T {} ", "type T {}x", "type T {}\ntype U {}",
		"type T {} type U {}", "type T {\n name\n}", "type T { name }", "type T {\n name age\n}",
		"type T {\n name: string\n}", "type T {\n name: [string]\n}", "type T {\n name: [string\n}",
		"type T {\n name: [string!\n}", "type T {\n name: string!!\n}", "type T {\n name: [string!]!\n}",
		"type T {\n name: [string]!!\n}", "type T {\n name:\n}", "type T {\n name: \n}", "type T {\n : string\n}",
		"type T {\n name: string .\n}", "type T {\n 0\n}", "type T {\n name { }\n}", "type T {\n name\n name\n}",
		"type T {\n name\n age\n name\n}", "type T {\n <name>\n}", "type T {\n name\n}\nname: string .",
		"type {\n name\n}", "type T T {\n name\n}", "type T { name\n}", "type T {\n name}",
		"type T {\n name # comment\n}", "type T {\n name\n} # comment", "type T {\n @name\n}",
		"type <T> {\n name\n}", "type T {\n name\n}\n\ntype T {\n age\n}", "type dgraph.type.User {\n dgraph.xid\n}",
		"name: string .\ntype T {\n name\n}\nage: int @index(int) .",
		"name: string @index(exact) .\nname: int .", "name: string @index(trigram, term, fulltext) .",
		"name: string @index(fulltext, term, exact, hash, trigram, sha256) .",
		"name: string @index(exact, hash) .", "name: string @index(ngram) .", "dob: datetime @index(year, month) .",
		"dob: datetime @index(hour) .", "dob: datetime @index(day, hour) .", "price: bigfloat @index(bigfloat) .",
		"loc: geo @index(geo) .", "ok: bool @index(bool) .", "age: int @index(int, int) .",
		"age: int @index(float) .", "f: float @index(float) .", "v: float32vector .",
		"v: float32vector @index(exact) .", "v: float32vector @index(hnsw(metric:\"cosine\")) @count .",
		"v: float32vector @index(hnsw(metric:\"cosine\")) @upsert @unique .",
		"pass: password @index(exact) .", "pass: password @upsert .", "friend: uid @reverse @count @upsert .",
		"friend: uid @noconflict @reverse .", "friend: string @reverse .", "age: int @lang .",
		"dgraph.xid: string @index(exact) @upsert .", "dgraph.type: [string] @index(exact) .",
		"\n#\n# comment\n#\n", "# only a comment", "#", "name: string .\n#", "name: string . #",
		"name: string .\n\n\n\nage: int .", "  name  :  string  @index( exact , term )  .  ",
		"name:string@index(exact,term).",
		// The spec's scenarios.
		"name: string @index(exact, exact) .", "name: string @index(term, term) .", "friend: [string .",
		"friend: [bool] .", "friend: uid @index(exact) .", "alive: bool @index(geo) .", "age: int @lang .",
		"name: string @reverse .", "[0xf2] name: string .", "email: string @index(exact) @upsert @unique .",
		`emb: float32vector @index(hnsw(metric: "euclidean", exponent: "4")) .`, "[", "[ ",
		// Go lowercases a rune by its simple mapping: İ (U+0130) becomes i.
		"p: <İnt> .", "p: <strİng> @index(<EXACT>, <İnt>) .", "p: <STRİNG> @index(<trİgram>) .", "name: string @ index(exact) .", "name: string @index (exact) .",
		"name: string @index(exact)@count .", "name: STRING @index(EXACT) .", "name: String @index(Term) .",
	}
)

func schemaCorpus() ([]string, error) {
	lits, err := schemaTestLiterals()
	if err != nil {
		return nil, err
	}
	corpus := append([]string{}, lits...)
	corpus = append(corpus, schemaMisc...)
	for _, t := range schemaTypeNames {
		corpus = append(corpus, "p: "+t+" .", "p: ["+t+"] .", "p: ["+t+"] @index(exact) .")
		for _, tk := range schemaTokenizerNames {
			corpus = append(corpus, "p: "+t+" @index("+tk+") .")
		}
		for _, d := range schemaDirectives {
			corpus = append(corpus, "p: "+t+" "+d+" .", "p: ["+t+"] "+d+" .")
		}
	}
	for _, t := range []string{"string", "datetime"} {
		for _, a := range schemaTokenizerNames {
			for _, b := range schemaTokenizerNames {
				corpus = append(corpus, "p: "+t+" @index("+a+", "+b+") .")
			}
		}
	}
	for _, v := range schemaVectorIndexes {
		corpus = append(corpus, "v: float32vector @index("+v+") .", "v: [float32vector] @index("+v+") .",
			"v: string @index("+v+") .")
	}
	for _, ns := range schemaNamespacePrefixes {
		corpus = append(corpus, ns+" name: string .", ns+"name: string @index(exact) .", ns,
			ns+" type T {\n name\n}", ns+" name: string .\n"+ns+" type T {\n name\n}\nage: int .",
			"name: string .\n"+ns+" age: int .", ns+" name: string @index(exact, exact) .")
	}
	seen := map[string]bool{}
	var out []string
	for _, s := range corpus {
		if !seen[s] {
			seen[s] = true
			out = append(out, s)
		}
	}
	return out, nil
}

func genSchemaParse() (any, error) {
	corpus, err := schemaCorpus()
	if err != nil {
		return nil, err
	}
	var cases []schemaParseCase
	for _, s := range corpus {
		cases = append(cases, schemaParseCase{
			Input: s,
			Parse: schemaRun(func() (*schema.ParsedSchema, error) { return schema.Parse(s) }),
			Ns5:   schemaRun(func() (*schema.ParsedSchema, error) { return schema.ParseWithNamespace(s, 5) }),
		})
	}
	return cases, nil
}

type schemaInitialCase struct {
	Namespace      uint64             `json:"namespace"`
	AclEnabled     bool               `json:"acl_enabled"`
	Schema         []schemaUpdateJSON `json:"schema"`
	Types          []schemaTypeJSON   `json:"types"`
	CompleteSchema []schemaUpdateJSON `json:"complete_schema"`
	CompleteTypes  []schemaTypeJSON   `json:"complete_types"`
}

func genSchemaInitial() (any, error) {
	saved := x.WorkerConfig.AclEnabled
	defer func() { x.WorkerConfig.AclEnabled = saved }()
	var cases []schemaInitialCase
	for _, ns := range []uint64{0, 5} {
		for _, acl := range []bool{false, true} {
			x.WorkerConfig.AclEnabled = acl
			cases = append(cases, schemaInitialCase{
				Namespace:      ns,
				AclEnabled:     acl,
				Schema:         schemaUpdatesToJSON(schema.InitialSchema(ns)),
				Types:          schemaTypesToJSON(schema.InitialTypes(ns)),
				CompleteSchema: schemaUpdatesToJSON(schema.CompleteInitialSchema(ns)),
				CompleteTypes:  schemaTypesToJSON(schema.CompleteInitialTypes(ns)),
			})
		}
	}
	return cases, nil
}

// schemaPredefCase checks every predicate and type a schema text parses into against the pre-defined ones:
// CheckAndModifyPreDefPredicate's answer and the update as it leaves it, and IsPreDefTypeChanged's answer.
type schemaPredefCase struct {
	Input       string             `json:"input"`
	Namespace   uint64             `json:"namespace"`
	PredChanged []bool             `json:"pred_changed"`
	PredsAfter  []schemaUpdateJSON `json:"preds_after"`
	TypeChanged []bool             `json:"type_changed"`
}

var schemaPredefInputs = []string{
	"dgraph.type: [string] @index(exact) .",
	"dgraph.type: string @index(exact) .",
	"dgraph.type: [string] @index(exact) @upsert .",
	"dgraph.drop.op: string .",
	"dgraph.graphql.schema: string .",
	"dgraph.graphql.xid: string @index(exact) @upsert .",
	"dgraph.graphql.xid: string @index(exact) .",
	"dgraph.graphql.p_query: string @index(sha256) .",
	"dgraph.namespace.name: string @index(exact) @unique @upsert .",
	"dgraph.namespace.name: string @index(exact) @upsert .",
	"dgraph.namespace.id: int @index(int) @unique @upsert .",
	"dgraph.xid: string @index(exact) @upsert @unique .",
	"dgraph.xid: string @index(exact) @upsert .",
	"dgraph.xid: string @index(exact) .",
	"dgraph.xid: string @index(hash) @upsert .",
	"dgraph.password: password .",
	"dgraph.user.group: [uid] @reverse .",
	"dgraph.user.group: [uid] .",
	"dgraph.acl.rule: [uid] .",
	"dgraph.rule.predicate: string @index(exact) @upsert .",
	"dgraph.rule.permission: int .",
	"dgraph.rule.permission: int @index(int) .",
	"dgraph.unknown: string .",
	"DGRAPH.TYPE: [string] @index(exact) .",
	"Dgraph.Xid: string @index(exact) @upsert .",
	"name: string .",
	"type dgraph.graphql {\n dgraph.graphql.schema\n dgraph.graphql.xid\n}",
	"type dgraph.graphql {\n dgraph.graphql.xid\n dgraph.graphql.schema\n}",
	"type dgraph.graphql {\n dgraph.graphql.schema\n}",
	"type dgraph.graphql.persisted_query {\n dgraph.graphql.p_query\n}",
	"type dgraph.namespace {\n dgraph.namespace.name\n dgraph.namespace.id\n}",
	"type dgraph.type.User {\n dgraph.xid\n dgraph.password\n dgraph.user.group\n}",
	"type dgraph.type.User {\n dgraph.xid\n dgraph.password\n}",
	"type dgraph.type.Group {\n dgraph.xid\n dgraph.acl.rule\n}",
	"type dgraph.type.Rule {\n dgraph.rule.predicate\n dgraph.rule.permission\n}",
	"type dgraph.type.Rule {\n dgraph.rule.permission\n dgraph.rule.predicate\n}",
	"type dgraph.unknown {\n name\n}",
	"type Person {\n name\n}",
}

func genSchemaPredefined() (any, error) {
	var cases []schemaPredefCase
	for _, ns := range []uint64{0, 5} {
		for _, s := range schemaPredefInputs {
			res, err := schema.ParseWithNamespace(s, ns)
			if err != nil {
				return nil, fmt.Errorf("parsing %q: %w", s, err)
			}
			c := schemaPredefCase{Input: s, Namespace: ns, PredChanged: []bool{}, TypeChanged: []bool{}}
			for _, p := range res.Preds {
				c.PredChanged = append(c.PredChanged, schema.CheckAndModifyPreDefPredicate(p))
			}
			c.PredsAfter = schemaUpdatesToJSON(res.Preds)
			for _, t := range res.Types {
				c.TypeChanged = append(c.TypeChanged, schema.IsPreDefTypeChanged(t))
			}
			cases = append(cases, c)
		}
	}
	return cases, nil
}
