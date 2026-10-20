// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

import (
	"math"
	"math/rand"
	"strconv"

	"github.com/golang/geo/s2"
)

type s2gLatLngCase struct {
	LatDeg     string    `json:"lat_deg"`
	LngDeg     string    `json:"lng_deg"`
	LatRad     string    `json:"lat_rad"`
	LngRad     string    `json:"lng_rad"`
	Point      [3]string `json:"point"`
	BackLat    string    `json:"back_lat"`
	BackLng    string    `json:"back_lng"`
	BackLatDeg string    `json:"back_lat_deg"`
	BackLngDeg string    `json:"back_lng_deg"`
	Cell       string    `json:"cell"`
	Face       int       `json:"face"`
	Level      int       `json:"level"`
	Tokens     []string  `json:"tokens"` // Parent(level).ToToken() for level 0..30
	Levels     []int     `json:"levels"` // Parent(level).Level()
}

func s2gLatLngCaseOf(lat, lng float64) s2gLatLngCase {
	ll := s2.LatLngFromDegrees(lat, lng)
	p := s2.PointFromLatLng(ll)
	back := s2.LatLngFromPoint(p)
	c := s2.CellIDFromLatLng(ll)
	lc := s2gLatLngCase{
		LatDeg: s2gF(lat), LngDeg: s2gF(lng),
		LatRad: s2gF(ll.Lat.Radians()), LngRad: s2gF(ll.Lng.Radians()),
		Point:   s2gP(p),
		BackLat: s2gF(back.Lat.Radians()), BackLng: s2gF(back.Lng.Radians()),
		BackLatDeg: s2gF(back.Lat.Degrees()), BackLngDeg: s2gF(back.Lng.Degrees()),
		Cell:  s2gC(c),
		Face:  c.Face(),
		Level: c.Level(),
	}
	for l := 0; l <= s2.MaxLevel; l++ {
		lc.Tokens = append(lc.Tokens, c.Parent(l).ToToken())
		lc.Levels = append(lc.Levels, c.Parent(l).Level())
	}
	return lc
}

func s2gGenLatLng(dir string) {
	r := rand.New(rand.NewSource(2))
	var cases []s2gLatLngCase
	special := [][2]float64{
		{0, 0}, {90, 0}, {-90, 0}, {90, 180}, {-90, -180}, {0, 180}, {0, -180}, {45, 45}, {-45, -45},
		{0, 45}, {0, -45}, {0, 135}, {0, -135}, {35.26438968275466, 45}, {-35.26438968275466, -135},
		{89.999999, 179.999999}, {-89.999999, -179.999999}, {1e-300, -1e-300}, {math.Copysign(0, -1), math.Copysign(0, -1)},
		{37.4249518, -122.082506}, {46.372067, -67.852721}, {12.5, -70}, {90, 90}, {-90, 45},
		{0, 90}, {0, -90}, {54.7356103172453, 0}, {-54.7356103172453, 180}, {0.000001, 0.000001},
	}
	for _, s := range special {
		cases = append(cases, s2gLatLngCaseOf(s[0], s[1]))
	}
	for i := 0; i < 1500; i++ {
		lat := r.Float64()*180 - 90
		lng := r.Float64()*360 - 180
		if i%3 == 0 {
			// Dgraph-like inputs: decimal strings with 6 or 7 fractional digits.
			lat, _ = strconv.ParseFloat(strconv.FormatFloat(lat, 'f', 6+i%2, 64), 64)
			lng, _ = strconv.ParseFloat(strconv.FormatFloat(lng, 'f', 6+i%2, 64), 64)
		}
		cases = append(cases, s2gLatLngCaseOf(lat, lng))
	}
	// Points near the cube-face boundaries and the antimeridian.
	for i := 0; i < 300; i++ {
		lng := []float64{45, -45, 135, -135, 180, -180}[i%6] + (r.Float64()-0.5)*1e-9
		lat := (r.Float64()*2 - 1) * 50
		cases = append(cases, s2gLatLngCaseOf(lat, lng))
	}
	for i := 0; i < 200; i++ {
		lat := []float64{90, -90}[i%2] - math.Copysign(r.Float64()*1e-6, []float64{1, -1}[i%2])
		lng := r.Float64()*360 - 180
		cases = append(cases, s2gLatLngCaseOf(lat, lng))
	}
	s2gWriteJSON(dir, "latlng.json", cases)
}

