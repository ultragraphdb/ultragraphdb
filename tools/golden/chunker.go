// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_chunker: ParseRDF's N-Quad or error for every input of Dgraph's testNQuads
// table (read from chunker/rdf_parser_test.go) and further variants, ParseRDFs over multi-line inputs with their
// type hints, and NQuadBuffer.ParseJSON (encoding/json, the parser FastParseJSON falls back to) for set and delete.
//
// JSON objects are Go maps, so the order of the N-Quads, of a JSON N-Quad's facets and the counter of each blank
// node depend on map iteration: the N-Quads are recorded as Go emits them with the random part of each blank node
// removed (`_:dg.<n>`), facets sorted by key, and every JSON case is parsed several times: a case whose sorted
// result differs between runs is marked unstable, and every error text a run gave is kept. A failed parse may
// have pushed some N-Quads first, which ones depending on map iteration.

import (
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"io"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/dgraph-io/dgo/v250/protos/api"
	"github.com/dgraph-io/dgraph/v25/chunker"
	"github.com/dgraph-io/dgraph/v25/lex"
	"github.com/dgraph-io/dgraph/v25/protos/pb"
)

func init() {
	register("chunker_rdf", "ultragraphdb_core_chunker", genChunkerRDF)
	register("chunker_rdfs", "ultragraphdb_core_chunker", genChunkerRDFs)
	register("chunker_json", "ultragraphdb_core_chunker", genChunkerJSON)
}

type valueJSON struct {
	Kind string `json:"kind"`
	Str  string `json:"str"`
	Hex  string `json:"hex"`
	Int  int64  `json:"int"`
	Bool bool   `json:"bool"`
	Uid  uint64 `json:"uid"`
}

type nquadJSON struct {
	Subject     string       `json:"subject"`
	Predicate   string       `json:"predicate"`
	ObjectId    string       `json:"object_id"`
	ObjectValue *valueJSON   `json:"object_value"`
	Lang        string       `json:"lang"`
	Facets      []*facetJSON `json:"facets"`
	Namespace   uint64       `json:"namespace"`
}

func valueToJSON(v *api.Value) *valueJSON {
	if v == nil {
		return nil
	}
	switch x := v.Val.(type) {
	case *api.Value_DefaultVal:
		return &valueJSON{Kind: "default", Str: x.DefaultVal}
	case *api.Value_BytesVal:
		return &valueJSON{Kind: "bytes", Hex: hex.EncodeToString(x.BytesVal)}
	case *api.Value_IntVal:
		return &valueJSON{Kind: "int", Int: x.IntVal}
	case *api.Value_BoolVal:
		return &valueJSON{Kind: "bool", Bool: x.BoolVal}
	case *api.Value_StrVal:
		return &valueJSON{Kind: "str", Str: x.StrVal}
	case *api.Value_DoubleVal:
		return &valueJSON{Kind: "double", Hex: fmt.Sprintf("%016x", math.Float64bits(x.DoubleVal))}
	case *api.Value_GeoVal:
		return &valueJSON{Kind: "geo", Hex: hex.EncodeToString(x.GeoVal)}
	case *api.Value_DateVal:
		return &valueJSON{Kind: "date", Hex: hex.EncodeToString(x.DateVal)}
	case *api.Value_DatetimeVal:
		return &valueJSON{Kind: "datetime", Hex: hex.EncodeToString(x.DatetimeVal)}
	case *api.Value_PasswordVal:
		return &valueJSON{Kind: "password", Str: x.PasswordVal}
	case *api.Value_UidVal:
		return &valueJSON{Kind: "uid", Uid: x.UidVal}
	case *api.Value_BigfloatVal:
		return &valueJSON{Kind: "bigfloat", Hex: hex.EncodeToString(x.BigfloatVal)}
	case *api.Value_Vfloat32Val:
		return &valueJSON{Kind: "vfloat32", Hex: hex.EncodeToString(x.Vfloat32Val)}
	default:
		return &valueJSON{Kind: "none"}
	}
}

var blankRe = regexp.MustCompile(`_:dg\.\d+\.(\d+)`)

func nquadToJSON(nq *api.NQuad, sortFacets bool) *nquadJSON {
	out := &nquadJSON{
		Subject:     blankRe.ReplaceAllString(nq.Subject, "_:dg.$1"),
		Predicate:   nq.Predicate,
		ObjectId:    blankRe.ReplaceAllString(nq.ObjectId, "_:dg.$1"),
		ObjectValue: valueToJSON(nq.ObjectValue),
		Lang:        nq.Lang,
		Facets:      []*facetJSON{},
		Namespace:   nq.Namespace,
	}
	for _, f := range nq.Facets {
		out.Facets = append(out.Facets, facetToJSON(f))
	}
	if sortFacets {
		sort.Slice(out.Facets, func(i, j int) bool { return out.Facets[i].Key < out.Facets[j].Key })
	}
	return out
}

type rdfCase struct {
	Input       string     `json:"input"`
	InputHex    string     `json:"input_hex"`
	Valid       bool       `json:"valid_utf8"`
	Empty       bool       `json:"empty"`
	Err         string     `json:"err"`
	Panic       string     `json:"panic"`
	NQuad       *nquadJSON `json:"nquad"`
	TableErr    bool       `json:"table_expected_err"`
	TableIgnore bool       `json:"table_should_ignore"`
	FromTable   bool       `json:"from_table"`
}

type tableEntry struct {
	input        string
	expectedErr  bool
	shouldIgnore bool
}

// testNQuadsTable reads the inputs of the testNQuads table from Dgraph's chunker/rdf_parser_test.go.
func testNQuadsTable() ([]tableEntry, error) {
	root, err := filepath.Abs("../..")
	if err != nil {
		return nil, err
	}
	fork := os.Getenv("DGRAPH_SRC")
	if fork == "" {
		fork = filepath.Join(root, "..", "..", "fork", "dgraph")
	}
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, filepath.Join(fork, "chunker", "rdf_parser_test.go"), nil, 0)
	if err != nil {
		return nil, err
	}
	var entries []tableEntry
	ast.Inspect(f, func(n ast.Node) bool {
		vs, ok := n.(*ast.ValueSpec)
		if !ok || len(vs.Names) != 1 || vs.Names[0].Name != "testNQuads" {
			return true
		}
		for _, elt := range vs.Values[0].(*ast.CompositeLit).Elts {
			var e tableEntry
			for _, kv := range elt.(*ast.CompositeLit).Elts {
				kv := kv.(*ast.KeyValueExpr)
				switch kv.Key.(*ast.Ident).Name {
				case "input":
					s, err := strconv.Unquote(kv.Value.(*ast.BasicLit).Value)
					if err != nil {
						panic(err)
					}
					e.input = s
				case "expectedErr":
					e.expectedErr = kv.Value.(*ast.Ident).Name == "true"
				case "shouldIgnore":
					e.shouldIgnore = kv.Value.(*ast.Ident).Name == "true"
				}
			}
			entries = append(entries, e)
		}
		return false
	})
	if len(entries) == 0 {
		return nil, errors.New("testNQuads not found")
	}
	return entries, nil
}

