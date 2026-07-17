# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
#
# This library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.
"""Base class for the words-to-number conversion.

``words2num2`` is a thin Python binder over the compiled ``_rust`` core.
Every generic locale's reverse-table lookup is served by ``_rust.lookup``,
which materialises the same ``{normalized_words: number}`` table Python used
to build (rendering num2words2 across ``LOOKUP_RANGE``) — natively, and once.
There is no pure-Python conversion fallback: the core is authoritative. The
only Python left here is argument shaping — the sign/digit/error tail the
reverse table never covered.
"""
from __future__ import unicode_literals

import re
import unicodedata


class Words2NumError(ValueError):
    """Raised when an input string cannot be parsed as a number."""


# The compiled core is mandatory: this package is a binder over it. A source
# checkout must build the extension (see bench/install_rust_w2n.sh) — an
# ImportError here is the honest signal that it has not been built.
from . import _rust as _RUST  # noqa: E402

_RUST_LANGS = frozenset(_RUST.supported_langs())


class Words2Num_Base(object):
    """Base class for inverse-of-num2words converters.

    Subclasses configure ``LANG`` (the num2words2 locale code). Conversion is
    delegated to the core: :meth:`to_cardinal` / :meth:`to_ordinal` call
    ``_rust.lookup``, which reproduces num2words2's rendering across
    ``LOOKUP_RANGE`` and looks the normalised input up in it. Hand-written
    grammar parsers (only ``en`` today) override the ``to_*`` methods.
    """

    LANG = None
    DECIMAL_SEPARATORS = ("point", "dot", "comma")
    NEGATIVE_WORDS = ("minus", "negative")
    AND_WORDS = ()
    LOOKUP_RANGE = (-1, 10001)  # kept for reference: the core's built-in range

    def __init__(self):
        self.errmsg_unparseable = "cannot parse %r as a number"
        self.errmsg_lang = "no num2words2 backend for locale %r"
        self.setup()

    # -- override hooks ---------------------------------------------------
    def setup(self):
        """Subclasses may set attributes prior to the first conversion."""

    # -- public API -------------------------------------------------------
    def str_to_number(self, text, to="cardinal"):
        """Convert ``text`` (words) to a number using the table-based path."""
        return getattr(self, "to_{}".format(to))(text)

    def to_cardinal(self, text):
        return self._convert(text, ordinal=False)

    def to_ordinal(self, text):
        return self._convert(text, ordinal=True)

    def _convert(self, text, ordinal):
        """Core reverse-table lookup, then the sign/digit/error tail.

        ``_rust.lookup`` returns ``None`` both for "not in the table" and for
        "language not supported"; either way the input is not a spelled-out
        number the table knows, so it falls to :meth:`_parse_literal` — which
        owns the digit-string and error branches exactly as the historic
        ``_lookup`` tail did.
        """
        hit = self._rust_lookup(text, ordinal)
        if hit is not None:
            return hit
        return self._parse_literal(text)

    def _rust_lookup(self, text, ordinal):
        """Try the Rust reverse-table. ``None`` means "fall through"."""
        if not self.LANG or self.LANG not in _RUST_LANGS:
            return None
        if not isinstance(text, str):
            return None
        try:
            return _RUST.lookup(self.LANG, text, ordinal, list(self.NEGATIVE_WORDS))
        except Exception:  # noqa: BLE001 - never let the fast path break a call
            return None

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

    def _parse_literal(self, text):
        """The non-word tail of the old ``_lookup``: a bare digit string, a
        leading sign word, or genuinely unparseable input. The reverse-table
        word lookup itself now lives entirely in the core."""
        normalized = self._normalize(text)
        if not normalized:
            raise Words2NumError(self.errmsg_unparseable % normalized)
        sign = 1
        for neg in self.NEGATIVE_WORDS:
            if normalized.startswith(neg + " "):
                sign = -1
                normalized = normalized[len(neg) + 1:]
                break
            if normalized == neg:
                raise Words2NumError(self.errmsg_unparseable % normalized)
        # If the input is already digits, return as int/float.
        try:
            if "." in normalized:
                return sign * float(normalized)
            return sign * int(normalized)
        except ValueError:
            pass
        raise Words2NumError(self.errmsg_unparseable % normalized)
