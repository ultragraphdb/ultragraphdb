// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_types' geo values (types/geofilter.go, s2.go, s2index.go): index tokens,
// query tokens and filter matches, through the fork's exported functions. Parent tokens come out of a Go map, so
// every token list is sorted.

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strings"

	geom "github.com/twpayne/go-geom"
	"github.com/twpayne/go-geom/encoding/geojson"

	"github.com/dgraph-io/dgraph/v25/protos/pb"
	"github.com/dgraph-io/dgraph/v25/types"
)

func init() {
	register("types_geo_index", "ultragraphdb_core_types", genTypesGeoIndex)
	register("types_geo_query", "ultragraphdb_core_types", genTypesGeoQuery)
	register("types_geojson", "ultragraphdb_core_types", genTypesGeoJSON)
}

func goldenGeoms() []geom.T {
	return []geom.T{
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{-122.082506, 37.4249518}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{1, 2}),
		geom.NewPoint(geom.XYZ).MustSetCoords(geom.Coord{1, 2, 3}),
		geom.NewPoint(geom.XYM).MustSetCoords(geom.Coord{1, 2, 4}),
		geom.NewPoint(geom.XYZM).MustSetCoords(geom.Coord{1, 2, 3, 4}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{1e-7, 1e21}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{0.1, -0.000001}),
		geom.NewLineString(geom.XY).MustSetCoords([]geom.Coord{{0, 0}, {1, 1}, {2, 0.5}}),
		geom.NewLineString(geom.XY),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{
			{{-122, 37}, {-123, 37}, {-123, 38}, {-122, 38}, {-122, 37}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{
			{{0, 0}, {10, 0}, {10, 10}, {0, 10}, {0, 0}}, {{2, 2}, {2, 4}, {4, 4}, {2, 2}}}),
		geom.NewMultiPoint(geom.XY).MustSetCoords([]geom.Coord{{1, 2}, {3, 4}}),
		geom.NewMultiLineString(geom.XYZ).MustSetCoords([][]geom.Coord{{{1, 2, 3}, {4, 5, 6}, {7, 8, 9}, {1, 2, 3}}}),
		geom.NewMultiPolygon(geom.XY).MustSetCoords([][][]geom.Coord{
			{{{-112, 39}, {-113, 39}, {-113, 40}, {-112, 40}, {-112, 39}}},
			{{{71.09, 42.35}, {72.09, 42.35}, {72.09, 41.35}, {71.09, 41.35}, {71.09, 42.35}}}}),
		geom.NewGeometryCollection().MustPush(geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{1, 2}),
			geom.NewLineString(geom.XY).MustSetCoords([]geom.Coord{{0, 0}, {1, 1}})),
	}
}

func loadFeature(name string) (geom.T, error) {
	b, err := os.ReadFile(filepath.Join(forkDir(), "types", "testdata", name))
	if err != nil {
		return nil, err
	}
	var f geojson.Feature
	if err := f.UnmarshalJSON(b); err != nil {
		return nil, err
	}
	return f.Geometry, nil
}

func forkDir() string {
	if d := os.Getenv("DGRAPH_SRC"); d != "" {
		return d
	}
	root, _ := filepath.Abs("../..")
	return filepath.Join(root, "..", "..", "fork", "dgraph")
}

// geoString is Dgraph's string form of a geometry: GeoJSON with single quotes, via Convert from WKB.
func geoString(g geom.T) string {
	out, err := types.Convert(types.Val{Tid: types.GeoID, Value: wkbOf(g)}, types.StringID)
	if err != nil {
		panic(err)
	}
	return out.Value.(string)
}

func sorted(t []string) []string {
	s := append([]string(nil), t...)
	sort.Strings(s)
	return s
}

type geoIndexCase struct {
	Name   string   `json:"name"`
	Wkb    string   `json:"wkb"`
	Tokens []string `json:"tokens,omitempty"`
	Err    string   `json:"err,omitempty"`
}

