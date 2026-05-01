# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""Per-locale number-format defaults and a configurable string parser.

Supports caller-overridable thousands/decimal separators with auto-detection
heuristics when no override is given.
"""
from __future__ import unicode_literals

import re
import unicodedata
from decimal import Decimal

from .base import Words2NumError

# Common space-like characters that can appear as thousands separators.
NBSP = " "
NNBSP = " "  # Narrow NBSP — French standard
THIN_SPACE = " "
SPACE_LIKE = " \t" + NBSP + NNBSP + THIN_SPACE

# CLDR-inspired per-locale defaults. Keys are the same locale codes as
# the rest of words2num2; "_default" is the fallback.
NUMBER_FORMAT_DEFAULTS = {
    "_default": {"thousands": ",", "decimal": "."},
    # English + CJK group (comma thousands, dot decimal)
    "en":    {"thousands": ",", "decimal": "."},
    "en_GB": {"thousands": ",", "decimal": "."},
    "en_IN": {"thousands": ",", "decimal": "."},
    "en_NG": {"thousands": ",", "decimal": "."},
    "zh":    {"thousands": ",", "decimal": "."},
    "zh_CN": {"thousands": ",", "decimal": "."},
    "zh_HK": {"thousands": ",", "decimal": "."},
    "zh_TW": {"thousands": ",", "decimal": "."},
    "ja":    {"thousands": ",", "decimal": "."},
    "ko":    {"thousands": ",", "decimal": "."},
    "th":    {"thousands": ",", "decimal": "."},
    "vi":    {"thousands": ".", "decimal": ","},
    # French (space thousands, comma decimal)
    "fr":    {"thousands": " ", "decimal": ","},
    "fr_BE": {"thousands": " ", "decimal": ","},
    "fr_DZ": {"thousands": " ", "decimal": ","},
    # Swiss French uses apostrophe
    "fr_CH": {"thousands": "'", "decimal": "."},
    # Continental European (dot thousands, comma decimal)
    "de":    {"thousands": ".", "decimal": ","},
    "es":    {"thousands": ".", "decimal": ","},
    "es_CO": {"thousands": ".", "decimal": ","},
    "es_CR": {"thousands": ".", "decimal": ","},
    "es_GT": {"thousands": ".", "decimal": ","},
    "es_NI": {"thousands": ".", "decimal": ","},
    "es_VE": {"thousands": ".", "decimal": ","},
    "it":    {"thousands": ".", "decimal": ","},
    "pt":    {"thousands": ".", "decimal": ","},
    "pt_BR": {"thousands": ".", "decimal": ","},
    "nl":    {"thousands": ".", "decimal": ","},
    "ro":    {"thousands": ".", "decimal": ","},
    "hr":    {"thousands": ".", "decimal": ","},
    "sl":    {"thousands": ".", "decimal": ","},
    "sr":    {"thousands": ".", "decimal": ","},
    "tr":    {"thousands": ".", "decimal": ","},
    "el":    {"thousands": ".", "decimal": ","},
    # Slavic / Scandinavian / Baltic (space thousands, comma decimal)
    "ru":    {"thousands": " ", "decimal": ","},
    "uk":    {"thousands": " ", "decimal": ","},
    "be":    {"thousands": " ", "decimal": ","},
    "bg":    {"thousands": " ", "decimal": ","},
    "pl":    {"thousands": " ", "decimal": ","},
    "cs":    {"thousands": " ", "decimal": ","},
    "sk":    {"thousands": " ", "decimal": ","},
    "hu":    {"thousands": " ", "decimal": ","},
    "sv":    {"thousands": " ", "decimal": ","},
    "no":    {"thousands": " ", "decimal": ","},
    "nn":    {"thousands": " ", "decimal": ","},
    "da":    {"thousands": ".", "decimal": ","},
    "fi":    {"thousands": " ", "decimal": ","},
    "et":    {"thousands": " ", "decimal": ","},
    "lt":    {"thousands": " ", "decimal": ","},
    "lv":    {"thousands": " ", "decimal": ","},
    "is":    {"thousands": ".", "decimal": ","},
    "fo":    {"thousands": ".", "decimal": ","},
    # Arabic + Persian use Western digits but local separators in some
    # contexts; default to Western style.
    "ar":    {"thousands": ",", "decimal": "."},
    "fa":    {"thousands": ",", "decimal": "."},
    "he":    {"thousands": ",", "decimal": "."},
}


def get_format(lang):
    """Return the {thousands, decimal} dict for ``lang`` (with fallback)."""
    if lang in NUMBER_FORMAT_DEFAULTS:
        return NUMBER_FORMAT_DEFAULTS[lang]
    base = lang.split("_")[0] if lang else ""
    if base in NUMBER_FORMAT_DEFAULTS:
        return NUMBER_FORMAT_DEFAULTS[base]
    return NUMBER_FORMAT_DEFAULTS["_default"]


def parse_number_string(
    s,
    thousands_sep=None,
    decimal_sep=None,
    lang=None,
):
    """Parse a numeric string with configurable separators.

    Resolution order:
      1. If ``thousands_sep`` and/or ``decimal_sep`` are given, use them.
      2. Otherwise if ``lang`` matches a known locale, use its defaults.
      3. Otherwise auto-detect from the string itself.
    """
    if not isinstance(s, str):
        raise Words2NumError("expected str, got %r" % type(s).__name__)
    s = s.strip()
    if not s:
        raise Words2NumError("empty numeric string")

    sign = 1
    if s[0] in "+-":
        if s[0] == "-":
            sign = -1
        s = s[1:].lstrip()
    if not s:
        raise Words2NumError("empty numeric string after sign")

    if thousands_sep is not None or decimal_sep is not None:
        return sign * _parse_with_explicit(s, thousands_sep, decimal_sep)

    if lang is not None:
        fmt = get_format(lang)
        try:
            return sign * _parse_with_explicit(s, fmt["thousands"], fmt["decimal"])
        except Words2NumError:
            pass

    return sign * _auto_detect_parse(s)


def _parse_with_explicit(s, thousands_sep, decimal_sep):
    if thousands_sep:
        if thousands_sep in (" ", NBSP, NNBSP, THIN_SPACE):
            # treat any space-like as thousands when one is configured
            s = re.sub(r"[\s   ]", "", s)
        else:
            s = s.replace(thousands_sep, "")
    if decimal_sep:
        if decimal_sep != ".":
            if "." in s:
                # User explicitly gave a non-dot decimal but the string
                # also has dots — treat dots as a stray separator and drop.
                s = s.replace(".", "")
            s = s.replace(decimal_sep, ".")
    return _to_number(s)


def _auto_detect_parse(s):
    # Strip clearly-thousands-only separators first.
    s2 = re.sub(r"[\s   '_]", "", s)
    has_comma = "," in s2
    has_dot = "." in s2

    if has_comma and has_dot:
        # Both present — the rightmost is decimal.
        last_comma = s2.rfind(",")
        last_dot = s2.rfind(".")
        if last_comma > last_dot:
            s2 = s2.replace(".", "").replace(",", ".")
        else:
            s2 = s2.replace(",", "")
        return _to_number(s2)

    if has_comma:
        return _to_number(_resolve_single_sep(s2, ","))
    if has_dot:
        return _to_number(_resolve_single_sep(s2, "."))
    return _to_number(s2)


def _resolve_single_sep(s, sep):
    """Decide whether ``sep`` is decimal or thousands when only it appears."""
    parts = s.split(sep)
    if len(parts) > 2:
        # Multiple occurrences → thousands.
        return s.replace(sep, "")
    after = parts[1]
    # Exactly 3 trailing digits *and* a leading group of at most 3 →
    # ambiguous. Default to decimal (preserves precision) unless the
    # leading group has more than 3 digits, which would only make sense
    # as a thousands grouping for very large numbers.
    if len(after) == 3 and parts[0] and len(parts[0]) <= 3:
        if sep == ".":
            # "1.234" — most reading: 1.234 (decimal). But "12.345" is
            # almost certainly thousands. Heuristic: if leading group
            # is 1-3 digits AND there's no other context, prefer
            # decimal in en-default mode.
            return s  # treat as decimal
        else:
            # comma single occurrence with 3 trailing digits — same call.
            return s.replace(",", ".")
    if len(after) != 3:
        # Not a 3-digit group → must be decimal.
        return s if sep == "." else s.replace(",", ".")
    return s.replace(sep, "")


def _to_number(s):
    if not re.fullmatch(r"\d+(?:\.\d+)?", s):
        raise Words2NumError("not a parseable number: %r" % s)
    if "." in s:
        return float(s)
    return int(s)
