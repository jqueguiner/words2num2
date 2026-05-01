# -*- coding: utf-8 -*-
"""Compatibility shims (kept for parity with num2words2)."""
from __future__ import unicode_literals


def to_s(text):
    if isinstance(text, bytes):
        return text.decode("utf-8")
    return str(text)
