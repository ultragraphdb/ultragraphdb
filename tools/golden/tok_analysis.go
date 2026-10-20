// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_tok's text analysis, the bleve v2.5.7 pieces Dgraph's tok package configures
// (tok/bleve.go, stemmers.go, stopwords.go):
//
//   - tok_segment: blevesearch/segment's word segments and their types over Unicode 8.0.0's WordBreakTest inputs
//     (the segment module's tables_test.go, read as Go source) and the text corpus (tok_corpus.go);
//   - tok_analysis: the token stream of Dgraph's term and fulltext analyzer (unicode tokenizer, to_lower,
//     NFKC), built as tok/bleve.go builds it, with each token's offsets and type;
//   - tok_cjk_bigram: that stream through bleve's cjk_bigram filter, as Dgraph's zh, ja and ko stemmer;
//   - tok_stemmers: each stemmer filter Dgraph's langStemmers names, over words built from the language's
//     sentences, its stop words, the string literals of bleve's tests and stemmer source, and the suffixes of the
//     Snowball stemmer for the language (snowballstem v0.9.0's among tables, read as Go source).

import (
	"encoding/hex"
	"go/ast"
	"go/parser"
	"go/token"
	"math/rand"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"unicode/utf8"

	"github.com/blevesearch/bleve/v2/analysis"
	"github.com/blevesearch/bleve/v2/analysis/analyzer/custom"
	"github.com/blevesearch/bleve/v2/analysis/token/lowercase"
	"github.com/blevesearch/bleve/v2/analysis/token/unicodenorm"
	"github.com/blevesearch/bleve/v2/analysis/tokenizer/unicode"
	"github.com/blevesearch/bleve/v2/registry"
	"github.com/blevesearch/segment"

	_ "github.com/dgraph-io/dgraph/v25/tok" // registers every filter Dgraph uses
)

func init() {
	register("tok_segment", "ultragraphdb_core_tok", genTokSegment)
	register("tok_analysis", "ultragraphdb_core_tok", genTokAnalysis)
	register("tok_cjk_bigram", "ultragraphdb_core_tok", genTokCjkBigram)
	register("tok_stemmers", "ultragraphdb_core_tok", genTokStemmers)
}

func tokModuleDir(path string) string {
	out, err := exec.Command("go", "list", "-m", "-f", "{{.Dir}}", path).Output()
	check(err)
	return strings.TrimSpace(string(out))
}

// tokWordBreakTests are the inputs of segment's unicodeWordTests (Unicode 8.0.0's WordBreakTest.txt).
func tokWordBreakTests() []string {
	path := filepath.Join(tokModuleDir("github.com/blevesearch/segment"), "tables_test.go")
	f, err := parser.ParseFile(token.NewFileSet(), path, nil, 0)
	check(err)
	var out []string
	for _, d := range f.Decls {
		g, ok := d.(*ast.GenDecl)
		if !ok {
			continue
		}
		for _, sp := range g.Specs {
			vs, ok := sp.(*ast.ValueSpec)
			if !ok || vs.Names[0].Name != "unicodeWordTests" {
				continue
			}
			for _, el := range vs.Values[0].(*ast.CompositeLit).Elts {
				for _, kv := range el.(*ast.CompositeLit).Elts {
					kv := kv.(*ast.KeyValueExpr)
					if kv.Key.(*ast.Ident).Name != "input" {
						continue
					}
					var b []byte
					for _, e := range kv.Value.(*ast.CompositeLit).Elts {
						n, err := strconv.ParseUint(e.(*ast.BasicLit).Value, 0, 8)
						check(err)
						b = append(b, byte(n))
					}
					out = append(out, string(b))
				}
			}
		}
	}
	return out
}

type tokSegmentCase struct {
	Input    string   `json:"input"`    // hex
	Segments []string `json:"segments"` // hex
	Types    []int    `json:"types"`
	Error    string   `json:"error"`
}

