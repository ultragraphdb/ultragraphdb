#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The by-value golden vectors of PROTOCOL.md section 10.5, computed from the section's text alone, independently of
# the Rust code and of oxsdatatypes: each payload laid out big-endian from the value's components, which are written
# here by hand from the lexical form, timeOnTimeline by the section's formula with exact decimals, a double or float
# packed by Python's struct, and BLAKE2b-128 by hashlib. A form that is not the section's canonical form, or whose
# value its payload does not hold, is kept by its lexical form under tag 0x30 (up to 15 bytes) or 0x31.
#
#   python3 -I generate_by_value.py > by_value.tsv
#
# Each line: the datatype's IRI, the lexical form, the encoding in hex.

import hashlib
import struct
from decimal import Decimal, getcontext

getcontext().prec = 100
XSD = "http://www.w3.org/2001/XMLSchema#"
SCALE = Decimal(10) ** 18
# Datatype: (by-value tag, known datatype code).
TYPES = {
    "boolean": (0x10, 1), "integer": (0x11, 2), "decimal": (0x12, 3), "double": (0x13, 4), "float": (0x14, 5),
    "dateTime": (0x15, 6), "time": (0x16, 7), "date": (0x17, 8), "gYearMonth": (0x18, 9), "gYear": (0x19, 10),
    "gMonthDay": (0x1A, 11), "gDay": (0x1B, 12), "gMonth": (0x1C, 13), "duration": (0x1D, 14),
    "yearMonthDuration": (0x1E, 15), "dayTimeDuration": (0x1F, 16),
}


def be(value, width):
    return int(value).to_bytes(width, "big", signed=True)


def scaled(value):
    exact = Decimal(value) * SCALE
    assert exact == exact.to_integral_value(), value
    return be(exact, 16)


def leap(year):
    return year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)


def days_in_month(year, month):
    return [31, 29 if leap(year) else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]


