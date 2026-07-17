# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""Per-locale number-format defaults and a configurable string parser.

The separator table below is data (kept in Python and exported); the parser
itself — explicit / per-locale / auto-detect resolution — is a full port in
the Rust core, so :func:`parse_number_string` is a thin binder over
``_rust.parse_number_string``.
"""
from __future__ import unicode_literals

from .base import _RUST

# CLDR-inspired per-locale defaults. Keys are the same locale codes as
# the rest of words2num2; "_default" is the fallback. This table mirrors the
# one compiled into the core; it stays here because callers import it.
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


def parse_number_string(s, thousands_sep=None, decimal_sep=None, lang=None):
    """Parse a numeric string with configurable separators.

    Resolution order (all in the core):
      1. If ``thousands_sep`` and/or ``decimal_sep`` are given, use them.
      2. Otherwise if ``lang`` matches a known locale, use its defaults.
      3. Otherwise auto-detect from the string itself.
    """
    return _RUST.parse_number_string(s, thousands_sep, decimal_sep, lang)