type s2gCellIDCase struct {
	ID           string      `json:"id"`
	Token        string      `json:"token"`
	Level        int         `json:"level"`
	Face         int         `json:"face"`
	IsLeaf       bool        `json:"is_leaf"`
	Valid        bool        `json:"valid"`
	Parents      []string    `json:"parents"`
	RangeMin     string      `json:"range_min"`
	RangeMax     string      `json:"range_max"`
	Children     []string    `json:"children"`
	ChildBegin   string      `json:"child_begin"`
	ChildEnd     string      `json:"child_end"`
	Next         string      `json:"next"`
	Prev         string      `json:"prev"`
	Point        [3]string   `json:"point"`
	LatLng       [2]string   `json:"latlng"`
	VertexNbrs   []string    `json:"vertex_neighbors"` // VertexNeighbors(level-1) when level > 0
	FromToken    string      `json:"from_token"`
	BoundUV      [4]string   `json:"bound_uv"`
	Center       [3]string   `json:"center"`
	Vertices     [][3]string `json:"vertices"`
	RectBound    [4]string   `json:"rect_bound"`
	CapBound     s2gCap      `json:"cap_bound"`
	Probes       [][3]string `json:"probes"`
	ProbeIn      []bool      `json:"probe_in"`
	ChildBeginAt []string    `json:"child_begin_at"` // ChildBeginAtLevel(l) for l in level..min(level+3,30)
	ChildEndAt   []string    `json:"child_end_at"`
}

type s2gCellPairCase struct {
	A          string `json:"a"`
	B          string `json:"b"`
	Contains   bool   `json:"contains"`
	Intersects bool   `json:"intersects"`
	CommonLvl  int    `json:"common_level"`
	CommonOK   bool   `json:"common_ok"`
}

type s2gTokenCase struct {
	Token string `json:"token"`
	ID    string `json:"id"`
}

type s2gCellGolden struct {
	Cells  []s2gCellIDCase   `json:"cells"`
	Pairs  []s2gCellPairCase `json:"pairs"`
	Tokens []s2gTokenCase    `json:"tokens"`
	Unions []s2gUnionCase    `json:"unions"`
}

type s2gUnionCase struct {
	Input       []string      `json:"input"`
	Normalized  []string      `json:"normalized"`
	MinLevel    int           `json:"min_level"`
	LevelMod    int           `json:"level_mod"`
	Denorm      []string      `json:"denormalized,omitempty"`
	DenormCount int           `json:"denormalized_count"`
	DenormHash  string        `json:"denormalized_hash"`
	Probes      []string      `json:"probes"`
	ContainsID  []bool        `json:"contains_id"`
	IntersectID []bool        `json:"intersects_id"`
	CapBound    s2gCap        `json:"cap_bound"`
	RectBound   [4]string     `json:"rect_bound"`
	FromRange   [][]string    `json:"from_range"` // CellUnionFromRange(n[i].RangeMin(), n[i].RangeMax().Next())
	Coverings   []s2gCovering `json:"coverings"`  // RegionCoverer.Covering of the normalized union as a Region
}

func s2gIds(cu []s2.CellID) []string {
	out := make([]string, 0, len(cu))
	for _, c := range cu {
		out = append(out, s2gC(c))
	}
	return out
}

