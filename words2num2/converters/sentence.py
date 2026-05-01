# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""Sentence-level words-to-number converter.

Walks a sentence token by token. At each position, tries to match the
longest run of consecutive number-bearing tokens that the per-language
converter accepts; replaces that run with the numeric value.
"""
from __future__ import unicode_literals

import re


class SentenceConverter(object):
    # Tokens we are willing to *include* inside a number run even though
    # they are not numbers themselves. Currency words and units are not
    # included — they break the run.
    INCLUDABLE = {
        "en": {"and", "point", "dot", "minus", "negative", "a", "an"},
    }

    def convert(self, sentence, lang="en", to="cardinal", **kwargs):
        from .. import _resolve_lang, CONVERTER_CLASSES

        resolved = _resolve_lang(lang)
        converter = CONVERTER_CLASSES[resolved]
        includable = self.INCLUDABLE.get(resolved, set())

        # Tokenize keeping whitespace + punctuation positions.
        parts = re.findall(r"\S+|\s+", sentence)
        out = []
        i = 0
        n = len(parts)
        while i < n:
            piece = parts[i]
            if piece.isspace():
                out.append(piece)
                i += 1
                continue
            # A run must START with a real number word — connector words
            # like "and" / "point" can only appear *inside* a run.
            head = re.sub(r"[\.,;:!\?\"']+$", "", piece).lower()
            if not self._starts_run(head, converter):
                out.append(piece)
                i += 1
                continue
            # Try to grow a number run starting at i.
            best_value = None
            best_end = i
            j = i
            while j < n:
                tok = parts[j]
                if tok.isspace():
                    j += 1
                    continue
                clean = re.sub(r"[\.,;:!\?\"']+$", "", tok).lower()
                if not self._is_candidate(clean, converter, includable):
                    break
                # Try to parse [i..j].
                run = "".join(parts[i:j + 1]).strip()
                stripped = re.sub(r"[\.,;:!\?\"']+$", "", run)
                try:
                    value = getattr(
                        converter, "to_{}".format(to)
                    )(stripped, **kwargs)
                    best_value = value
                    best_end = j
                except Exception:
                    pass
                # A token ending in terminal punctuation closes the run.
                if re.search(r"[\.,;:!\?]$", tok):
                    break
                j += 1
            if best_value is not None:
                # Preserve trailing punctuation that we stripped during parse.
                run = "".join(parts[i:best_end + 1])
                m = re.search(r"[\.,;:!\?\"']+$", run)
                trailing = m.group() if m else ""
                out.append(str(best_value) + trailing)
                i = best_end + 1
            else:
                out.append(piece)
                i += 1
        return "".join(out)

    @staticmethod
    def _is_candidate(token, converter, includable):
        """Cheap pre-filter: is this token plausibly part of a number run?"""
        if not token:
            return False
        if token in includable:
            return True
        # Handle hyphenated forms like "twenty-one".
        for sub in token.replace("-", " ").split():
            if SentenceConverter._token_is_number_word(sub, converter):
                return True
        return False

    @staticmethod
    def _token_is_number_word(token, converter):
        # Try a single-token cardinal parse — cheap and language-agnostic.
        try:
            converter.to_cardinal(token)
            return True
        except Exception:
            return False

    @staticmethod
    def _starts_run(token, converter):
        """A run must start with a real number word — not "and"/"point"."""
        if not token:
            return False
        for sub in token.replace("-", " ").split():
            if SentenceConverter._token_is_number_word(sub, converter):
                return True
        return False
