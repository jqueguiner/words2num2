# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""English words-to-number parser.

The only hand-written locale. Its grammar — cardinals, ordinals, decimals,
negatives, scale words up to 'centillion', year forms — is a full port in
the Rust core (``w2n_lang_en.rs``); this class is a thin binder over the
``en_to_*`` entry points. A raised ``Words2NumError`` from the core is the
answer, so there is nothing for a Python parser to add.
"""
from __future__ import unicode_literals

from .base import Words2Num_Base, Words2NumError, _RUST


class Words2Num_EN(Words2Num_Base):
    LANG = "en"
    NEGATIVE_WORDS = ("minus", "negative")

    def to_cardinal(self, text):
        return self._en(text, "cardinal")

    def to_ordinal(self, text):
        return self._en(text, "ordinal")

    def to_year(self, text):
        # Years like "nineteen ninety nine" -> 1999.
        return self._en(text, "year")

    @staticmethod
    def _en(text, kind):
        if not isinstance(text, str):
            raise Words2NumError("expected str, got %r" % type(text).__name__)
        return getattr(_RUST, "en_to_%s" % kind)(text)
