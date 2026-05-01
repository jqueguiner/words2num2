# -*- coding: utf-8 -*-
"""Shared utilities."""
from __future__ import unicode_literals

import re

_DIGIT_RE = re.compile(r"-?\d+(?:\.\d+)?")


def extract_numeric(text):
    """Return the first numeric literal embedded in ``text`` or None."""
    m = _DIGIT_RE.search(text)
    if not m:
        return None
    raw = m.group()
    if "." in raw:
        return float(raw)
    return int(raw)


def tokens(text):
    """Cheap whitespace tokenizer kept consistent across language modules."""
    return [tok for tok in re.split(r"\s+", text.strip()) if tok]
