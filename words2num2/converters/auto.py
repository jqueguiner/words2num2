# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""Auto-parse: extract numeric values + units from free text.

Supports:
    - Currency-prefixed amounts: ``$12,345.00`` → 12345.0 USD
    - Currency-suffixed amounts: ``12 €``, ``45 USD``
    - Currency-with-scale shortcuts: ``$5m`` → 5,000,000
    - Number + unit suffix: ``5cm``, ``20°C``, ``3.5 km``, ``42%``
    - Word-form numbers + unit: ``forty-two kg``, ``twenty-three percent``
    - Pure word-form numbers: delegated to the per-language parser
    - Configurable thousands/decimal separators (per-locale or explicit)
    - Disambiguation hints via ``prefer={"m": "mile"}``

All entry points return a :class:`Quantity` describing the extracted
value, the canonical unit short/long form, the kind, and a confidence
score in [0, 1].
"""
from __future__ import unicode_literals

import re
from dataclasses import dataclass, field
from decimal import Decimal
from typing import Any, Dict, List, Optional, Union

from ..base import Words2NumError
from ..formats import parse_number_string


# ---------------------------------------------------------------------------
# Registries
# ---------------------------------------------------------------------------


@dataclass
class UnitInfo:
    short: str
    long: str
    kind: str
    confidence: float = 1.0


# Multiple aliases can map to the same unit.
UNITS: Dict[str, UnitInfo] = {
    # Length
    "mm": UnitInfo("mm", "millimeter", "length"),
    "cm": UnitInfo("cm", "centimeter", "length"),
    "dm": UnitInfo("dm", "decimeter", "length"),
    "m":  UnitInfo("m",  "meter",      "length", confidence=0.6),
    "km": UnitInfo("km", "kilometer",  "length"),
    "in": UnitInfo("in", "inch",       "length", confidence=0.5),
    "ft": UnitInfo("ft", "foot",       "length"),
    "yd": UnitInfo("yd", "yard",       "length"),
    "mi": UnitInfo("mi", "mile",       "length"),
    "nm": UnitInfo("nm", "nanometer",  "length"),
    "µm": UnitInfo("µm", "micrometer", "length"),
    "um": UnitInfo("µm", "micrometer", "length"),
    # Mass
    "mg": UnitInfo("mg", "milligram", "mass"),
    "g":  UnitInfo("g",  "gram",      "mass", confidence=0.6),
    "kg": UnitInfo("kg", "kilogram",  "mass"),
    "t":  UnitInfo("t",  "tonne",     "mass", confidence=0.5),
    "lb": UnitInfo("lb", "pound",     "mass"),
    "lbs": UnitInfo("lb", "pound",    "mass"),
    "oz": UnitInfo("oz", "ounce",     "mass"),
    # Temperature
    "°":  UnitInfo("°",  "degree",            "temperature", confidence=0.7),
    "°C": UnitInfo("°C", "degree celsius",    "temperature"),
    "°F": UnitInfo("°F", "degree fahrenheit", "temperature"),
    "K":  UnitInfo("K",  "kelvin",            "temperature", confidence=0.6),
    "C":  UnitInfo("°C", "degree celsius",    "temperature", confidence=0.5),
    "F":  UnitInfo("°F", "degree fahrenheit", "temperature", confidence=0.5),
    # Time
    "ms":  UnitInfo("ms",  "millisecond", "time"),
    "s":   UnitInfo("s",   "second",      "time", confidence=0.6),
    "sec": UnitInfo("s",   "second",      "time"),
    "min": UnitInfo("min", "minute",      "time"),
    "h":   UnitInfo("h",   "hour",        "time", confidence=0.7),
    "hr":  UnitInfo("h",   "hour",        "time"),
    "hrs": UnitInfo("h",   "hour",        "time"),
    "d":   UnitInfo("d",   "day",         "time", confidence=0.5),
    # Volume
    "ml":  UnitInfo("ml",  "milliliter", "volume"),
    "cl":  UnitInfo("cl",  "centiliter", "volume"),
    "dl":  UnitInfo("dl",  "deciliter",  "volume"),
    "l":   UnitInfo("L",   "liter",      "volume", confidence=0.7),
    "L":   UnitInfo("L",   "liter",      "volume"),
    "gal": UnitInfo("gal", "gallon",     "volume"),
    # Percent
    "%":   UnitInfo("%", "percent", "percent"),
}


@dataclass
class CurrencyInfo:
    code: str
    symbol: str
    long: str


CURRENCIES: Dict[str, CurrencyInfo] = {
    "$":   CurrencyInfo("USD", "$", "dollar"),
    "€":   CurrencyInfo("EUR", "€", "euro"),
    "£":   CurrencyInfo("GBP", "£", "pound"),
    "¥":   CurrencyInfo("JPY", "¥", "yen"),
    "₹":   CurrencyInfo("INR", "₹", "rupee"),
    "₽":   CurrencyInfo("RUB", "₽", "ruble"),
    "₩":   CurrencyInfo("KRW", "₩", "won"),
    "₺":   CurrencyInfo("TRY", "₺", "lira"),
    "USD": CurrencyInfo("USD", "$", "US dollar"),
    "EUR": CurrencyInfo("EUR", "€", "euro"),
    "GBP": CurrencyInfo("GBP", "£", "pound sterling"),
    "JPY": CurrencyInfo("JPY", "¥", "yen"),
    "CHF": CurrencyInfo("CHF", "CHF", "Swiss franc"),
    "CAD": CurrencyInfo("CAD", "$", "Canadian dollar"),
    "AUD": CurrencyInfo("AUD", "$", "Australian dollar"),
    "CNY": CurrencyInfo("CNY", "¥", "yuan"),
    "INR": CurrencyInfo("INR", "₹", "rupee"),
    "BRL": CurrencyInfo("BRL", "R$", "real"),
    "MXN": CurrencyInfo("MXN", "$", "Mexican peso"),
    "RUB": CurrencyInfo("RUB", "₽", "ruble"),
    "KRW": CurrencyInfo("KRW", "₩", "won"),
}


# Currency-prefix scale shortcuts: $5k, $5m, $5b, $5t.
SCALE_SUFFIXES = {
    "k": 10 ** 3,
    "K": 10 ** 3,
    "m": 10 ** 6,
    "M": 10 ** 6,
    "b": 10 ** 9,
    "B": 10 ** 9,
    "bn": 10 ** 9,
    "t": 10 ** 12,
    "T": 10 ** 12,
    "tn": 10 ** 12,
}


# ---------------------------------------------------------------------------
# Quantity result
# ---------------------------------------------------------------------------


@dataclass
class Quantity:
    value: Union[int, float, Decimal]
    unit: Optional[str] = None
    unit_long: Optional[str] = None
    kind: Optional[str] = None
    confidence: float = 1.0
    raw: str = ""

    def __repr__(self):
        if self.unit:
            return "Quantity(value={!r}, unit={!r}, kind={!r}, confidence={})".format(
                self.value, self.unit, self.kind, self.confidence
            )
        return "Quantity(value={!r})".format(self.value)


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def auto_parse(
    text,
    lang="en",
    prefer=None,
    thousands_sep=None,
    decimal_sep=None,
):
    """Parse a single quantity expression.

    Args:
        text: The expression to parse, e.g. ``"$12,345.00"`` or ``"5cm"``.
        lang: Language for word-form fallback and locale separators.
        prefer: Disambiguation hints, e.g. ``{"m": "mile", "g": "gram"}``.
        thousands_sep, decimal_sep: Override separators.

    Returns:
        A :class:`Quantity`.

    Raises:
        Words2NumError: If ``text`` cannot be parsed.
    """
    if not isinstance(text, str):
        raise Words2NumError("expected str, got %r" % type(text).__name__)
    raw = text
    text = text.strip()
    if not text:
        raise Words2NumError("empty input")
    prefer = prefer or {}

    # 1. Currency-prefix form: $12.50 / €12,50 / $5m
    cur = _try_currency_prefix(text)
    if cur is not None:
        sym, num_str, scale = cur
        try:
            value = parse_number_string(
                num_str, thousands_sep, decimal_sep, lang
            )
            if scale:
                value = _apply_scale(value, scale)
            return _quantity_currency(value, sym, raw)
        except Words2NumError:
            pass

    # 2. Currency-suffix form: 12.50 € / 45 USD
    cur = _try_currency_suffix(text)
    if cur is not None:
        sym, num_str = cur
        try:
            value = parse_number_string(
                num_str, thousands_sep, decimal_sep, lang
            )
            return _quantity_currency(value, sym, raw)
        except Words2NumError:
            pass

    # 3. Number + unit suffix (digit form): "5cm", "20°C", "42%", "3.5 km"
    res = _try_digit_unit(text, prefer, thousands_sep, decimal_sep, lang)
    if res is not None:
        res.raw = raw
        return res

    # 4. Word-form number + unit: "forty-two kg", "twenty-three percent"
    res = _try_word_unit(text, lang, prefer)
    if res is not None:
        res.raw = raw
        return res

    # 5. Pure number — try digit form, then word form.
    try:
        value = parse_number_string(text, thousands_sep, decimal_sep, lang)
        return Quantity(value=value, raw=raw)
    except Words2NumError:
        pass

    from .. import words2num
    try:
        value = words2num(text, lang=lang)
        return Quantity(value=value, raw=raw)
    except Exception as exc:
        raise Words2NumError("could not auto-parse %r: %s" % (raw, exc))


def auto_parse_sentence(
    text,
    lang="en",
    prefer=None,
    thousands_sep=None,
    decimal_sep=None,
    expand=False,
):
    """Walk free text and replace every quantity expression in place.

    By default each match is replaced with ``"<value> <unit>"`` (e.g.
    ``"$12.50"`` → ``"12.5 USD"``). If ``expand=True``, units use their
    long form (``"12.5 dollar"``).

    Returns the rewritten string.
    """
    prefer = prefer or {}

    # Tokenize the sentence into runs that might be quantities. A "run"
    # is a number-bearing token plus optional adjacent currency/unit
    # tokens. Numeric segments only allow internal separators that
    # are immediately followed by digits, so the regex never eats
    # trailing punctuation like ", " after "$12.50".
    NUM = r"[-+]?\d+(?:[.,'_\u00a0\u202f\u2009 ]\d+)*"
    pattern = re.compile(
        r"(?:[$\u20ac\u00a3\u00a5\u20b9\u20bd\u20a9\u20ba]\s*"
        + NUM
        + r"(?:\s*[kKmMbBtT][nN]?)?"
        + r"|"
        + NUM
        + r"(?:\s*(?:\u00b0[CF]?|%|[$\u20ac\u00a3\u00a5\u20b9\u20bd\u20a9\u20ba]|[A-Z]{3}|[a-zA-Z\u00b5]+))?"
        + r")"
    )

    def _replace(match):
        raw = match.group(0)
        try:
            q = auto_parse(
                raw,
                lang=lang,
                prefer=prefer,
                thousands_sep=thousands_sep,
                decimal_sep=decimal_sep,
            )
        except Words2NumError:
            return raw
        return _format_quantity(q, expand)

    return pattern.sub(_replace, text)


# ---------------------------------------------------------------------------
# Internals
# ---------------------------------------------------------------------------


def _try_currency_prefix(text):
    """Match $/€/£/¥/₹/USD/EUR/... at the start; return (symbol, num, scale)."""
    # Symbol prefix
    m = re.match(
        r"^\s*([$€£¥₹₽₩₺])\s*([-+]?[\d.,'    _]+)"
        r"\s*([kKmMbBtT][nN]?)?\s*$",
        text,
    )
    if m:
        sym = m.group(1)
        num = m.group(2).strip()
        scale = m.group(3)
        return sym, num, scale
    # 3-letter ISO code prefix (USD, EUR, ...)
    m = re.match(
        r"^\s*([A-Z]{3})\s*([-+]?[\d.,'    _]+)\s*$", text
    )
    if m and m.group(1) in CURRENCIES:
        return m.group(1), m.group(2).strip(), None
    return None


def _try_currency_suffix(text):
    """Match number followed by currency symbol/code suffix."""
    # Symbol suffix
    m = re.match(
        r"^\s*([-+]?[\d.,'    _]+)\s*([$€£¥₹₽₩₺])\s*$", text
    )
    if m:
        return m.group(2), m.group(1).strip()
    # 3-letter ISO code suffix
    m = re.match(
        r"^\s*([-+]?[\d.,'    _]+)\s*([A-Z]{3})\s*$", text
    )
    if m and m.group(2) in CURRENCIES:
        return m.group(2), m.group(1).strip()
    return None


def _quantity_currency(value, sym, raw):
    info = CURRENCIES[sym]
    return Quantity(
        value=value,
        unit=info.code,
        unit_long=info.long,
        kind="currency",
        confidence=1.0,
        raw=raw,
    )


def _apply_scale(value, scale_str):
    multiplier = SCALE_SUFFIXES[scale_str]
    if isinstance(value, int):
        # Convert to float if multiplier preserves precision; keep int
        # when both sides are integral.
        result = value * multiplier
        return result
    return float(value) * multiplier


def _try_digit_unit(text, prefer, thousands_sep, decimal_sep, lang):
    """Match a digit-form number followed by an optional unit suffix."""
    # Special-cases first (multi-char): °C, °F
    m = re.match(
        r"^\s*([-+]?[\d.,'    _]+)\s*(°[CF]?|°[CF]?|µm|"
        r"[a-zA-Z]+|%)\s*$",
        text,
    )
    if not m:
        # Number-only fallback handled by caller.
        return None
    num_str = m.group(1).strip()
    unit_str = m.group(2).strip()

    info, alt_long, conf = _resolve_unit(unit_str, prefer)
    if info is None:
        return None
    try:
        value = parse_number_string(num_str, thousands_sep, decimal_sep, lang)
    except Words2NumError:
        return None
    return Quantity(
        value=value,
        unit=info.short,
        unit_long=alt_long or info.long,
        kind=info.kind,
        confidence=conf,
    )


def _try_word_unit(text, lang, prefer):
    """Match a word-form number followed by a unit/currency word."""
    # Try splitting at the last space-separated chunk; treat the tail
    # as a candidate unit/currency word, head as the numeric phrase.
    parts = text.rsplit(None, 1)
    if len(parts) != 2:
        return None
    head, tail = parts[0], parts[1].lower().rstrip("s")

    # Word-form unit aliases (English-only for now).
    word_units = {
        "millimeter": ("mm", "length"),
        "millimetre": ("mm", "length"),
        "centimeter": ("cm", "length"),
        "centimetre": ("cm", "length"),
        "meter":      ("m",  "length"),
        "metre":      ("m",  "length"),
        "kilometer":  ("km", "length"),
        "kilometre":  ("km", "length"),
        "inch":       ("in", "length"),
        "foot":       ("ft", "length"),
        "feet":       ("ft", "length"),
        "yard":       ("yd", "length"),
        "mile":       ("mi", "length"),
        "milligram":  ("mg", "mass"),
        "gram":       ("g",  "mass"),
        "kilogram":   ("kg", "mass"),
        "tonne":      ("t",  "mass"),
        "ton":        ("t",  "mass"),
        "pound":      ("lb", "mass"),
        "ounce":      ("oz", "mass"),
        "second":     ("s",  "time"),
        "minute":     ("min", "time"),
        "hour":       ("h",  "time"),
        "day":        ("d",  "time"),
        "millisecond":("ms", "time"),
        "milliliter": ("ml", "volume"),
        "millilitre": ("ml", "volume"),
        "liter":      ("L",  "volume"),
        "litre":      ("L",  "volume"),
        "gallon":     ("gal", "volume"),
        "percent":    ("%",  "percent"),
        "degree":     ("°",  "temperature"),
        "celsius":    ("°C", "temperature"),
        "fahrenheit": ("°F", "temperature"),
        "kelvin":     ("K",  "temperature"),
        # currency words
        "dollar":     ("USD", "currency"),
        "euro":       ("EUR", "currency"),
        "pound sterling": ("GBP", "currency"),
        "yen":        ("JPY", "currency"),
        "rupee":      ("INR", "currency"),
        "yuan":       ("CNY", "currency"),
        "franc":      ("CHF", "currency"),
        "ruble":      ("RUB", "currency"),
        "won":        ("KRW", "currency"),
    }
    short = None
    kind = None
    long_name = None
    if tail in word_units:
        short, kind = word_units[tail]
    else:
        # Fall back to short-form unit tokens (kg, cm, °C, etc.) so
        # mixed forms like "forty-two kg" work.
        info, _alt, _conf = _resolve_unit(parts[1], prefer)
        if info is not None:
            short, kind = info.short, info.kind
            long_name = info.long
    if short is None:
        return None

    from .. import words2num
    try:
        value = words2num(head, lang=lang)
    except Exception:
        return None

    if kind == "currency":
        info = CURRENCIES[short]
        return Quantity(
            value=value,
            unit=info.code,
            unit_long=info.long,
            kind="currency",
            confidence=1.0,
        )
    long_name = next(
        (u.long for u in UNITS.values() if u.short == short), tail
    )
    return Quantity(
        value=value,
        unit=short,
        unit_long=long_name,
        kind=kind,
        confidence=1.0,
    )


def _resolve_unit(unit_str, prefer):
    """Resolve a unit token to (UnitInfo, override_long, confidence).

    Honors caller's ``prefer`` overrides for ambiguous tokens, e.g.
    ``prefer={"m": "mile"}``.
    """
    # Direct hit.
    info = UNITS.get(unit_str)
    if info is None:
        # Try lowercasing (but preserve °C/°F/K specifics first).
        info = UNITS.get(unit_str.lower())
    if info is None:
        return None, None, 0.0

    # Apply prefer-override.
    override_target = prefer.get(unit_str)
    if override_target:
        # Find a UnitInfo whose long form matches the override.
        for u in UNITS.values():
            if u.long == override_target or u.short == override_target:
                # Keep the *original* short token but flip to override
                # canonicalisation; clamp confidence to a value reflecting
                # caller intervention.
                return info, u.long, max(info.confidence, 0.9)
    return info, None, info.confidence


def _format_quantity(q, expand):
    if q.unit is None:
        return _format_number(q.value)
    label = q.unit_long if expand else q.unit
    # Tight glue for percent and bare degree; space-separated otherwise.
    if not expand and q.unit in ("%", "°"):
        return "{}{}".format(_format_number(q.value), label)
    return "{} {}".format(_format_number(q.value), label)


def _format_number(value):
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    return str(value)