// rdfVariants are further lines: every type of the type map, labels, facets, functions, stars, comments, spaces,
// and bytes that are not UTF-8.
var rdfVariants = []string{
	`<a> <age> "013"^^<xs:int> .`,
	`<a> <name> "chat"@fr .`,
	`<alice> * * .`,
	`<alice> <knows> .`,
	`<a> <p> "-13"^^<xs:int> .`,
	`<a> <p> "0x10"^^<xs:int> .`,
	`<a> <p> "9223372036854775808"^^<xs:int> .`,
	`<a> <p> "1.5"^^<xs:int> .`,
	`<a> <p> "13"^^<xs:positiveInteger> .`,
	`<a> <p> "13"^^<http://www.w3.org/2001/XMLSchema#integer> .`,
	`<a> <p> "13"^^<http://www.w3.org/2001/XMLSchema#int> .`,
	`<a> <p> "13"^^<http://www.w3.org/2001/XMLSchema#positiveInteger> .`,
	`<a> <p> "1.5"^^<xs:double> .`,
	`<a> <p> "1e400"^^<xs:double> .`,
	`<a> <p> "inf"^^<xs:float> .`,
	`<a> <p> "1.5"^^<http://www.w3.org/2001/XMLSchema#double> .`,
	`<a> <p> "1.5"^^<http://www.w3.org/2001/XMLSchema#float> .`,
	`<a> <p> "true"^^<xs:boolean> .`,
	`<a> <p> "false"^^<xs:boolean> .`,
	`<a> <p> "1"^^<xs:boolean> .`,
	`<a> <p> "T"^^<xs:boolean> .`,
	`<a> <p> "yes"^^<xs:boolean> .`,
	`<a> <p> "true"^^<http://www.w3.org/2001/XMLSchema#boolean> .`,
	`<a> <p> "2006-01-02T15:04:05Z"^^<xs:dateTime> .`,
	`<a> <p> "2006-01-02T15:04:05+07:00"^^<xs:dateTime> .`,
	`<a> <p> "2006-01-02"^^<xs:date> .`,
	`<a> <p> "2006"^^<xs:date> .`,
	`<a> <p> "2006-13-02"^^<xs:date> .`,
	`<a> <p> "2006-01-02T15:04:05"^^<http://www.w3.org/2001/XMLSchema#dateTime> .`,
	`<a> <p> "2006-01-02"^^<http://www.w3.org/2001/XMLSchema#date> .`,
	`<a> <p> "2006"^^<http://www.w3.org/2001/XMLSchema#gYear> .`,
	`<a> <p> "2006-01"^^<http://www.w3.org/2001/XMLSchema#gYearMonth> .`,
	`<a> <p> "abc"^^<http://www.w3.org/2001/XMLSchema#string> .`,
	`<a> <p> "aGVsbG8="^^<xs:base64Binary> .`,
	`<a> <p> ""^^<xs:base64Binary> .`,
	`<a> <p> "1.25"^^<xs:decimal> .`,
	`<a> <p> "123456789012345678901234567890.5"^^<xs:decimal> .`,
	`<a> <p> "abc"^^<xs:decimal> .`,
	`<a> <p> "[1.5, 2, -3e2]"^^<xs:[]float32> .`,
	`<a> <p> "[]"^^<xs:[]float32> .`,
	`<a> <p> "abc"^^<xs:[]float32> .`,
	`<a> <p> "{'type':'Point','coordinates':[1.1,2.0]}"^^<geo:geojson> .`,
	`<a> <p> "{'type':'Polygon','coordinates':[[[0,0],[1,0],[1,1],[0,0]]]}"^^<geo:geojson> .`,
	`<a> <p> "{'type':'Point'}"^^<geo:geojson> .`,
	`<a> <p> "nope"^^<geo:geojson> .`,
	`<a> <p> "null"^^<geo:geojson> .`,
	`<a> <p> "secret"^^<xs:password> .`,
	`<a> <p> ""^^<xs:password> .`,
	`<a> <p> "x"^^<xs:unknown> .`,
	`<a> <p> ""^^<xs:boolean> .`,
	`<a> <p> "x"^^<*> .`,
	`<a> * "x"^^<xs:string> .`,
	`* * * .`,
	`* * "x" .`,
	`<a> * <b> .`,
	`<a> <p> * <0x5> .`,
	`<a> <p> "x"@en <5> .`,
	`<a> <p> "x" <0> .`,
	`<a> <p> "x" <0b101> .`,
	`<a> <p> "x" <0o7> .`,
	`<a> <p> "x" <07> .`,
	`<a> <p> "x" <1_000> .`,
	`<a> <p> "x" <18446744073709551615> .`,
	`<a> <p> "x" <18446744073709551616> .`,
	`<a> <p> "x" <-1> .`,
	`<a> <p> "x" <abc> .`,
	`<a> <p> "x" _:label .`,
	`<a> <p> "x" <5> <6> .`,
	`<a> <p> <b> <5> .`,
	`<a> <p> "x" (k=1) <5> .`,
	`<a> <p> "x" <5> (k=1) .`,
	`<a> <p> <b> (k="v", n=2.5, t=true) .`,
	`<a> <p> <b> (k="v") (j=1) .`,
	`<a> <p> <b> () .`,
	`<a> <p> <b> (k) .`,
	`<a> <p> <b> (k=abc) .`,
	`<a> <p> <b> (k="a\'b") .`,
	`<a> <p> <b> (k="a\x41") .`,
	`<a> <p> <b> (k="unterminated) .`,
	`<a> <p> <b> (k=1,) .`,
	`<a> <p> <b> (=1) .`,
	`<a> <p> <b> (k==1) .`,
	`<a> <p> <b> (k=1 2) .`,
	`<a> <p> <b> (k=1)`,
	`<a> <p> <b> (k=1) . # done`,
	`<a> <p> <b> (k=1, k=2) .`,
	`<a> <p> "x" (k=NaN) .`,
	`<a> <p> "x" (k=1e400) .`,
	`<a> <p> "x" (k=99999999999999999999) .`,
	`<a> <p> "x" (k=2006-01-02) .`,
	`<a> <p> "x" (k="2006-01-02") .`,
	`<a> <p> "x" (k="Ünïcödé Straße") .`,
	`uid(v) <p> uid(w) .`,
	`val(v) <p> "x" .`,
	`<a> <p> val(v) .`,
	`uid(v)<p> "x" .`,
	`uid (v) <p> "x" .`,
	`uid(v) (k=1) <p> "x" .`,
	`<a> uid(v) "x" .`,
	`<a> <p> uid(v) (k=1) .`,
	`<a> <p> uid(v) <5> .`,
	`uid(  ) <p> "x" .`,
	`uid(v <p> "x" .`,
	`uxd(v) <p> "x" .`,
	`vaal(v) <p> "x" .`,
	`0x1 <p> "x" .`,
	`0x1 <p> 0x2 .`,
	`123 <p> "x" .`,
	`0x1<p> "x" .`,
	`0xg <p> "x" .`,
	`18446744073709551616 <p> "x" .`,
	`<a> <p> 0x2 .`,
	`_:a <p> _:b .`,
	`_:a.b.c <p> _:b-c .`,
	`_:a: <p> _:b .`,
	`_:a<p> _:b .`,
	`_: <p> _:b .`,
	`_:a. <p> _:b .`,
	`_:0 <p> _:9 .`,
	`_:é <p> _:ñ .`,
	`_:a·b <p> _:x‿y .`,
	`_a <p> <b> .`,
	`_:a <p> _:b`,
	`_:a <p> _:b. `,
	`<a> <p> "x".`,
	`<a> <p> "x"   .   `,
	`<a> <p> "x" . <b>`,
	`<a> <p> "x" .# c`,
	`<a> <p> "x" . . `,
	`<a> <p> "x" ! ? .`,
	`<a> <p> "x" .. `,
	`<a><p><b>.`,
	"<a>\t<p>\t\"x\"\t.",
	"\t <a> <p> \"x\" . \t",
	" <a> <p> \"x\" . ",
	" <a> <p> \"x\" .",
	"<a> <p> \"x\" .\r",
	"<a> <p> \"x\r\" .",
	"<a> <p> \"x\ny\" .",
	`#`,
	`# <a> <p> <b> .`,
	`   # indented comment`,
	``,
	`.`,
	`<a> .`,
	`<a> <p> . .`,
	`<> <p> <b> .`,
	`<a> <> <b> .`,
	`<a> <p> <> .`,
	`<a> <p> "" .`,
	`<-> <p> <b> .`,
	`<a> <-> <b> .`,
	`<a> <p> <-> .`,
	`<a> <p> <!> .`,
	`<a> <ⅳ> <b> .`,
	`<a> <²> <b> .`,
	`<a> <٣> <b> .`,
	`<a> <名前> "値" .`,
	`<a> <p@en> "x" .`,
	`<a> <p@> "x" .`,
	`<a> <@en> "x" .`,
	`<a> <p@en> "x"@de .`,
	`<a> <p> "x"@en-US-x-private .`,
	`<a> <p> "x"@1en .`,
	`<a> <p> "x"@ .`,
	`<a> <p> "x"@en^^<xs:string> .`,
	`<a> <p> "x"^^<xs:string>@en .`,
	`<a> <p> "x"^^<xs:string> "y" .`,
	`<a> <p> "x" "y" .`,
	`<a> <p> "x" <b> .`,
	`<a> <p> <b> "x" .`,
	`<a> <p> "é\U0001F600" .`,
	`<a> <p> "\uD800" .`,
	`<a> <p> "\U00110000" .`,
	`<a> <p> "\x41\x42" .`,
	`<a> <p> "\xff" .`,
	`<a> <p> "\xe2\x82" .`,
	`<a> <p> "\xe2\x82\xac" .`,
	`<a> <p> "\'" .`,
	`<a> <p> "a\\" .`,
	`<a> <p> "a\" .`,
	`<a> <p> "\101" .`,
	`<a> <A> "x" .`,
	`<a> <\u004> "x" .`,
	`<a> <\x41> "x" .`,
	"<a> <p> \"\xff\" .",
	"<a\xff> <p> \"x\" .",
	"<a> <p> \"\xe2\x82\" .",
	"<a> <p> <b> (k=\"\xff\") .",
	"<a> <p\xc0> \"x\" .",
	"\xff",
	// Bytes that are not UTF-8 before an error: Go counts each as one byte of the column.
	"<a\xff> \xfe",
	"<a\xff> \xfe .",
	"<a\xff\xfe\xfd> !",
	"<a\xe2\x82> !",
	"<a\xf0\x9f\x98> <p> ~",
	"<a\xff><p\xfe> \"x\" ~",
	"\"\xff\" <p> <b> .",
	"<a\xff> <p> \"x\"@\xfe .",
	"<a\xff> <p> \"x\"^^<xs:int> <\xfe",
	"<a\xff> <p> \"unterminated",
	"<a\xff> <p> \"x\\q\" .",
	"<a\xff>\r<p\xfe> \"x\" ~ .",
	"_:a\xff <p> _:b\xfe ~",
	"<a\xff> <p> <b> (k=\xff) ~",
	"<a\xff> <p\xfe> <b\xfd> . ~",
	"<a> <p> \"\xff\" ~",
	"<\xff\xff\xff\xff\xff> <p> \"x\" ! .",
	"<a\xef\xbf\xbd\xff> !",
}

