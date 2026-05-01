# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
#
# This library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.
"""Base classes for the words-to-number conversion."""
from __future__ import unicode_literals

import re
import unicodedata
from decimal import Decimal


class Words2NumError(ValueError):
    """Raised when an input string cannot be parsed as a number."""


class Words2Num_Base(object):
    """Base class for inverse-of-num2words converters.

    Subclasses configure ``LANG`` (the num2words2 locale code). The default
    implementation uses :mod:`num2words2` to derive a reverse lookup table
    on demand, which works for *any* locale supported by num2words2 — at
    the cost of ambiguity for languages that produce multi-token forms
    (compound French numbers, written-Chinese, etc.).

    Hand-written grammar parsers should override :meth:`to_cardinal` and
    related ``to_*`` methods to handle conjunctions, ordinals, decimals,
    and out-of-range values precisely.
    """

    LANG = None
    # Lookup tables seeded by ``setup``. Subclasses may extend.
    DECIMAL_SEPARATORS = ("point", "dot", "comma")
    NEGATIVE_WORDS = ("minus", "negative")
    AND_WORDS = ()
    LOOKUP_RANGE = (-1, 10001)  # build reverse table for this range

    def __init__(self):
        self.errmsg_unparseable = "cannot parse %r as a number"
        self.errmsg_lang = "no num2words2 backend for locale %r"
        self._cardinal_table = None
        self._ordinal_table = None
        self.setup()

    # -- override hooks ---------------------------------------------------
    def setup(self):
        """Subclasses may set attributes prior to the first conversion."""

    # -- public API -------------------------------------------------------
    def str_to_number(self, text, to="cardinal"):
        """Convert ``text`` (words) to a number using the table-based path."""
        return getattr(self, "to_{}".format(to))(text)

    def to_cardinal(self, text):
        return self._lookup(self._normalize(text), self._cardinal())

    def to_ordinal(self, text):
        return self._lookup(self._normalize(text), self._ordinal())

    def to_ordinal_num(self, text):
        # ordinal_num is e.g. "1st" — reuse direct numeric extraction.
        m = re.search(r"-?\d+", text)
        if not m:
            raise Words2NumError(self.errmsg_unparseable % text)
        return int(m.group())

    def to_year(self, text):
        return self.to_cardinal(text)

    def to_currency(self, text):
        # Currency parsing is locale-specific and best implemented per-language.
        # Default: strip currency words and try cardinal.
        return self.to_cardinal(text)

    # -- helpers ----------------------------------------------------------
    @staticmethod
    def _normalize(text):
        """Lowercase, strip diacritics, collapse whitespace, remove hyphens."""
        if not isinstance(text, str):
            raise Words2NumError("expected str, got %r" % type(text).__name__)
        # Normalize diacritics so e.g. "trente-deux" matches "trente deux".
        nfkd = unicodedata.normalize("NFKD", text)
        text = "".join(c for c in nfkd if not unicodedata.combining(c))
        text = text.lower().replace("_", " ")
        # Hyphen joins word-pairs ("forty-two") but signals sign before a
        # digit ("-17"). Replace only the word-joining hyphens.
        text = re.sub(r"(?<=[a-z])-(?=[a-z])", " ", text)
        text = re.sub(r"[,;:!\?\"']", " ", text)
        # Remove sentence-final '.' but keep decimal points.
        text = re.sub(r"\.(?!\d)", " ", text)
        text = re.sub(r"\s+", " ", text).strip()
        return text

    def _lookup(self, normalized, table):
        if not normalized:
            raise Words2NumError(self.errmsg_unparseable % normalized)
        # Handle leading negative.
        sign = 1
        for neg in self.NEGATIVE_WORDS:
            if normalized.startswith(neg + " "):
                sign = -1
                normalized = normalized[len(neg) + 1:]
                break
            if normalized == neg:
                raise Words2NumError(self.errmsg_unparseable % normalized)
        if normalized in table:
            return sign * table[normalized]
        # If the input is already digits, return as int/float.
        try:
            if "." in normalized:
                return sign * float(normalized)
            return sign * int(normalized)
        except ValueError:
            pass
        raise Words2NumError(self.errmsg_unparseable % normalized)

    def _cardinal(self):
        if self._cardinal_table is None:
            self._cardinal_table = self._build_table("cardinal")
        return self._cardinal_table

    def _ordinal(self):
        if self._ordinal_table is None:
            self._ordinal_table = self._build_table("ordinal")
        return self._ordinal_table

    def _build_table(self, kind):
        """Use num2words2 to materialize a {normalized_words: number} table.

        Built lazily on first access. Range is bounded by ``LOOKUP_RANGE``;
        outside this range the generic backend will raise unless the
        subclass overrides :meth:`to_cardinal`.
        """
        try:
            from num2words2 import num2words
        except ImportError as exc:
            raise Words2NumError(
                "num2words2 is required for the generic backend"
            ) from exc
        if not self.LANG:
            raise Words2NumError(self.errmsg_lang % self.LANG)
        lo, hi = self.LOOKUP_RANGE
        table = {}
        for n in range(lo, hi):
            try:
                words = num2words(n, lang=self.LANG, to=kind)
            except (NotImplementedError, Exception):  # pragma: no cover
                continue
            key = self._normalize(words)
            # First-write wins so canonical short form takes precedence.
            table.setdefault(key, n)
        return table
