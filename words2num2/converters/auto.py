# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""Auto-parse: extract numeric values + units from free text.

The extraction logic — currency prefix/suffix forms, scale shortcuts
(``$5m``), digit+unit and word+unit suffixes, per-locale separators,
``prefer`` disambiguation, pluralisation — is a full port in the Rust core
(``w2n_sentence.rs``). This module is a thin binder: it owns the public
result/registry *types* (:class:`Quantity`, :data:`UNITS`,
:data:`CURRENCIES`) that callers import, and :func:`auto_parse` /
:func:`auto_parse_sentence` delegate to ``_rust``. The core builds the
:class:`Quantity` defined here, so the dataclass stays the single source of
truth for the result shape.
"""
from __future__ import unicode_literals

from dataclasses import dataclass
from decimal import Decimal
from typing import Dict, Optional, Union

from ..base import _RUST


# ---------------------------------------------------------------------------
# Registries (exported; the core carries its own copy for parsing)
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


# ---------------------------------------------------------------------------
# Quantity result (built by the core; defined here as the public shape)
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
# Public API (thin binder over the core)
# ---------------------------------------------------------------------------


def auto_parse(
    text,
    lang="en",
    prefer=None,
    thousands_sep=None,
    decimal_sep=None,
):
    """Parse a single quantity expression, e.g. ``"$12,345.00"`` or ``"5cm"``.

    Returns a :class:`Quantity`; raises ``Words2NumError`` if unparseable.
    ``prefer`` disambiguates ambiguous unit tokens (``{"m": "mile"}``).
    """
    return _RUST.auto_parse(text, lang, prefer, thousands_sep, decimal_sep)


def auto_parse_sentence(
    text,
    lang="en",
    prefer=None,
    thousands_sep=None,
    decimal_sep=None,
    expand=False,
):
    """Walk free text and replace every quantity expression in place.

    Each match becomes ``"<value> <unit>"`` (e.g. ``"$12.50"`` -> ``"12.5
    USD"``); with ``expand=True`` units use their long form. Returns the
    rewritten string.
    """
    return _RUST.auto_parse_sentence(
        text, lang, prefer, thousands_sep, decimal_sep, expand)


def pluralize(long_form, value):
    """Return ``long_form`` pluralized for ``value`` (English rules: singular
    at +/-1, irregulars like foot->feet, uncountables like yen). Thin binder
    over the core so the standalone helper and ``auto_parse_sentence`` share
    one implementation."""
    return _RUST.pluralize(long_form, value)