func rdfCaseFor(input string) (c rdfCase) {
	c = rdfCase{Input: input, InputHex: hex.EncodeToString([]byte(input))}
	c.Valid = strings.ToValidUTF8(input, "\x00") == input
	// `"null"^^<geo:geojson>` makes Dgraph's ObjectValue panic on its unchecked type assertion.
	defer func() {
		if r := recover(); r != nil {
			c.Panic = fmt.Sprint(r)
		}
	}()
	nq, err := chunker.ParseRDF(input, &lex.Lexer{})
	switch {
	case errors.Is(err, chunker.ErrEmpty):
		c.Empty = true
		c.Err = err.Error()
	case err != nil:
		c.Err = err.Error()
	default:
		c.NQuad = nquadToJSON(&nq, false)
	}
	return c
}

func genChunkerRDF() (any, error) {
	table, err := testNQuadsTable()
	if err != nil {
		return nil, err
	}
	var cases []rdfCase
	for _, e := range table {
		c := rdfCaseFor(e.input)
		c.FromTable = true
		c.TableErr = e.expectedErr
		c.TableIgnore = e.shouldIgnore
		cases = append(cases, c)
	}
	for _, v := range rdfVariants {
		cases = append(cases, rdfCaseFor(v))
	}
	return cases, nil
}

