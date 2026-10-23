// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// One S2 vector on which Go's fused multiply-adds change the result (ledger 11): the point of a lat/lng, whose Z, and
// the latitude read back from it, differ in their last bit. Like every S2 generator this one runs with fusing off
// (scripts/golden.sh) and computes the unfused values. The fused values were generated once on arm64 by the same
// computation without -d=fmahash=n, and are recorded here, so the test checks that the port computes the unfused
// values and not the fused ones.

import (
	"fmt"
	"math"

	"github.com/golang/geo/s2"
)

type s2gFmaCase struct {
	LatDeg       string    `json:"lat_deg"`
	LngDeg       string    `json:"lng_deg"`
	Point        [3]string `json:"point"`
	BackLat      string    `json:"back_lat"`
	FusedPoint   [3]string `json:"fused_point"`
	FusedBackLat string    `json:"fused_back_lat"`
}

func s2gFma() (any, error) {
	lat := math.Float64frombits(0xc0460c76534373f3)
	lng := math.Float64frombits(0xc00a20ddc6195480)
	p := s2.PointFromLatLng(s2.LatLngFromDegrees(lat, lng))
	c := s2gFmaCase{
		LatDeg:       s2gF(lat),
		LngDeg:       s2gF(lng),
		Point:        s2gP(p),
		BackLat:      s2gF(s2.LatLngFromPoint(p).Lat.Radians()),
		FusedPoint:   [3]string{"3fe6f198f6351ece", "bfa4f2d711086852", "bfe644a58ba6efaf"},
		FusedBackLat: "bfe8a0ecbfd5da2c",
	}
	if c.Point == c.FusedPoint || c.BackLat == c.FusedBackLat {
		return nil, fmt.Errorf("types_s2_fma: this build fuses multiply-adds; run scripts/golden.sh")
	}
	return []any{c}, nil
}

func init() {
	register("types_s2_fma", "ultragraphdb_core_types", s2gFma)
}
