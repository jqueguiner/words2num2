words2num2 — words to numbers
=============================

.. image:: https://img.shields.io/pypi/v/words2num2.svg
   :target: https://pypi.python.org/pypi/words2num2
   :alt: PyPI version

.. image:: https://img.shields.io/pypi/pyversions/words2num2.svg
   :target: https://pypi.python.org/pypi/words2num2
   :alt: Python versions

.. image:: https://img.shields.io/pypi/dm/words2num2.svg
   :target: https://pypi.python.org/pypi/words2num2
   :alt: Downloads / month

.. image:: https://img.shields.io/pypi/status/words2num2.svg
   :target: https://pypi.python.org/pypi/words2num2
   :alt: Package status

.. image:: https://img.shields.io/aur/version/python-words2num2.svg
   :target: https://aur.archlinux.org/packages/python-words2num2
   :alt: AUR version

.. image:: https://github.com/jqueguiner/words2num2/workflows/CI/badge.svg
   :target: https://github.com/jqueguiner/words2num2/actions/workflows/ci.yml
   :alt: CI

.. image:: https://github.com/jqueguiner/words2num2/workflows/Lint/badge.svg
   :target: https://github.com/jqueguiner/words2num2/actions/workflows/lint.yml
   :alt: Lint

.. image:: https://github.com/jqueguiner/words2num2/workflows/CodeQL/badge.svg
   :target: https://github.com/jqueguiner/words2num2/actions/workflows/codeql-analysis.yml
   :alt: CodeQL

.. image:: https://github.com/jqueguiner/words2num2/workflows/E2E%20Tests/badge.svg
   :target: https://github.com/jqueguiner/words2num2/actions/workflows/e2e-tests.yml
   :alt: E2E Tests

.. image:: https://coveralls.io/repos/github/jqueguiner/words2num2/badge.svg?branch=main
   :target: https://coveralls.io/github/jqueguiner/words2num2?branch=main
   :alt: Coverage

.. image:: https://img.shields.io/github/v/release/jqueguiner/words2num2.svg
   :target: https://github.com/jqueguiner/words2num2/releases
   :alt: Latest release

.. image:: https://img.shields.io/github/last-commit/jqueguiner/words2num2.svg
   :target: https://github.com/jqueguiner/words2num2/commits/main
   :alt: Last commit

.. image:: https://img.shields.io/github/issues/jqueguiner/words2num2.svg
   :target: https://github.com/jqueguiner/words2num2/issues
   :alt: Issues

.. image:: https://img.shields.io/badge/license-LGPL--2.1-blue.svg
   :target: https://github.com/jqueguiner/words2num2/blob/main/COPYING
   :alt: License

The inverse of `num2words2 <https://github.com/jqueguiner/num2words2>`_.

``words2num2`` parses spoken-form numbers — ``"forty-two"``,
``"trois cent quatre"``, ``"二十三"`` — and returns numeric values.
It mirrors num2words2's locale list (**100+ languages, 120 dispatch
entries**) and adds a free-text *auto-parse* mode that handles
currencies, units, configurable thousands/decimal separators, and
ASR/LLM-style mixed text.

The project is hosted on GitHub_. Contributions are welcome.

.. _GitHub: https://github.com/jqueguiner/words2num2

Why this library
----------------

Existing inverse libraries are usually English-only, lack a sentence
mode, and don't compose with the locale defaults you already use for
the forward direction. ``words2num2``:

* Accepts the **same locale codes** as ``num2words2`` so the two
  libraries are drop-in inverses of each other.
* Has a hand-written grammar parser for **English** and a generic
  reverse-lookup backend that auto-derives ``{words → number}``
  tables from ``num2words2`` for **every other locale** out of the box.
* Walks free text via ``words2num_sentence`` / ``auto_parse_sentence``
  — useful when post-processing ASR transcripts, LLM output, or
  user-typed forms that mix words and digits.
* Handles currency symbols (``$ € £ ¥ ₹ ₽ ₩ ₺``), ISO codes
  (``USD/EUR/...``), scale shortcuts (``$5m`` → 5,000,000), units
  (length / mass / temperature / time / volume / percent), and
  CLDR-style number formats per locale.