func indexInputs() ([]string, []geom.T, error) {
	names := []string{}
	geoms := []geom.T{}
	for _, f := range []string{"zip.json", "aruba.json", "sudan.json", "us.json"} {
		g, err := loadFeature(f)
		if err != nil {
			return nil, nil, err
		}
		names = append(names, f)
		geoms = append(geoms, g)
		if mp, ok := g.(*geom.MultiPolygon); ok {
			for i := 0; i < mp.NumPolygons(); i += 7 {
				names = append(names, f+"#"+itoa(i))
				geoms = append(geoms, mp.Polygon(i))
			}
		}
	}
	for i, g := range goldenGeoms() {
		names = append(names, "golden#"+itoa(i))
		geoms = append(geoms, g)
	}
	extra := []geom.T{
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-122, 37}, {-122, 38}, {-123, 38}, {-123, 37}, {-122, 37}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-122, 37}, {-123, 37}, {-123, 38}, {-122, 38}, {-122, 38}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{1, 1}, {2, 2}, {1, 1}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-179, -10}, {179, -10}, {179, 10}, {-179, 10}, {-179, -10}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{0, 80}, {120, 80}, {-120, 80}, {0, 80}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-10, -10}, {10, -10}, {10, 10}, {-10, 10}, {-10, -10}}}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{180, 90}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{-180, -90}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{0, 0}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{-157.9197, 21.33}),
	}
	for i, g := range extra {
		names = append(names, "extra#"+itoa(i))
		geoms = append(geoms, g)
	}
	return names, geoms, nil
}

func itoa(i int) string {
	b, _ := json.Marshal(i)
	return string(b)
}

func genTypesGeoIndex() (any, error) {
	names, geoms, err := indexInputs()
	if err != nil {
		return nil, err
	}
	var cases []geoIndexCase
	for i, g := range geoms {
		c := geoIndexCase{Name: names[i], Wkb: h(wkbOf(g))}
		toks, err := types.IndexGeoTokens(g)
		if err != nil {
			c.Err = err.Error()
		} else {
			c.Tokens = sorted(toks)
		}
		cases = append(cases, c)
	}
	return cases, nil
}

type geoQueryCase struct {
	Func    string   `json:"func"`
	Args    []string `json:"args"`
	Tokens  []string `json:"tokens,omitempty"`
	Err     string   `json:"err,omitempty"`
	Matches []bool   `json:"matches,omitempty"`
}

type geoQueryFile struct {
	Candidates []string       `json:"candidates"` // WKB hex of the values each query is matched against
	Queries    []geoQueryCase `json:"queries"`
}