type hintJSON struct {
	Pred string `json:"pred"`
	Hint int    `json:"hint"`
}

type rdfsCase struct {
	Name   string       `json:"name"`
	Input  string       `json:"input"`
	Err    string       `json:"err"`
	NQuads []*nquadJSON `json:"nquads"`
	Hints  []hintJSON   `json:"hints"`
}

func hintsToJSON(md *pb.Metadata) []hintJSON {
	hints := []hintJSON{}
	if md == nil {
		return hints
	}
	for p, h := range md.PredHints {
		hints = append(hints, hintJSON{Pred: p, Hint: int(h)})
	}
	sort.Slice(hints, func(i, j int) bool { return hints[i].Pred < hints[j].Pred })
	return hints
}

func genChunkerRDFs() (any, error) {
	root, err := filepath.Abs("../..")
	if err != nil {
		return nil, err
	}
	fork := os.Getenv("DGRAPH_SRC")
	if fork == "" {
		fork = filepath.Join(root, "..", "..", "fork", "dgraph")
	}
	data, err := os.ReadFile(filepath.Join(fork, "chunker", "data.rdf"))
	if err != nil {
		return nil, err
	}
	inputs := []struct{ name, input string }{
		{"empty", ""},
		{"blank lines and comments", "\n\n# c\n   \n"},
		{"one line", `<a> <p> "x" .`},
		{"single and list", "<a> <p> \"x\" .\n<a> <q> \"y\" .\n<a> <q> \"z\" .\n<b> <p> \"x\" .\n"},
		{"same pair different subjects", "<a> <p> \"x\" .\n<b> <p> \"y\" .\n"},
		{"crlf", "<a> <p> \"x\" .\r\n<a> <p> \"y\" .\r\n"},
		{"error on line two", "<a> <p> \"x\" .\n<a> <p> .\n<a> <p> \"y\" .\n"},
		{"comment after statement", "<a> <p> \"x\" . # c\n# only\n<a> <p> \"y\" .\n"},
		{"star lines", "<a> * * .\n<a> <p> * .\n"},
		{"invalid utf8", "<a> <p> \"\xff\" .\n<a\xfe> <p> \"x\" .\n"},
		{"invalid utf8 before an error", "<a> <p> \"\xff\" .\n<a\xfe> \xfd <p> \"x\" .\n"},
		{"data.rdf", string(data)},
	}
	var cases []rdfsCase
	for _, in := range inputs {
		c := rdfsCase{Name: in.name, Input: hex.EncodeToString([]byte(in.input)), NQuads: []*nquadJSON{}}
		nqs, md, err := chunker.ParseRDFs([]byte(in.input))
		if err != nil {
			c.Err = err.Error()
		} else {
			for _, nq := range nqs {
				c.NQuads = append(c.NQuads, nquadToJSON(nq, false))
			}
		}
		c.Hints = hintsToJSON(md)
		cases = append(cases, c)
	}
	return cases, nil
}

type jsonCase struct {
	Name     string       `json:"name"`
	Input    string       `json:"input"`
	InputHex string       `json:"input_hex"`
	Op       int          `json:"op"`
	Err      string       `json:"err"`
	Errs     []string     `json:"errs"`
	ErrJSON  bool         `json:"err_json"`
	NQuads   []*nquadJSON `json:"nquads"`
	Hints    []hintJSON   `json:"hints"`
	Stable   bool         `json:"stable"`
	Blanks   int          `json:"blanks"`
	Repeated int          `json:"runs"`
}

type jsonRun struct {
	err    string
	nqs    []*api.NQuad
	md     *pb.Metadata
	errTyp bool
}

func parseJSONOnce(input string, op int) jsonRun {
	buf := chunker.NewNQuadBuffer(-1)
	err := buf.ParseJSON([]byte(input), op)
	buf.Flush()
	nqs := <-buf.Ch()
	r := jsonRun{nqs: nqs, md: buf.Metadata()}
	if err != nil {
		r.err = err.Error()
		var se *json.SyntaxError
		var te *json.UnmarshalTypeError
		r.errTyp = errors.As(err, &se) || errors.As(err, &te) || errors.Is(err, io.EOF) ||
			errors.Is(err, io.ErrUnexpectedEOF)
	}
	return r
}

// sortedKey is a run's result with blank-node counters dropped, N-Quads and facets sorted: what must not depend on
// map iteration.
func sortedKey(r jsonRun) string {
	var lines []string
	counter := regexp.MustCompile(`_:dg\.\d+`)
	for _, nq := range r.nqs {
		b, _ := json.Marshal(nquadToJSON(nq, true))
		lines = append(lines, counter.ReplaceAllString(string(b), "_:dg"))
	}
	sort.Strings(lines)
	h, _ := json.Marshal(hintsToJSON(r.md))
	return r.err + "\n" + strings.Join(lines, "\n") + "\n" + string(h)
}

type jsonSchool struct {
	Name string `json:"name,omitempty"`
}

type jsonAddress struct {
	Type   string    `json:"type,omitempty"`
	Coords []float64 `json:"coordinates,omitempty"`
}

// jsonPerson is Person of Dgraph's json_parser_test.go.
type jsonPerson struct {
	Uid       string       `json:"uid,omitempty"`
	Namespace string       `json:"namespace,omitempty"`
	Name      string       `json:"name,omitempty"`
	Age       int          `json:"age,omitempty"`
	Married   *bool        `json:"married,omitempty"`
	Now       *time.Time   `json:"now,omitempty"`
	Address   jsonAddress  `json:"address,omitempty"`
	Friends   []jsonPerson `json:"friend,omitempty"`
	School    *jsonSchool  `json:"school,omitempty"`
}

// jsonProduct is Product of Dgraph's json_parser_test.go.
type jsonProduct struct {
	Uid           string `json:"uid,omitempty"`
	Name          string `json:"name"`
	Description   string `json:"description"`
	Description_v string `json:"description_v"`
}