func genTokSegment() (any, error) {
	inputs := append(tokWordBreakTests(), tokCorpus()...)
	var out []tokSegmentCase
	for _, in := range inputs {
		c := tokSegmentCase{Input: hex.EncodeToString([]byte(in)), Segments: []string{}, Types: []int{}}
		s := segment.NewWordSegmenterDirect([]byte(in))
		for s.Segment() {
			c.Segments = append(c.Segments, hex.EncodeToString(s.Bytes()))
			c.Types = append(c.Types, s.Type())
		}
		if err := s.Err(); err != nil {
			c.Error = err.Error()
		}
		out = append(out, c)
	}
	return out, nil
}

// tokAnalyzer is Dgraph's term and fulltext analyzer, defined as tok/bleve.go defines it.
func tokAnalyzer() analysis.Analyzer {
	cache := registry.NewCache()
	_, err := cache.DefineTokenFilter("unicodenorm_nfkc", map[string]interface{}{
		"type": unicodenorm.Name,
		"form": unicodenorm.NFKC,
	})
	check(err)
	a, err := cache.DefineAnalyzer("term", map[string]interface{}{
		"type":          custom.Name,
		"tokenizer":     unicode.Name,
		"token_filters": []string{lowercase.Name, "unicodenorm_nfkc"},
	})
	check(err)
	return a
}

type tokStreamToken struct {
	Term  string `json:"term"` // hex
	Start int    `json:"start"`
	End   int    `json:"end"`
	Type  int    `json:"type"`
}

type tokStreamCase struct {
	Input  string           `json:"input"` // hex
	Tokens []tokStreamToken `json:"tokens"`
}

func tokStream(ts analysis.TokenStream) []tokStreamToken {
	out := []tokStreamToken{}
	for _, t := range ts {
		out = append(out, tokStreamToken{Term: hex.EncodeToString(t.Term), Start: t.Start, End: t.End, Type: int(t.Type)})
	}
	return out
}

func genTokAnalysis() (any, error) {
	a := tokAnalyzer()
	var out []tokStreamCase
	for _, in := range append(tokWordBreakTests(), tokCorpus()...) {
		out = append(out, tokStreamCase{Input: hex.EncodeToString([]byte(in)), Tokens: tokStream(a.Analyze([]byte(in)))})
	}
	return out, nil
}

func genTokCjkBigram() (any, error) {
	a := tokAnalyzer()
	f, err := registry.NewCache().TokenFilterNamed("cjk_bigram")
	check(err)
	var out []tokStreamCase
	for _, in := range tokCorpus() {
		out = append(out, tokStreamCase{Input: hex.EncodeToString([]byte(in)), Tokens: tokStream(f.Filter(a.Analyze([]byte(in))))})
	}
	return out, nil
}

// tokStemmerSources names, per stemmer filter of Dgraph's langStemmers but cjk_bigram, its language and the
// snowballstem package whose suffixes build test words.
var tokStemmerSources = []struct{ filter, lang, snowball string }{
	{"stemmer_ar", "ar", "arabic"},
	{"stemmer_ckb", "ckb", ""},
	{"stemmer_da_snowball", "da", "danish"},
	{"stemmer_de_light", "de", "german"},
	{"stemmer_porter", "en", "english"},
	{"stemmer_es_light", "es", "spanish"},
	{"stemmer_fi_snowball", "fi", "finnish"},
	{"stemmer_fr_light", "fr", "french"},
	{"stemmer_hi", "hi", ""},
	{"stemmer_hu_snowball", "hu", "hungarian"},
	{"stemmer_it_light", "it", "italian"},
	{"stemmer_nl_snowball", "nl", "dutch"},
	{"stemmer_no_snowball", "no", "norwegian"},
	{"stemmer_pt_light", "pt", "portuguese"},
	{"stemmer_ro_snowball", "ro", "romanian"},
	{"stemmer_ru_snowball", "ru", "russian"},
	{"stemmer_sv_snowball", "sv", "swedish"},
	{"stemmer_tr_snowball", "tr", "turkish"},
}