* Pluralizes long-form units in expand mode (``5 dollars`` /
  ``1 dollar``, ``5 feet`` / ``1 foot``, ``5 yen`` / ``1 yen``).

Installation
------------

**pip**::

    pip install words2num2

**Arch Linux / Manjaro (AUR):**

.. code-block:: bash

    # With an AUR helper
    yay -S python-words2num2
    paru -S python-words2num2

    # Or manually
    git clone https://aur.archlinux.org/python-words2num2.git
    cd python-words2num2
    makepkg -si

**From source**::

    git clone https://github.com/jqueguiner/words2num2
    cd words2num2
    pip install -e .

``num2words2`` is a runtime dependency for the generic multi-language
backend and is installed automatically.

Quickstart
----------

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

Auto-parse mode
---------------

``auto_parse`` extracts a numeric value plus its unit from any free-text
expression. ``auto_parse_sentence`` walks running text and replaces every
quantity in place. It supports configurable thousands/decimal
separators per locale, currency symbols and ISO codes, scale shortcuts,
SI/imperial units, percent, and disambiguation hints.

.. code-block:: python

    >>> from words2num2 import auto_parse, auto_parse_sentence

    # Currencies
    >>> auto_parse("$12,345.00")
    Quantity(value=12345.0, unit='USD', kind='currency', confidence=1.0)
    >>> auto_parse("$5m").value
    5000000
    >>> auto_parse("12,50 €", lang="de").value
    12.5

    # Units
    >>> auto_parse("5cm")
    Quantity(value=5, unit='cm', kind='length', confidence=1.0)
    >>> auto_parse("20°C").kind
    'temperature'
    >>> auto_parse("forty-two kg").value
    42

    # Configurable separators
    >>> auto_parse("1.234,56", lang="de").value
    1234.56
    >>> auto_parse("1 234,56", lang="fr").value
    1234.56

    # Disambiguation for ambiguous unit tokens
    >>> auto_parse("5m", prefer={"m": "mile"}).unit_long
    'mile'

    # Sentence mode
    >>> auto_parse_sentence("Pay $12.50 for 5kg of apples at -5°C.")
    'Pay 12.5 USD for 5 kg of apples at -5 °C.'

    # Expand mode renders the long unit form, with English plural rules
    >>> auto_parse_sentence("Pay $12.50 for 5kg.", expand=True)
    'Pay 12.5 dollars for 5 kilograms.'
    >>> auto_parse_sentence("Pay $1.00 for 1kg.", expand=True)
    'Pay 1 dollar for 1 kilogram.'
    >>> auto_parse_sentence("5 ft and 1 ft.", expand=True)
    '5 feet and 1 foot.'

Configurable number formats
---------------------------

``parse_number_string`` is the primitive used by ``auto_parse`` for
digit-form numbers. You can call it directly with explicit separators
or rely on per-locale CLDR-style defaults:

.. code-block:: python

    >>> from words2num2 import parse_number_string

    >>> parse_number_string("12,345.67")                              # auto-detect
    12345.67
    >>> parse_number_string("12.345,67", lang="de")                   # German defaults
    12345.67
    >>> parse_number_string("1 234,56", lang="fr")                    # French defaults (NBSP)
    1234.56
    >>> parse_number_string("12'345.67", thousands_sep="'", decimal_sep=".")  # Swiss
    12345.67
    >>> parse_number_string("1_234.56", thousands_sep="_")            # programmer
    1234.56

The locale defaults table covers 50+ locales: English/CJK use comma
thousands and period decimal; French uses non-breaking-space + comma;
Swiss French uses apostrophe + period; German/Spanish/Italian/Portuguese/
Dutch/Romanian use period + comma; Russian/Scandinavian/Slavic use space
+ comma. See :file:`words2num2/formats.py` for the full table.

Auto-detection heuristic (when no override and no locale match):

1. If both ``.`` and ``,`` appear, the rightmost one is the decimal.
2. If one separator appears multiple times, it is thousands.
3. If one separator appears once with exactly 3 trailing digits, it is
   thousands; otherwise it is decimal.