def time_on_timeline(year=None, month=None, day=None, hour=None, minute=None, second=None, tz=None):
    """XML Schema 1.1 Part 2 section E.3.4, as PROTOCOL.md section 10.5 states it; div and mod toward negative
    infinity, as Python's // and %."""
    yr = 1971 if year is None else year - 1
    mo = 12 if month is None else month
    da = days_in_month(yr + 1, mo) - 1 if day is None else day - 1
    hr = 0 if hour is None else hour
    mi = (0 if minute is None else minute) - (0 if tz is None else tz)
    se = Decimal(0) if second is None else Decimal(second)
    return (31536000 * yr + 86400 * (yr // 400 - yr // 100 + yr // 4)
            + 86400 * sum(days_in_month(yr + 1, m) for m in range(1, mo)) + 86400 * da + 3600 * hr + 60 * mi + se)


def date_payload(**c):
    tz = c.get("tz")
    return scaled(time_on_timeline(**c)) + (b"\xff\xff" if tz is None else be(tz, 2))


def by_value(datatype, lexical, payload):
    return (datatype, lexical, bytes([TYPES[datatype][0]]) + payload)


def kept(datatype, lexical):
    code = TYPES[datatype][1]
    data = lexical.encode()
    if len(data) <= 15:
        return (datatype, lexical, bytes([0x30, code, len(data)]) + data)
    return (datatype, lexical, bytes([0x31, code]) + hashlib.blake2b(data, digest_size=16).digest())


VECTORS = [
    by_value("boolean", "true", b"\x01"), by_value("boolean", "false", b"\x00"),
    kept("boolean", "1"), kept("boolean", "0"), kept("boolean", "TRUE"),

    by_value("integer", "0", be(0, 8)), by_value("integer", "42", be(42, 8)), by_value("integer", "-7", be(-7, 8)),
    by_value("integer", "9223372036854775807", be(2**63 - 1, 8)),
    by_value("integer", "-9223372036854775808", be(-2**63, 8)),
    kept("integer", "042"), kept("integer", "+5"), kept("integer", "-0"),
    # Beyond i64: no by-value payload holds it (point 49).
    kept("integer", "9223372036854775808"), kept("integer", "-9223372036854775809"),

    by_value("decimal", "0", scaled("0")), by_value("decimal", "1.5", scaled("1.5")),
    by_value("decimal", "-0.25", scaled("-0.25")), by_value("decimal", "3", scaled("3")),
    by_value("decimal", "0.000000000000000001", scaled("0.000000000000000001")),
    by_value("decimal", "123456789.123456789012345678", scaled("123456789.123456789012345678")),
    kept("decimal", "1.50"), kept("decimal", "01.5"), kept("decimal", "3.0"), kept("decimal", "-0"),
    kept("decimal", "+1"),
    # Nineteen fraction digits: the payload holds eighteen (point 49).
    kept("decimal", "0.1234567890123456789"),

    by_value("double", "1", struct.pack(">d", 1.0)), by_value("double", "0.5", struct.pack(">d", 0.5)),
    by_value("double", "-0", struct.pack(">d", -0.0)), by_value("double", "1.5", struct.pack(">d", 1.5)),
    by_value("double", "0.1", struct.pack(">d", 0.1)),
    by_value("double", "100000000000000000000", struct.pack(">d", 1e20)),
    by_value("double", "INF", struct.pack(">d", float("inf"))),
    by_value("double", "-INF", struct.pack(">d", float("-inf"))),
    by_value("double", "NaN", struct.pack(">d", float("nan"))),
    kept("double", "1.5E0"), kept("double", "1.0"), kept("double", "1e0"), kept("double", "+1"),
    kept("double", "0.10"),

    by_value("float", "1", struct.pack(">f", 1.0)), by_value("float", "0.5", struct.pack(">f", 0.5)),
    by_value("float", "-0", struct.pack(">f", -0.0)), by_value("float", "0.1", struct.pack(">f", 0.1)),
    by_value("float", "INF", struct.pack(">f", float("inf"))),
    by_value("float", "NaN", struct.pack(">f", float("nan"))),
    kept("float", "1.0"), kept("float", "1.5E0"),

    by_value("dateTime", "0001-01-01T00:00:00Z",
             date_payload(year=1, month=1, day=1, hour=0, minute=0, second=0, tz=0)),
    by_value("dateTime", "2026-10-26T12:00:00Z",
             date_payload(year=2026, month=10, day=26, hour=12, minute=0, second=0, tz=0)),
    by_value("dateTime", "2026-10-26T12:00:00",
             date_payload(year=2026, month=10, day=26, hour=12, minute=0, second=0)),
    by_value("dateTime", "2026-10-26T12:00:00.5+05:30",
             date_payload(year=2026, month=10, day=26, hour=12, minute=0, second="0.5", tz=330)),
    by_value("dateTime", "2026-10-26T12:00:00+00:01",
             date_payload(year=2026, month=10, day=26, hour=12, minute=0, second=0, tz=1)),
    by_value("dateTime", "-0001-03-01T00:00:00Z",
             date_payload(year=-1, month=3, day=1, hour=0, minute=0, second=0, tz=0)),
    kept("dateTime", "2026-10-26T12:00:00+00:00"), kept("dateTime", "2026-10-26T12:00:00.0Z"),
    kept("dateTime", "2026-10-26T24:00:00Z"),
    # An offset of -00:01 would read back as no timezone (point 11).
    kept("dateTime", "2026-10-26T12:00:00-00:01"),

    by_value("time", "12:00:00Z", date_payload(hour=12, minute=0, second=0, tz=0)),
    by_value("time", "00:00:00", date_payload(hour=0, minute=0, second=0)),
    by_value("time", "12:34:56.789-08:00", date_payload(hour=12, minute=34, second="56.789", tz=-480)),
    kept("time", "12:00:00+00:00"), kept("time", "12:00:00.50"), kept("time", "24:00:00"),
    kept("time", "12:00:00-00:01"),

    by_value("date", "2026-10-26", date_payload(year=2026, month=10, day=26)),
    by_value("date", "2026-10-26Z", date_payload(year=2026, month=10, day=26, tz=0)),
    by_value("date", "2024-02-29-05:00", date_payload(year=2024, month=2, day=29, tz=-300)),
    kept("date", "2026-10-26+00:00"), kept("date", "2026-10-26-00:01"),

    by_value("gYearMonth", "2026-10", date_payload(year=2026, month=10)),
    by_value("gYearMonth", "2026-10Z", date_payload(year=2026, month=10, tz=0)),
    kept("gYearMonth", "2026-10+00:00"), kept("gYearMonth", "2026-10-00:01"),

    by_value("gYear", "2026", date_payload(year=2026)), by_value("gYear", "2026Z", date_payload(year=2026, tz=0)),
    by_value("gYear", "-0044", date_payload(year=-44)),
    kept("gYear", "2026+00:00"), kept("gYear", "2026-00:01"),

    by_value("gMonthDay", "--10-26", date_payload(month=10, day=26)),
    by_value("gMonthDay", "--02-29Z", date_payload(month=2, day=29, tz=0)),
    kept("gMonthDay", "--10-26+00:00"), kept("gMonthDay", "--10-26-00:01"),

    by_value("gDay", "---26", date_payload(day=26)), by_value("gDay", "---01Z", date_payload(day=1, tz=0)),
    kept("gDay", "---26+00:00"), kept("gDay", "---26-00:01"),

    by_value("gMonth", "--10", date_payload(month=10)), by_value("gMonth", "--12Z", date_payload(month=12, tz=0)),
    kept("gMonth", "--10+00:00"), kept("gMonth", "--10-00:01"),

    by_value("duration", "PT0S", be(0, 8) + scaled(0)),
    by_value("duration", "P1Y2M3DT4H5M6S", be(14, 8) + scaled(3 * 86400 + 4 * 3600 + 5 * 60 + 6)),
    by_value("duration", "-P1D", be(0, 8) + scaled(-86400)),
    by_value("duration", "PT0.5S", be(0, 8) + scaled("0.5")),
    by_value("duration", "P1Y2M", be(14, 8) + scaled(0)),
    by_value("duration", "P1DT2H", be(0, 8) + scaled(26 * 3600)),
    kept("duration", "P0D"), kept("duration", "P1Y2M3DT4H5M6.0S"), kept("duration", "P14M"),
    kept("duration", "PT26H"),

    by_value("yearMonthDuration", "P1Y2M", be(14, 8)), by_value("yearMonthDuration", "P0M", be(0, 8)),
    by_value("yearMonthDuration", "-P3M", be(-3, 8)),
    kept("yearMonthDuration", "P14M"), kept("yearMonthDuration", "P0Y"),

    by_value("dayTimeDuration", "P1DT2H", scaled(26 * 3600)), by_value("dayTimeDuration", "PT0S", scaled(0)),
    by_value("dayTimeDuration", "PT0.5S", scaled("0.5")), by_value("dayTimeDuration", "-PT1M", scaled(-60)),
    kept("dayTimeDuration", "PT26H"), kept("dayTimeDuration", "P0D"), kept("dayTimeDuration", "PT60S"),
]

for datatype, lexical, encoding in VECTORS:
    print(f"{XSD}{datatype}\t{lexical}\t{encoding.hex()}")