func genTypesGeoQuery() (any, error) {
	_, idx, err := indexInputs()
	if err != nil {
		return nil, err
	}
	candidates := []geom.T{
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{-122.082506, 37.4249518}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{-123.082506, 37.4249518}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{-122.080668, 37.426753}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-122, 37}, {-123, 37}, {-123, 38}, {-122, 38}, {-122, 37}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-122, 36}, {-123, 36}, {-123, 37}, {-122, 37}, {-122, 36}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-122.1, 37.1}, {-122.9, 37.1}, {-122.9, 37.9}, {-122.1, 37.9}, {-122.1, 37.1}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-121, 36}, {-124, 36}, {-124, 39}, {-121, 39}, {-121, 36}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-121.5, 36.5}, {-122.5, 36.5}, {-122.5, 37.5}, {-121.5, 37.5}, {-121.5, 36.5}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-120, 35}, {-121, 35}, {-121, 36}, {-120, 36}, {-120, 35}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-119.53, 37.86}, {-120.53, 37.86}, {-120.53, 36.86}, {-119.53, 36.86}, {-119.53, 37.86}}}),
		geom.NewMultiPolygon(geom.XY).MustSetCoords([][][]geom.Coord{{
			{{-119.53, 37.86}, {-120.53, 37.86}, {-120.53, 36.86}, {-119.53, 36.86}, {-119.53, 37.86}},
			{{-115.13, 36.18}, {-116.13, 36.18}, {-116.13, 35.18}, {-115.13, 35.18}, {-115.13, 36.18}},
			{{-71.09, 42.35}, {-72.09, 42.35}, {-72.09, 41.35}, {-71.09, 41.35}, {-71.09, 42.35}}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{
			{-122.5033039, 37.7334601}, {-122.503128, 37.7335189}, {-122.5031222, 37.7335205},
			{-122.5030813, 37.7335868}, {-122.5031511, 37.73359}, {-122.5031933, 37.7335916},
			{-122.5032228, 37.7336022}, {-122.5032697, 37.7335937}, {-122.5033194, 37.7335874},
			{-122.5033723, 37.7335518}, {-122.503369, 37.7335068}, {-122.5033462, 37.7334474},
			{-122.5033039, 37.7334601}}}),
		geom.NewLineString(geom.XY).MustSetCoords([]geom.Coord{{0, 0}, {1, 1}}),
	}
	candidates = append(candidates, idx[0], idx[len(idx)-1])
	if us, err := loadFeature("us.json"); err == nil {
		candidates = append(candidates, us)
	}
	if sudan, err := loadFeature("sudan.json"); err == nil {
		candidates = append(candidates, sudan)
	}
	var f geoQueryFile
	for _, c := range candidates {
		f.Candidates = append(f.Candidates, h(wkbOf(c)))
	}
	point := `[-122.082506, 37.4249518]`
	poly := `[[[-122, 37], [-123, 37], [-123, 38], [-122, 38], [-122, 37]]]`
	queries := []geoQueryCase{
		{Func: "near", Args: []string{point, "1000"}},
		{Func: "near", Args: []string{point, "0"}},
		{Func: "near", Args: []string{point, "-1"}},
		{Func: "near", Args: []string{point, "abc"}},
		{Func: "near", Args: []string{point}},
		{Func: "Near", Args: []string{`{'type':'Point','coordinates':[-157.9197, 21.33]}`, "50000"}},
		{Func: "near", Args: []string{poly, "100"}},
		{Func: "within", Args: []string{poly}},
		{Func: "within", Args: []string{point}},
		{Func: "contains", Args: []string{point}},
		{Func: "contains", Args: []string{`[-157.9197, 21.33]`}},
		{Func: "contains", Args: []string{`[-153.369141, 66.160507]`}},
		{Func: "contains", Args: []string{`[77.224249103, 28.6077159025]`}},
		{Func: "contains", Args: []string{`[0, 0]`}},
		{Func: "contains", Args: []string{`[[[-112, 39], [-113, 39], [-113, 40], [-112, 40], [-112, 39]]]`}},
		{Func: "contains", Args: []string{`[[[[-112, 39], [-113, 39], [-113, 40], [-112, 40], [-112, 39]]], [[[71.09, 42.35], [72.09, 42.35], [72.09, 41.35], [71.09, 41.35], [71.09, 42.35]]]]`}},
		{Func: "intersects", Args: []string{poly}},
		{Func: "intersects", Args: []string{`[[[-121.5, 36.5], [-122.5, 36.5], [-122.5, 37.5], [-121.5, 37.5], [-121.5, 36.5]]]`}},
		{Func: "intersects", Args: []string{`[[[-122.4989104270935, 37.736953437345356], [-122.50504732131958, 37.729096212099975], [-122.49515533447264, 37.732049133202324], [-122.4989104270935, 37.736953437345356]]]`}},
		{Func: "intersects", Args: []string{point}},
		{Func: "within", Args: []string{`[[[1, 2], [3, 4], [5, 6]]]`}},
		{Func: "within", Args: []string{`[[1.76, -2.234], [3.543, 4.534], [4.54, 6.213]]`}},
		{Func: "within", Args: []string{`[[1.253, 2.2343], [-4.563, 6.1231], []]`}},
		{Func: "within", Args: []string{`[1.0, 2.0] `}},
		{Func: "within", Args: []string{`{"type":"Polygon","coordinates":[[[-122, 37], [-123, 37], [-123, 38], [-122, 38], [-122, 37]]]}`}},
		{Func: "within", Args: []string{`{"type":"Polygon","coordinates":[[[-122, 37], [-123, 37], [-123, 38], [-122, 38], [-122, 38]]]}`}},
		{Func: "within", Args: []string{`[1]`}},
		{Func: "within", Args: []string{`abc`}},
		{Func: "within", Args: []string{poly, poly}},
		{Func: "intersects", Args: []string{`{"type":"LineString","coordinates":[[0,0],[1,1]]}`}},
		{Func: "foo", Args: []string{poly}},
	}
	for _, name := range []string{"us.json", "sudan.json", "zip.json"} {
		g, err := loadFeature(name)
		if err != nil {
			return nil, err
		}
		s := bytes.ReplaceAll([]byte(geoString(g)), []byte("'"), []byte(`"`))
		for _, fn := range []string{"within", "intersects", "contains"} {
			queries = append(queries, geoQueryCase{Func: fn, Args: []string{string(s)}})
		}
	}
	for _, q := range queries {
		toks, qd, err := types.GetGeoTokens(&pb.SrcFunction{Name: q.Func, Args: q.Args})
		if err != nil {
			q.Err = err.Error()
		} else {
			q.Tokens = sorted(toks)
			for _, c := range candidates {
				q.Matches = append(q.Matches, types.MatchGeo(&pb.TaskValue{Val: wkbOf(c), ValType: pb.Posting_GEO}, qd))
			}
		}
		f.Queries = append(f.Queries, q)
	}
	return []any{f}, nil // one case: the file holds a slice of cases
}

type geoJSONCase struct {
	In  string `json:"in"` // hex: some inputs are not UTF-8
	Err string `json:"err,omitempty"`
	Nil bool   `json:"nil,omitempty"`
	Out string `json:"out,omitempty"` // the geometry as geojson.Marshal writes it
}