4. Spaces, NBSP, apostrophe, and underscore are always thousands.

Command line
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

``words2num2`` mirrors num2words2's locale list — **120 dispatch
entries** including:

  af, am, ar, as, az, ba, be, bg, bn, bo, br, bs, ca, ce, cs, cy, da,
  de, el, en, en_IN, en_NG, eo, es, es_CO, es_CR, es_GT, es_NI, es_VE,
  et, eu, fa, fi, fo, fr, fr_BE, fr_CH, fr_DZ, gl, gu, ha, haw, he, hi,
  hr, ht, hu, hy, id, is, it, ja, jw, ka, kk, km, kn, ko, kz, la, lb,
  ln, lo, lt, lv, mg, mi, mk, ml, mn, mr, ms, mt, my, ne, nl, nn, no,
  oc, pa, pl, ps, pt, pt_BR, ro, ru, sa, sd, si, sk, sl, sn, so, sq,
  sr, su, sv, sw, ta, te, tet, tg, th, tk, tl, tr, tt, uk, ur, uz, vi,
  wo, yi, yo, zh, zh_CN, zh_HK, zh_TW

Aliases: ``jp`` → ``ja``, ``cn`` → ``zh_CN``.

Conversion types
~~~~~~~~~~~~~~~~

The ``to=`` parameter accepts ``cardinal``, ``ordinal``, ``ordinal_num``,
``year``, and ``currency`` — same set as ``num2words2``.

How it works
------------

* English (``lang_EN``) ships a hand-written recursive-descent parser
  that handles cardinals, ordinals, decimals, negatives, scale words to
  *centillion*, year mode, "and" connectors, and hyphenation.
* Every other locale uses ``Words2Num_Base``, which lazily builds a
  ``{normalized_words: integer}`` table by calling ``num2words2`` for
  each integer in a configurable range (defaults to ``-1..10000``).
  This guarantees correctness for the lookup window for every locale
  supported upstream — at the cost of out-of-range values raising
  ``Words2NumError`` until a hand-written parser is added.

Hand-written grammar parsers can be added incrementally per locale by
overriding ``to_cardinal`` / ``to_ordinal`` in the corresponding
``words2num2/lang_XX.py`` module — same pattern as num2words2.

Public API
----------

================================== =================================================
Function / class                   Purpose
================================== =================================================
``words2num(text, lang, to)``      Parse a single word-form number.
``words2num_sentence(text, ...)``  Replace every word-number in running text.
``auto_parse(text, ...)``          Parse a single quantity (number + unit).
``auto_parse_sentence(text, ...)`` Replace every quantity in running text.
``parse_number_string(text, ...)`` Digit-form parser with separators.
``Quantity``                       Dataclass returned by ``auto_parse``.
``UNITS`` / ``CURRENCIES``         Registries of recognized units and currencies.
``NUMBER_FORMAT_DEFAULTS``         Per-locale separator defaults.
``CONVERTER_CLASSES``              Per-locale converter registry.
``Words2NumError``                 Raised when input cannot be parsed.
================================== =================================================

See :file:`REFERENCE.md` for the full API reference with parameters,
return types, and examples.

Development
-----------

.. code-block:: bash

    git clone https://github.com/jqueguiner/words2num2
    cd words2num2
    make install-dev
    make test          # pytest
    make lint          # black + flake8 + isort
    make format        # apply black + isort

Releasing
~~~~~~~~~

Every push of a tag matching ``v*`` triggers GitHub Actions to:

1. Build sdist + wheel.
2. Run the test installation in a clean environment.
3. Generate release notes and create a GitHub Release.
4. Publish to PyPI via Trusted Publishing (no token in CI).

To cut a release::

    git tag vX.Y.Z
    git push origin vX.Y.Z

A manual fallback workflow (``Publish to PyPI (manual)``) is available
via ``gh workflow run`` and uses ``PYPI_API_TOKEN`` /
``TEST_PYPI_API_TOKEN`` repo secrets.

Changelog
---------

See :file:`CHANGELOG.md`.

License
-------

LGPL-2.1, mirroring ``num2words2``. See :file:`COPYING`.