// tokGoStrings returns the words of every string literal in the Go files of dir matching suffix.
func tokGoStrings(dir, suffix string) []string {
	files, err := filepath.Glob(filepath.Join(dir, "*"+suffix))
	check(err)
	var out []string
	for _, path := range files {
		f, err := parser.ParseFile(token.NewFileSet(), path, nil, 0)
		check(err)
		ast.Inspect(f, func(n ast.Node) bool {
			if lit, ok := n.(*ast.BasicLit); ok && lit.Kind == token.STRING {
				if s, err := strconv.Unquote(lit.Value); err == nil {
					out = append(out, strings.Fields(s)...)
				}
			}
			return true
		})
	}
	return out
}

type tokStemmerCase struct {
	Filter string     `json:"filter"`
	Words  [][]string `json:"words"` // [in, out]
}

func genTokStemmers() (any, error) {
	bleveDir := tokModuleDir("github.com/blevesearch/bleve/v2")
	snowDir := tokModuleDir("github.com/blevesearch/snowballstem")
	porterDir := tokModuleDir("github.com/blevesearch/go-porterstemmer")
	a := tokAnalyzer()
	cache := registry.NewCache()
	var out []tokStemmerCase
	for i, src := range tokStemmerSources {
		langDir := filepath.Join(bleveDir, "analysis", "lang", src.lang)
		var words []string
		for _, s := range tokLangSentences[src.lang] {
			for _, t := range a.Analyze([]byte(s)) {
				words = append(words, string(t.Term))
			}
		}
		stops, err := cache.TokenMapNamed("stop_" + src.lang)
		check(err)
		for w := range stops {
			words = append(words, w)
		}
		words = append(words, tokGoStrings(langDir, "_test.go")...)
		if src.lang == "en" {
			words = append(words, tokGoStrings(porterDir, "_test.go")...)
		}
		var affixes []string
		if src.snowball != "" {
			affixes = append(affixes, tokGoStrings(filepath.Join(snowDir, src.snowball), ".go")...)
		}
		if src.lang == "ar" || src.lang == "ckb" || src.lang == "hi" {
			affixes = append(affixes, tokGoStrings(langDir, ".go")...)
		}
		words = tokUnique(words)
		affixes = tokUnique(affixes)
		var bases []string
		for _, w := range words {
			if len([]rune(w)) >= 3 && len(bases) < 40 {
				bases = append(bases, w)
			}
		}
		all := append([]string{}, words...)
		for _, b := range bases {
			for _, s := range affixes {
				all = append(all, b+s, s+b)
			}
		}
		r := rand.New(rand.NewSource(int64(1000 + i)))
		for k := 0; k < 3000 && len(affixes) > 0 && len(bases) > 0; k++ {
			all = append(all, bases[r.Intn(len(bases))]+affixes[r.Intn(len(affixes))]+affixes[r.Intn(len(affixes))])
		}
		for _, w := range words {
			all = append(all, strings.ToUpper(w))
		}
		all = tokUnique(all)
		f, err := cache.TokenFilterNamed(src.filter)
		check(err)
		c := tokStemmerCase{Filter: src.filter}
		for _, w := range all {
			ts := f.Filter(analysis.TokenStream{&analysis.Token{Term: []byte(w)}})
			c.Words = append(c.Words, []string{w, string(ts[0].Term)})
		}
		out = append(out, c)
	}
	return out, nil
}

func tokUnique(in []string) []string {
	set := map[string]bool{}
	var out []string
	for _, s := range in {
		if s != "" && utf8.ValidString(s) && !set[s] {
			set[s] = true
			out = append(out, s)
		}
	}
	sort.Strings(out)
	return out
}
