# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""Sentence-level words-to-number converter.

The token walk — longest-run matching, connector words, trailing
punctuation — is a full port in the Rust core (``w2n_sentence.rs``). This
class is a thin binder kept for callers that construct it directly;
:func:`words2num2.words2num_sentence` calls the core the same way.
"""
from __future__ import unicode_literals


class SentenceConverter(object):
    def convert(self, sentence, lang="en", to="cardinal", **kwargs):
        from ..base import _RUST

        return _RUST.words2num_sentence(sentence, lang, to, kwargs or None)