func s2gGenCellID(dir string) {
	r := rand.New(rand.NewSource(3))
	var g s2gCellGolden
	var all []s2.CellID
	for f := 0; f < 6; f++ {
		all = append(all, s2.CellIDFromFace(f))
	}
	for i := 0; i < 400; i++ {
		all = append(all, s2gRandomCellID(r, -1))
	}
	for i := 0; i < 100; i++ {
		all = append(all, s2gRandomCellID(r, s2.MaxLevel))
	}
	for _, c := range all {
		cc := s2gCellIDCase{
			ID: s2gC(c), Token: c.ToToken(), Level: c.Level(), Face: c.Face(), IsLeaf: c.IsLeaf(), Valid: c.IsValid(),
			RangeMin: s2gC(c.RangeMin()), RangeMax: s2gC(c.RangeMax()),
			Next: s2gC(c.Next()), Prev: s2gC(c.Prev()),
			Point:     s2gP(c.Point()),
			FromToken: s2gC(s2.CellIDFromToken(c.ToToken())),
		}
		ll := c.LatLng()
		cc.LatLng = [2]string{s2gF(ll.Lat.Radians()), s2gF(ll.Lng.Radians())}
		for l := 0; l <= c.Level(); l++ {
			cc.Parents = append(cc.Parents, s2gC(c.Parent(l)))
		}
		if !c.IsLeaf() {
			ch := c.Children()
			cc.Children = s2gIds(ch[:])
			cc.ChildBegin = s2gC(c.ChildBegin())
			cc.ChildEnd = s2gC(c.ChildEnd())
			for l := c.Level(); l <= c.Level()+3 && l <= s2.MaxLevel; l++ {
				cc.ChildBeginAt = append(cc.ChildBeginAt, s2gC(c.ChildBeginAtLevel(l)))
				cc.ChildEndAt = append(cc.ChildEndAt, s2gC(c.ChildEndAtLevel(l)))
			}
		}
		if c.Level() > 0 {
			cc.VertexNbrs = s2gIds(c.VertexNeighbors(c.Level() - 1))
		}
		cell := s2.CellFromCellID(c)
		uv := cell.BoundUV()
		cc.BoundUV = [4]string{s2gF(uv.X.Lo), s2gF(uv.X.Hi), s2gF(uv.Y.Lo), s2gF(uv.Y.Hi)}
		cc.Center = s2gP(cell.Center())
		for k := 0; k < 4; k++ {
			cc.Vertices = append(cc.Vertices, s2gP(cell.Vertex(k)))
		}
		cc.RectBound = s2gR(cell.RectBound())
		cc.CapBound = s2gCapOf(cell.CapBound())
		var probes []s2.Point
		probes = append(probes, cell.Center())
		for k := 0; k < 4; k++ {
			probes = append(probes, cell.Vertex(k), s2gPerturb(r, cell.Vertex(k), 3))
		}
		for k := 0; k < 4; k++ {
			probes = append(probes, s2gRandomPoint(r))
		}
		for _, p := range probes {
			cc.Probes = append(cc.Probes, s2gP(p))
			cc.ProbeIn = append(cc.ProbeIn, cell.ContainsPoint(p))
		}
		g.Cells = append(g.Cells, cc)
	}
	for i := 0; i < 3000; i++ {
		a := all[r.Intn(len(all))]
		var b s2.CellID
		switch i % 3 {
		case 0:
			b = all[r.Intn(len(all))]
		case 1:
			b = a.Parent(r.Intn(a.Level() + 1))
		default:
			b = a
			if !a.IsLeaf() {
				b = a.ChildBeginAtLevel(a.Level() + 1 + r.Intn(s2.MaxLevel-a.Level()))
			}
		}
		lvl, ok := a.CommonAncestorLevel(b)
		g.Pairs = append(g.Pairs, s2gCellPairCase{s2gC(a), s2gC(b), a.Contains(b), a.Intersects(b), lvl, ok})
	}
	for _, t := range []string{"", "X", "0", "1", "10066f42858b642f", "3", "89c25", "89c259", "zz", "123456789abcdef01", "b112966aaaaaaaab",
		"0000000000000000", "1000000000000000", "ffffffffffffffff", "-1", " 1", "a"} {
		g.Tokens = append(g.Tokens, s2gTokenCase{t, s2gC(s2.CellIDFromToken(t))})
	}
	for i := 0; i < 300; i++ {
		n := 1 + r.Intn(40)
		base := s2gRandomCellID(r, r.Intn(12))
		var cu s2.CellUnion
		for k := 0; k < n; k++ {
			switch r.Intn(3) {
			case 0:
				cu = append(cu, s2gRandomCellID(r, -1))
			default:
				l := base.Level() + r.Intn(4)
				if l > s2.MaxLevel {
					l = s2.MaxLevel
				}
				c := base.ChildBeginAtLevel(l)
				for j := r.Intn(8); j > 0 && base.Contains(c.Next()); j-- {
					c = c.Next()
				}
				cu = append(cu, c)
			}
		}
		// Add all four children of some cell so that normalization merges them.
		if r.Intn(2) == 0 && !base.IsLeaf() {
			ch := base.Children()
			cu = append(cu, ch[:]...)
		}
		uc := s2gUnionCase{Input: s2gIds(cu)}
		norm := append(s2.CellUnion(nil), cu...)
		norm.Normalize()
		uc.Normalized = s2gIds(norm)
		uc.MinLevel = r.Intn(8)
		uc.LevelMod = 1 + r.Intn(3)
		den := append(s2.CellUnion(nil), norm...)
		den.Denormalize(uc.MinLevel, uc.LevelMod)
		uc.DenormCount = len(den)
		uc.DenormHash = s2gCoveringOf(s2gCoverConfig{}, den).Hash
		if len(den) <= 256 {
			uc.Denorm = s2gIds(den)
		}
		for k := 0; k < 20; k++ {
			var p s2.CellID
			if k%2 == 0 && len(norm) > 0 {
				c := norm[r.Intn(len(norm))]
				p = c.Parent(r.Intn(c.Level() + 1))
				if r.Intn(2) == 0 && !c.IsLeaf() {
					p = c.ChildBeginAtLevel(c.Level() + 1)
				}
			} else {
				p = s2gRandomCellID(r, -1)
			}
			uc.Probes = append(uc.Probes, s2gC(p))
			uc.ContainsID = append(uc.ContainsID, norm.ContainsCellID(p))
			uc.IntersectID = append(uc.IntersectID, norm.IntersectsCellID(p))
		}
		uc.CapBound = s2gCapOf(norm.CapBound())
		uc.RectBound = s2gR(norm.RectBound())
		for k, c := range norm {
			if k >= 3 {
				break
			}
			uc.FromRange = append(uc.FromRange, s2gIds(s2.CellUnionFromRange(c.RangeMin(), c.RangeMax().Next())))
		}
		for _, cfg := range []s2gCoverConfig{{0, 30, 1, 8}, {5, 16, 0, 18}, {2, 10, 2, 4}, {0, 30, 1, -20000}} {
			r := norm
			uc.Coverings = append(uc.Coverings, s2gCoveringOf(cfg, cfg.coverer().Covering(&r)))
		}
		g.Unions = append(g.Unions, uc)
	}
	s2gWriteJSON(dir, "cellid.json", g)
}
