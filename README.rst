words2num2 — words to numbers
==============================

The inverse of `num2words2 <https://github.com/jqueguiner/num2words2>`_.

Convert spoken-form numbers ("forty-two", "trois cent quatre",
"二十三") back into numeric values across **100+ locales**.

* Hand-written English grammar parser: cardinals, ordinals, decimals,
  negatives, scale words up to *centillion*, year forms, "and"
  conjunctions, hyphenation, ASR-style outputs.
* Generic backend for every other locale supported by ``num2words2``,
  derived automatically by reverse-mapping the forward conversion.
* Sentence-level mode that walks running text and replaces every
  word-number with its numeric form, preserving punctuation and
  surrounding text.

.. note::
   This is the inverse direction of ``num2words2``. For numbers → words
   see https://github.com/jqueguiner/num2words2.

Installation
------------

.. code-block:: bash

    pip install words2num2

``num2words2`` is a runtime dependency for the generic multi-language
backend.

Usage
-----

.. code-block:: python

    >>> from words2num2 import words2num, words2num_sentence
    >>> words2num("forty-two")
    42
    >>> words2num("one thousand two hundred thirty-four")
    1234
    >>> words2num("minus seven")
    -7
    >>> words2num("three point one four")
    Decimal('3.14')
    >>> words2num("nineteen ninety nine", to="year")
    1999
    >>> words2num("twenty-first", to="ordinal")
    21
    >>> words2num("quarante-deux", lang="fr")
    42
    >>> words2num("zweiundvierzig", lang="de")
    42
    >>> words2num("сорок два", lang="ru")
    42

    >>> words2num_sentence("I bought twenty-three apples and fourteen pears.")
    'I bought 23 apples and 14 pears.'

Command-line
------------

.. code-block:: bash

    $ words2num2 "forty-two"
    42
    $ words2num2 "trois cent quatre" --lang=fr
    304
    $ words2num2 "twenty-third" --to=ordinal
    23

Supported locales
-----------------

words2num2 mirrors num2words2's locale list — 100+ entries including:

  af, am, ar, as, az, ba, be, bg, bn, bo, br, bs, ca, ce, cs, cy, da,
  de, el, en, en_IN, en_NG, eo, es, es_CO, es_CR, es_GT, es_NI, es_VE,
  et, eu, fa, fi, fo, fr, fr_BE, fr_CH, fr_DZ, gl, gu, ha, haw, he, hi,
  hr, ht, hu, hy, id, is, it, ja, jw, ka, kk, km, kn, ko, kz, la, lb,
  ln, lo, lt, lv, mg, mi, mk, ml, mn, mr, ms, mt, my, ne, nl, nn, no,
  oc, pa, pl, ps, pt, pt_BR, ro, ru, sa, sd, si, sk, sl, sn, so, sq,
  sr, su, sv, sw, ta, te, tet, tg, th, tk, tl, tr, tt, uk, ur, uz, vi,
  wo, yi, yo, zh, zh_CN, zh_HK, zh_TW

Aliases: ``jp`` → ``ja``, ``cn`` → ``zh_CN``.

API
---

``words2num(text, lang="en", to="cardinal")``
    Parse ``text`` and return ``int``, ``float``, or ``Decimal``.
    ``to`` is one of ``cardinal``, ``ordinal``, ``ordinal_num``,
    ``year``, ``currency``.

``words2num_sentence(text, lang="en", to="cardinal")``
    Walk a sentence, replacing every word-number with its numeric form.
    Returns a string. Aliases: ``convert_sentence``, ``sentence_to_words``.

How it works
------------

* English (``lang_EN``) ships a hand-written recursive-descent parser
  that handles the full grammar — including ordinal/cardinal mixing,
  decimals, year mode, "a hundred" filler, and negative tokens.
* Every other locale uses ``Words2Num_Base``, which lazily builds a
  ``{normalized_words: integer}`` table by calling ``num2words2`` for
  each integer in a configurable range (defaults to ``-1..10000``).
  This guarantees correctness for the lookup window for *every*
  locale supported upstream — at the cost of out-of-range values
  raising ``Words2NumError`` until a hand-written parser is added.

Hand-written parsers can be added incrementally per locale by
overriding ``to_cardinal`` / ``to_ordinal`` in the corresponding
``lang_XX.py`` module — same pattern as ``num2words2``.

Development
-----------

.. code-block:: bash

    make install-dev
    make test
    make lint
    make format

License
-------

LGPL-2.1, mirroring ``num2words2``.