func marshal(v any) string {
	b, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	return string(b)
}

func jsonInputs() []struct{ name, input string } {
	tn := time.Date(2026, 10, 20, 10, 11, 12, 123456789, time.UTC)
	married, notMarried := true, false
	facets1 := fmt.Sprintf(`[{"name":"Alice","mobile":"040123456","car":"MA0123",`+
		`"mobile|operation": "%s",
         "car|first":true,
         "car|age": %d,
         "car|price": %f,
         "car|since": "%s"
}]`, "READ WRITE", 3, 30000.56, "2006-01-02T15:04:05Z")
	return []struct{ name, input string }{
		// Dgraph's json_parser_test.go.
		{"Json1", marshal(jsonPerson{Uid: "1", Namespace: "0x2", Name: "Alice", Age: 26, Married: &married, Now: &tn,
			Address: jsonAddress{Type: "Point", Coords: []float64{1.1, 2.0}}})},
		{"Json2", marshal(jsonPerson{Name: "Alice", Friends: []jsonPerson{{Name: "Charlie", Married: &notMarried},
			{Uid: "1000", Name: "Bob"}}})},
		{"Json3", marshal(jsonPerson{Name: "Alice", School: &jsonSchool{Name: "Wellington Public School"}})},
		{"Json4", `[{"name":"Alice","mobile":"040123456","car":"MA0123", "age": 21, "weight": 58.7}]`},
		{"JsonMap", "{\"name\":\"Alice\",\n\"age\": 25,\n\"friends\": [{\n\"name\": \"Bob\"\n}]}"},
		{"MultipleJsonObjects", `[{"name": "A","age": 25,"friends": [{"name": "A1","friends": [{"name": "A11"},
			{"name": "A12"}]},{"name": "A2","friends": [{"name": "A21"},{"name": "A22"}]}]},{"name": "B","age": 26,
			"friends": [{"name": "B1","friends": [{"name": "B11"},{"name": "B12"}]},{"name": "B2","friends": [
			{"name": "B21"},{"name": "B22"}]}]}]`},
		{"JsonNumberParsing int", `{"uid": "1", "key": 9223372036854775299}`},
		{"JsonNumberParsing float", `{"uid": "1", "key": 9223372036854775299.0}`},
		{"JsonNumberParsing too big", `{"uid": "1", "key": 27670116110564327426}`},
		{"JsonNumberParsing string", `{"uid": "1", "key": "23452786"}`},
		{"JsonNumberParsing float string", `{"uid": "1", "key": "23452786.2378"}`},
		{"JsonNumberParsing exponent", `{"uid": "1", "key": -1e10}`},
		{"JsonNumberParsing zero exponent", `{"uid": "1", "key": 0E-0}`},
		{"UidOutofRange", `{"uid":"0xa14222b693e4ba34123","name":"Name","following":[{"name":"Bob"}],"school":[{"uid":"","name@en":"Crown Public School"}]}`},
		{"JsonArray", `[{"uid": "uid(Project10)","Ticket.row": {"uid": "uid(x)"}},{"Project.columns": [{"uid": "uid(x)"}],
			"uid": "uid(Project3)"},{"Ticket.onColumn": {"uid": "uid(x)"},"uid": "uid(Ticket4)"}]`},
		{"NegativeUid", `{"uid":"-100","name":"Name","following":[{"name":"Bob"}],"school":[{"uid":"","name@en":"Crown Public School"}]}`},
		{"EmptyUid", "{\"uid\":\"\",\"name\":\"Alice\",\"following\":[{\"name\":\"Bob\"}],\"school\":[{\"uid\":\"\",\n\"name\":\"Crown Public School\"}]}"},
		{"BlankNodes", `{"uid":"_:alice","name":"Alice","following":[{"name":"Bob"}],"school":[{"uid":"_:school","name":"Crown Public School"}]}`},
		{"DeleteEdges", `[{"uid": "0x1","name":null,"mobile":null,"car":null}]`},
		{"Facets1", facets1},
		{"Facets2", `[{"name":"Alice","friend":[{"name":"Dave","friend|close":"true"}]}]`},
		{"Facets3", `[{"name":"Alice","friend": ["Joshua", "David", "Josh"],"friend|from": {"0": "school","2": "college"},
			"friend|age": {"1": 20,"2": 21}}]`},
		{"Facets4 facets_should_be_map", `[{"name":"Alice","friend": ["Joshua", "David", "Josh"],"friend|age": 20}]`},
		{"Facets4 predicate_should_be_list", `[{"name":"Alice","friend": "Joshua","friend|age": {"0": 20}}]`},
		{"Facets4 only_scalar_values_in_facet_map", `[{"name":"Alice","friend": ["Joshua"],"friend|age": {"0": {"1": 20}}}]`},
		{"Facets4 invalid_key_in_facet_map", `[{"name":"Alice","friend": ["Joshua"],"friend|age": {"a": 20}}]`},
		{"Facets4 predicate_is_null", `[{"name":"Alice","friend": null,"friend|age": {"0": 20}}]`},
		{"Facets4 empty_scalar_list", `[{"name":"Alice","friend": [],"friend|age": {"0": 20}}]`},
		{"Facets4 facet_map_is_null", `[{"name":"Alice","friend": ["Joshua"],"friend|age": null}]`},
		{"Facets4 facet_vales_should_not_be_list", `[{"name":"Alice","friend": ["Joshua", "David", "Josh"],"friend|age": ["20"]}]`},
		{"Facets4 facet_map_with_index_greater_than_scalarlist_length", `[{"name":"Alice","friend": ["Joshua", "David", "Josh"],"friend|age": {"100": 30,"20": 28}}]`},
		{"Facets5", `[{"name": "Alice","friend": [{"name": "Dave","friend|close": true,"friend": [{"name": "Emily","friend|close": true}]}]}]`},
		{"Error1", marshal(jsonPerson{Name: "Alice", School: &jsonSchool{Name: "Wellington Public School"}})},
		{"List", `{"address":["Riley Street","Redfern"],"phone_number":[123,9876],"points":[{"type":"Point", "coordinates":[1.1,2.0]},{"type":"Point", "coordinates":[2.0,1.1]}]}`},
		{"Delete", `{"uid":1000,"friend":[{"uid":1001}]}`},
		{"DeleteStar", `{"uid":1000,"name": null}`},
		{"ValInUpsert", `{"uid":1000, "name": "val(name)"}`},
		{"DeleteStarLang", `{"uid":1000,"name@es": null}`},
		{"EmptyFacet", `{"uid":1000,"doesnt|exist":null}`},
		{"EmptyString", `[{"name":""}]`},
		{"EmptyStringVectorPred", marshal([]jsonProduct{{Uid: "1"}})},
		{"EmptySquareBracketVectorPred", marshal(jsonProduct{Name: "ipad", Description_v: "[]"})},
		{"ValidVector", marshal(jsonProduct{Name: "ipad", Description_v: "[1.1, 2.2, 3.3]"})},
		// The mutation-input spec.
		{"spec nested", `{"uid": "_:a", "name": "A", "friend": {"uid": "_:b", "name": "B"}}`},
		{"spec facet", `{"uid": "_:a", "friend": {"uid": "_:b", "friend|close": true}}`},
		{"spec negative uid", `{"uid": "-100", "name": "x"}`},
		{"spec no uid", `{"name": "x"}`},
		// Roots.
		{"empty", ``},
		{"spaces", "  \n "},
		{"null", `null`},
		{"empty object", `{}`},
		{"empty list", `[]`},
		{"string root", `"abc"`},
		{"number root", `123`},
		{"list of numbers", `[1]`},
		{"list with null", `[{"name":"a"}, null]`},
		{"trailing data", `{"name":"a"} xyz`},
		{"trailing value after list", `[{"name":"a"}] [{"name":"b"}]`},
		{"trailing garbage after list", `[{"name":"a"}] xyz`},
		{"syntax error", `{"name":}`},
		{"truncated", `{"name":"a"`},
		{"invalid utf8 in string", "{\"name\":\"a\xffb\"}"},
		{"invalid utf8 outside string", "{\"name\":\"a\"\xff}"},
		{"escapes", `{"name":"a\"b\\c\/dé😀\n"}`},
		{"lone surrogate", `{"name":"a\ud800b"}`},
		{"duplicate keys", `{"name":"a","name":"b"}`},
		// uid and namespace.
		{"uid number", `{"uid": 5, "name": "x"}`},
		{"uid zero", `{"uid": 0, "name": "x"}`},
		{"uid negative number", `{"uid": -5, "name": "x"}`},
		{"uid float", `{"uid": 1.5, "name": "x"}`},
		{"uid exponent", `{"uid": 1e3, "name": "x"}`},
		{"uid string zero", `{"uid": "0", "name": "x"}`},
		{"uid hex", `{"uid": "0x1f", "name": "x"}`},
		{"uid octal", `{"uid": "017", "name": "x"}`},
		{"uid underscores", `{"uid": "1_000", "name": "x"}`},
		{"uid spaces", `{"uid": " 0x1 ", "name": "x"}`},
		{"uid blank spaces", `{"uid": " _:a", "name": "x"}`},
		{"uid function", `{"uid": "uid( v )", "name": "x"}`},
		{"uid bool", `{"uid": true, "name": "x"}`},
		{"uid null", `{"uid": null, "name": "x"}`},
		{"uid list", `{"uid": [1], "name": "x"}`},
		{"uid only", `{"uid": "0x1"}`},
		{"namespace number", `{"uid": "0x1", "namespace": 5, "name": "x"}`},
		{"namespace hex string", `{"uid": "0x1", "namespace": "0x10", "name": "x"}`},
		{"namespace empty string", `{"uid": "0x1", "namespace": "", "name": "x"}`},
		{"namespace spaces", `{"uid": "0x1", "namespace": " 7 ", "name": "x"}`},
		{"namespace bad string", `{"uid": "0x1", "namespace": "abc", "name": "x"}`},
		{"namespace float", `{"uid": "0x1", "namespace": 1.5, "name": "x"}`},
		{"namespace bool", `{"uid": "0x1", "namespace": true, "name": "x"}`},
		{"namespace nested", `{"uid": "0x1", "namespace": 3, "friend": {"name": "y"}}`},
		{"namespace nested own", `{"uid": "0x1", "namespace": 3, "friend": {"namespace": 4, "name": "y"}}`},
		// Values.
		{"bools", `{"uid": "0x1", "t": true, "f": false}`},
		{"numbers", `{"uid": "0x1", "i": -7, "f": 1.25, "e": 1E5, "z": 0, "zf": 0.0}`},
		{"big numbers", `{"uid": "0x1", "u": 18446744073709551615}`},
		{"huge float", `{"uid": "0x1", "f": 1e400}`},
		{"tiny float", `{"uid": "0x1", "f": 1e-400}`},
		{"float rounding", `{"uid": "0x1", "f": 0.1000000000000000055511151231257827021181583404541015625}`},
		{"uid and val functions", `{"uid": "0x1", "a": "uid(x)", "b": " val ( y ) ", "c": "uid(x"}`},
		{"vectors", `{"uid": "0x1", "v": "[1, 2.5, -3]", "e": "[]", "s": "[1, abc]", "w": " [ 0.5 ] "}`},
		{"languages", `{"uid": "0x1", "name@en": "x", "name@": "y", "@en": "z", "a@b@c": "w"}`},
		{"list with language", `{"uid": "0x1", "tags@en": ["a", "b"]}`},
		{"lone surrogate in key", `{"na\ud800me": "x"}`},
		{"empty nested object", `{"uid": "0x1", "p": {}}`},
		{"nested null", `{"uid": "0x1", "p": {"uid": "0x2", "q": null}}`},
		{"list of objects", `{"uid": "0x1", "p": [{"uid": "0x2"}, {"name": "b"}]}`},
		{"list mixed", `{"uid": "0x1", "p": [1, {"name": "b"}]}`},
		{"list with bool", `{"uid": "0x1", "p": [1, true]}`},
		{"list with null item", `{"uid": "0x1", "p": ["a", null]}`},
		{"list of lists", `{"uid": "0x1", "p": [[1]]}`},
		{"list starting with bool", `{"uid": "0x1", "p": [true]}`},
		{"list starting with object", `{"uid": "0x1", "p": [{"name": "a"}, "b"], "p|f": {"1": "x"}}`},
		{"empty list value", `{"uid": "0x1", "p": []}`},
		// Geo.
		{"geo point", `{"uid": "0x1", "loc": {"type": "Point", "coordinates": [1.1, 2.0]}}`},
		{"geo polygon", `{"uid": "0x1", "loc": {"type": "Polygon", "coordinates": [[[0,0],[1,0],[1,1],[0,0]]]}}`},
		{"geo bad coordinates", `{"uid": "0x1", "loc": {"type": "Point", "coordinates": "x"}}`},
		{"geo unknown type", `{"uid": "0x1", "loc": {"type": "Blob", "coordinates": [1, 2]}}`},
		{"geo three keys", `{"uid": "0x1", "loc": {"type": "Point", "coordinates": [1, 2], "name": "x"}}`},
		{"geo with uid", `{"uid": "0x1", "loc": {"uid": "0x2", "coordinates": [1, 2]}}`},
		{"geo list", `{"uid": "0x1", "loc": [{"type": "Point", "coordinates": [1, 2]}, {"type": "Point", "coordinates": [3, 4]}]}`},
		{"geo with facet", `{"uid": "0x1", "loc": {"type": "Point", "coordinates": [1, 2]}, "loc|since": "2006"}`},
		// Facets.
		{"facet types", `{"uid": "0x1", "p": "x", "p|s": "READ WRITE", "p|i": 3, "p|f": 1.5, "p|e": 1e5, "p|b": false, "p|d": "2006-01-02T15:04:05Z", "p|y": "2006"}`},
		{"facet big int", `{"uid": "0x1", "p": "x", "p|i": 99999999999999999999}`},
		{"facet list value", `{"uid": "0x1", "p": "x", "p|l": [1]}`},
		{"facet object value", `{"uid": "0x1", "p": "x", "p|o": {"a": 1}}`},
		{"facet null", `{"uid": "0x1", "p": "x", "p|n": null}`},
		{"facet without predicate", `{"uid": "0x1", "q|f": 1}`},
		{"facet on edge", `{"uid": "0x1", "p": {"uid": "0x2", "p|w": 0.5, "q": "y", "q|z": 1}}`},
		{"facet on edge from parent", `{"uid": "0x1", "p": {"uid": "0x2"}, "p|w": 0.5}`},
		{"facet on edge list", `{"uid": "0x1", "p": [{"uid": "0x2", "p|w": 1}, {"uid": "0x3", "p|w": 2}]}`},
		{"facet with language", `{"uid": "0x1", "name@en": "x", "name@en|f": 1}`},
		{"facet string escapes", `{"uid": "0x1", "p": "x", "p|s": "a\"b\nc\\"}`},
		{"facet empty string", `{"uid": "0x1", "p": "x", "p|s": ""}`},
		{"facet key with pipe", `{"uid": "0x1", "p": "x", "p|a|b": 1}`},
		{"scalar list facets", `{"uid": "0x1", "p": ["a", "b", "c"], "p|f": {"0": 1, "2": "z", "-1": 3, "+1": true}}`},
		{"scalar list facet bad value", `{"uid": "0x1", "p": ["a"], "p|f": {"0": [1]}}`},
		{"scalar list facet null value", `{"uid": "0x1", "p": ["a"], "p|f": {"0": null}}`},
		{"scalar list number facets", `{"uid": "0x1", "p": [1, 2], "p|f": {"1": 2.5}}`},
		// encoding/json's scanner, every context of its errors (Go 1.27, whose encoding/json runs on jsontext).
		{"scan start x", `x`},
		{"scan start close bracket", `]`},
		{"scan start close brace", `}`},
		{"scan start colon", `:`},
		{"scan start comma", `,`},
		{"scan start comma then value", `, {}`},
		{"scan start backslash", ` \a`},
		{"scan start control", "\x01 {}"},
		{"scan start form feed", "\f{}"},
		{"scan start invalid byte", "\xff"},
		{"scan start truncated rune", "\xc3"},
		{"scan start bom", "\xef\xbb\xbf{\"name\":\"a\"}"},
		{"scan start nbsp", "\u00a0{}"},
		{"scan start non-ascii", "é"},
		{"scan literal tru", `tru`},
		{"scan literal trux", `trux`},
		{"scan literal nul", `nul`},
		{"scan literal nulx", `nulx`},
		{"scan literal fals", `fals`},
		{"scan literal falsx", `falsx`},
		{"scan literal t in list", `[tx]`},
		{"scan literal n in object", `{"a":nx}`},
		{"scan literal f at end", `{"a":f`},
		{"scan literal t non-ascii", `[té]`},
		{"scan literal true true", `truetrue`},
		{"scan number minus", `-`},
		{"scan number minus x", `-x`},
		{"scan number leading zero", `01`},
		{"scan number dot", `1.`},
		{"scan number dot x", `1.x`},
		{"scan number e", `1e`},
		{"scan number ex", `1ex`},
		{"scan number e plus", `1e+`},
		{"scan number e plus x", `1e+x`},
		{"scan number e in list", `[1.5e]`},
		{"scan number minus in object", `{"a":-}`},
		{"scan number dot in object", `{"a":1.}`},
		{"scan number minus in list", `[-]`},
		{"scan number leading zero in list", `[01]`},
		{"scan number hex in object", `{"a":0x}`},
		{"scan number plus", `[+1]`},
		{"scan number dot first", `[.5]`},
		{"scan string unterminated", `"`},
		{"scan string unterminated text", `"abc`},
		{"scan string backslash at end", `"\`},
		{"scan string escape x", `"\x"`},
		{"scan string escape short u", `"\u12"`},
		{"scan string escape bad hex", `"\u12x4"`},
		{"scan string escape u at end", `"\u`},
		{"scan string escape u truncated", `"\u12`},
		{"scan string escape with space", `{"a":"\u12 4"}`},
		{"scan string escape with control", "{\"a\":\"\\u1\x01\"}"},
		{"scan string escape with invalid byte", "{\"a\":\"\\u1\xff\"}"},
		{"scan string escape non-ascii", `{"a":"\é"}`},
		{"scan string control", "\"a\x01\""},
		{"scan string tab", "{\"a\":\"\t\"}"},
		{"scan string newline", "{\"a\":\"\n\"}"},
		{"scan string truncated rune at end", "{\"a\":\"\xe2\x82"},
		{"scan key unterminated", `{"a`},
		{"scan key number", `{1}`},
		{"scan key missing colon", `{"a" 1}`},
		{"scan key colon non-ascii", `{"a"é}`},
		{"scan key only", `{"a"}`},
		{"scan key bad escape", `{"\a":1}`},
		{"scan object open", `{`},
		{"scan object after key", `{"a"`},
		{"scan object after colon", `{"a":`},
		{"scan object after value", `{"a":1`},
		{"scan object after comma", `{"a":1,`},
		{"scan object missing comma", `{"a":1 "b"}`},
		{"scan object bracket", `{"a":1]`},
		{"scan object trailing comma", `{"a":1,}`},
		{"scan object comma first", `{,}`},
		{"scan object value missing", `{"a":}`},
		{"scan object value bracket", `{"a":]}`},
		{"scan array open", `[`},
		{"scan array after value", `[1`},
		{"scan array after comma", `[1,`},
		{"scan array missing comma", `[1 2]`},
		{"scan array trailing comma", `[1,]`},
		{"scan array comma first", `[,]`},
		{"scan array brace", `[1}`},
		{"scan array colon", `[1:]`},
		{"scan array nested open", `[{"a":[1,{"b":`},
		{"scan depth 10000 arrays", strings.Repeat("[", 10000) + strings.Repeat("]", 10000)},
		{"scan depth 10001 arrays", strings.Repeat("[", 10001) + strings.Repeat("]", 10001)},
		{"scan depth 10000 under object", `{"a":` + strings.Repeat("[", 9999) + strings.Repeat("]", 9999) + `}`},
		{"scan depth 10001 under object", `{"a":` + strings.Repeat("[", 10000) + strings.Repeat("]", 10000) + `}`},
		{"scan depth 10001 objects", strings.Repeat(`{"a":`, 10001) + "1" + strings.Repeat("}", 10001)},
		{"scan depth 500 objects", strings.Repeat(`{"p":`, 500) + "1" + strings.Repeat("}", 500)},
		// After the top-level value.
		{"trailing x", `{"name":"a"}x`},
		{"trailing comma", `{"name":"a"},`},
		{"trailing close bracket", `{"name":"a"}]`},
		{"trailing close brace", `{"name":"a"}}`},
		{"trailing object", `{"name":"a"} {"name":"b"}`},
		{"trailing open brace", `{"name":"a"} {`},
		{"trailing nul", "{\"name\":\"a\"}\x00"},
		{"trailing invalid byte", "{\"name\":\"a\"}\xff"},
		{"trailing non-ascii", `{"name":"a"}é`},
		{"trailing number", `{"name":"a"} 1`},
		{"trailing after null", `null x`},
		{"trailing after empty list", `[] x`},
		{"trailing after string", `"abc" xyz`},
		{"trailing after number", `1 2`},
		{"trailing after list syntax error", `[{"name":"a"}] [,]`},
		{"trailing whitespace", "\t\n\r {\"name\":\"a\"} \t\n\r "},
		{"trailing newlines", "{\"name\":\"a\"}\n\n"},
		// Numbers.
		{"number max int64", `{"uid": "0x1", "p": 9223372036854775807}`},
		{"number past max int64", `{"uid": "0x1", "p": 9223372036854775808}`},
		{"number min int64", `{"uid": "0x1", "p": -9223372036854775808}`},
		{"number past min int64", `{"uid": "0x1", "p": -9223372036854775809}`},
		{"number past max uint64", `{"uid": "0x1", "p": 18446744073709551616}`},
		{"number negative zero", `{"uid": "0x1", "p": -0}`},
		{"number negative float zero", `{"uid": "0x1", "p": -0.0}`},
		{"number exponent forms", `{"uid": "0x1", "a": 1E2, "b": 1e+2, "c": 1e-2, "d": 1.0e+0}`},
		{"number uid past max int64", `{"uid": 9223372036854775808, "name": "x"}`},
		{"number namespace past max int64", `{"uid": "0x1", "namespace": 9223372036854775808, "name": "x"}`},
		{"number facet past max int64", `{"uid": "0x1", "p": "x", "p|f": 9223372036854775808}`},
		{"number facet float range", `{"uid": "0x1", "p": "x", "p|f": 1.5e400}`},
		// Strings.
		{"string paired surrogates", `{"name":"\ud800\udc00"}`},
		{"string upper case surrogates", `{"name":"\uD834\uDD1E"}`},
		{"string reversed surrogates", `{"name":"\udc00\ud800"}`},
		{"string surrogate before escape", `{"name":"\ud800\u0041"}`},
		{"string surrogate twice", `{"name":"\ud800\ud800\udc00"}`},
		{"string encoded surrogate", "{\"name\":\"\xed\xa0\x80\"}"},
		{"string truncated rune", "{\"name\":\"\xe2\x82\"}"},
		{"string invalid utf8 key", "{\"k\xff\":\"v\"}"},
		{"string nul escape", `{"name":"a\u0000b"}`},
		{"string escapes all", `{"name":"\"\\\/\b\f\n\r\t\u00e9"}`},
		{"duplicate nested", `{"name":"a","name":{"uid":"0x2"}}`},
		{"duplicate uid", `{"uid":"0x1","uid":"0x2","name":"a"}`},
		{"duplicate facet", `{"uid":"0x1","p":"x","p|f":1,"p|f":2}`},
		{"duplicate geo keys", `{"uid":"0x1","loc":{"type":"Point","type":"Point","coordinates":[1,2]}}`},
	}
}

func genChunkerJSON() (any, error) {
	const runs = 12
	var cases []jsonCase
	for _, in := range jsonInputs() {
		for _, op := range []int{chunker.SetNquads, chunker.DeleteNquads} {
			first := parseJSONOnce(in.input, op)
			stable := true
			errs := map[string]bool{}
			for i := 0; i < runs; i++ {
				r := first
				if i > 0 {
					r = parseJSONOnce(in.input, op)
				}
				if sortedKey(r) != sortedKey(first) {
					stable = false
				}
				if r.err != "" {
					errs[r.err] = true
				}
			}
			c := jsonCase{Name: in.name, Input: in.input, InputHex: hex.EncodeToString([]byte(in.input)), Op: op,
				Err: first.err, Errs: []string{},
				ErrJSON: first.errTyp, NQuads: []*nquadJSON{}, Hints: hintsToJSON(first.md), Stable: stable,
				Repeated: runs}
			for e := range errs {
				c.Errs = append(c.Errs, e)
			}
			sort.Strings(c.Errs)
			blanks := map[string]bool{}
			for _, nq := range first.nqs {
				j := nquadToJSON(nq, true)
				c.NQuads = append(c.NQuads, j)
				for _, s := range []string{j.Subject, j.ObjectId} {
					if strings.HasPrefix(s, "_:dg.") {
						blanks[s] = true
					}
				}
			}
			c.Blanks = len(blanks)
			cases = append(cases, c)
		}
	}
	return cases, nil
}