// genTypesGeoJSON records go-geom's geojson.Unmarshal, Go's json.Unmarshal into geojson.Geometry, of valid
// geometries, every syntax error context of encoding/json and the type errors of the struct's fields.
func genTypesGeoJSON() (any, error) {
	inputs := []string{
		`{"type":"Point","coordinates":[1,2]}`,
		`{"type":"Point","coordinates":[1.5,-2.25,3]}`,
		`{"type":"Point","coordinates":[]}`,
		`{"type":"Point"}`,
		`{"type":"Point","coordinates":null}`,
		`{"type":"LineString","coordinates":[[0,0],[1,1]]}`,
		`{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}`,
		`{"type":"MultiPoint","coordinates":[[1,2],null,[3,4]]}`,
		`{"type":"MultiLineString","coordinates":[[[1,2],[3,4]]]}`,
		`{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,0]]]]}`,
		`{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[1,2]},null]}`,
		`{"TYPE":"Point","Coordinates":[3,4]}`,
		`{"type":"Point","type":"LineString","coordinates":[[0,0],[1,1]]}`,
		`{"type":"Point","coordinates":[1,2],"bbox":[0,0,1,1],"crs":{"type":"name"},"extra":1}`,
		`null`,
		` {"type":"Point","coordinates":[1,2]} `,
		// Type errors.
		`"Point"`,
		`[1,2]`,
		`{"type":1,"coordinates":[1,2]}`,
		`{"type":1,"crs":2}`,
		`{"crs":2,"type":1}`,
		`{"type":1,"type":"Point","coordinates":[1,2]}`,
		`{"type":"Point","crs":"x"}`,
		`{"type":"Point","crs":{"type":1}}`,
		`{"type":"Point","crs":{"properties":1}}`,
		`{"type":"Point","coordinates":"x"}`,
		`{"type":"Point","coordinates":[1,"x"]}`,
		`{"type":"Point","coordinates":[1,1e400]}`,
		`{"type":"LineString","coordinates":[[0,0],"x"]}`,
		`{"type":"Polygon","coordinates":[[[0,0],[1,true]]]}`,
		`{"type":"GeometryCollection","geometries":"x"}`,
		`{"type":"GeometryCollection","geometries":[1]}`,
		`{"type":"GeometryCollection","geometries":[{"type":1}]}`,
		`{"type":"Curve","coordinates":[1,2]}`,
		`{"type":"Point","coordinates":[1]}`,
		`{}`,
		// Syntax errors.
		``,
		` `,
		`x`,
		`{`,
		`{"type"`,
		`{"type":`,
		`{"type":"Point"`,
		`{"type":"Point",`,
		`{1}`,
		`{"type" 1}`,
		`{"type":"Point" "x"}`,
		`{"type":"Point"]`,
		`{"type":"Point",}`,
		`{"type":"Point","coordinates":[1 2]}`,
		`{"type":"Point","coordinates":[1,]}`,
		`{"type":"Point","coordinates":[1,2}`,
		`{"type":"Point","coordinates":[1.]}`,
		`{"type":"Point","coordinates":[1.x]}`,
		`{"type":"Point","coordinates":[1e]}`,
		`{"type":"Point","coordinates":[-]}`,
		`{"type":"Point","coordinates":[01]}`,
		`{"type":tru}`,
		`{"type":"\x"}`,
		`{"type":"\u12x4"}`,
		"{\"type\":\"a\x01\"}",
		"{\"type\":\"a\xffb\"}",
		"\xff",
		"\xef\xbb\xbf{}",
		`{"type":"Point","coordinates":[1,2]}x`,
		`{"type":"Point","coordinates":[1,2]} {}`,
		strings.Repeat("[", 10001) + strings.Repeat("]", 10001),
		`{"type":"Point","coordinates":` + strings.Repeat("[", 10000) + strings.Repeat("]", 10000) + `}`,
		`{"type":"Point","coordinates":` + strings.Repeat("[", 9999) + strings.Repeat("]", 9999) + `}`,
	}
	var cases []geoJSONCase
	for _, in := range inputs {
		c := geoJSONCase{In: h([]byte(in))}
		var g geom.T
		if err := geojson.Unmarshal([]byte(in), &g); err != nil {
			c.Err = err.Error()
		} else if g == nil {
			c.Nil = true
		} else {
			b, err := geojson.Marshal(g)
			if err != nil {
				return nil, err
			}
			c.Out = string(b)
		}
		cases = append(cases, c)
	}
	return cases, nil
}
