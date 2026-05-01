# Changelog

All notable changes to `words2num2` are documented here. This project
follows [Semantic Versioning](https://semver.org/) and uses
[Keep a Changelog](https://keepachangelog.com/) style.

## [0.2.1] — 2026-05-01

### Added
- `pluralize(long_form, value)` helper in `words2num2.converters.auto`.
- Plural rules applied automatically by `auto_parse_sentence(..., expand=True)`.
  - Irregular forms: `foot → feet`, `inch → inches`, `pound sterling →
    pounds sterling`, `degree celsius/fahrenheit`, multi-word
    currencies (`Swiss francs`, etc.).
  - Uncountable units stay invariant: `yen`, `yuan`, `won`, `kelvin`,
    `percent`.
  - Regular `-s`/`-es`/`-ies` for everything else.
- 21 new pluralization tests.

### Fixed
- Sentence-mode regex no longer eats trailing whitespace before non-unit
  words (`Pay $12.50 for 5kg.` now keeps the space before "for").

## [0.2.0] — 2026-05-01

### Added
- **Auto-parse mode**: `auto_parse(text, ...)` and
  `auto_parse_sentence(text, ...)` — extract numeric values plus their
  unit from free text.
- `Quantity` dataclass with `value`, `unit`, `unit_long`, `kind`,
  `confidence`, `raw`.
- `parse_number_string(s, thousands_sep=None, decimal_sep=None,
  lang=None)` with caller-overridable separators, per-locale CLDR-style
  defaults for 50+ locales, and an auto-detect heuristic.
- Currency support: prefix and suffix forms for `$ € £ ¥ ₹ ₽ ₩ ₺` plus
  ISO codes (`USD`, `EUR`, `GBP`, `JPY`, `CHF`, `CAD`, `AUD`, `CNY`,
  `INR`, `BRL`, `MXN`, `RUB`, `KRW`).
- Currency scale shortcuts: `$5k`, `$5m`, `$5b`, `$5bn`, `$2.5t`.
- Unit support:
  - Length — `mm`, `cm`, `dm`, `m`, `km`, `in`, `ft`, `yd`, `mi`, `nm`, `µm`.
  - Mass — `mg`, `g`, `kg`, `t`, `lb`/`lbs`, `oz`.
  - Temperature — `°`, `°C`, `°F`, `K`, `C`, `F`.
  - Time — `ms`, `s`/`sec`, `min`, `h`/`hr`/`hrs`, `d`.
  - Volume — `ml`, `cl`, `dl`, `l`/`L`, `gal`.
  - Percent — `%`.
- Word-form unit aliases (English): `forty-two kilograms`,
  `twenty-three percent`, etc.
- Disambiguation hints: `prefer={"m": "mile", "g": "giga"}` for
  ambiguous unit tokens.
- `expand=True` mode for `auto_parse_sentence` — renders the long unit
  form (`12.5 dollar` instead of `12.5 USD`).
- 55 new tests covering currency, units, separators, and sentence mode.
- New public exports: `auto_parse`, `auto_parse_sentence`, `Quantity`,
  `UNITS`, `CURRENCIES`, `parse_number_string`,
  `NUMBER_FORMAT_DEFAULTS`.

## [0.1.1] — 2026-05-01

### Added
- Mirror of `num2words2`'s CI/CD as four GitHub Actions workflows:
  - `ci.yml` — Python 3.8–3.13 matrix, pytest, coverage.
  - `lint.yml` — black, flake8, isort, mypy.
  - `release.yml` — auto-build, GitHub Release, PyPI Trusted Publishing
    on tag push.
  - `publish-pypi.yml` — manual `workflow_dispatch` fallback using
    `PYPI_API_TOKEN` / `TEST_PYPI_API_TOKEN` repo secrets.

### Fixed
- Dependency spec `num2words2 >= 0.1.0.dev0` so pre-release versions
  from PyPI satisfy the requirement.

## [0.1.0] — 2026-05-01

### Added
- Initial release. Mirrors `num2words2`'s package layout with 120
  dispatch entries (~100 distinct languages, 14 regional variants, 2
  aliases).
- Hand-written English grammar parser (`lang_EN.py`) — cardinals,
  ordinals, decimals, negatives, scale words to *centillion*, year
  mode, "and" connectors, hyphenation.
- Generic backend (`Words2Num_Base`) that derives a `{words → number}`
  lookup table by calling `num2words2` forward across the integer
  range `-1..10000`. Provides correctness for that window for every
  locale that `num2words2` supports.
- 117 stub language modules subclassing the generic backend.
- `words2num(text, lang, to)` and `words2num_sentence(text, ...)`
  walking running text and replacing word-numbers in place.
- CLI: `words2num2 "forty-two"`.
- `Words2NumError` exception type.
- 59 tests.

[0.2.1]: https://github.com/jqueguiner/words2num2/releases/tag/v0.2.1
[0.2.0]: https://github.com/jqueguiner/words2num2/releases/tag/v0.2.0
[0.1.1]: https://github.com/jqueguiner/words2num2/releases/tag/v0.1.1
[0.1.0]: https://github.com/jqueguiner/words2num2/releases/tag/v0.1.0
