# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
#
# This library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.
"""words2num2 — inverse of num2words2.

Convert spoken-form numbers ("forty-two", "trois cent quatre", ...) back
into numeric values across the same 100+ locales as num2words2.

>>> from words2num2 import words2num
>>> words2num("forty-two")
42
>>> words2num("one thousand two hundred")
1200
>>> words2num("three point one four")
Decimal('3.14')
>>> words2num("trente-deux", lang="fr")
32
"""
from __future__ import unicode_literals

from . import (
    lang_AF,
    lang_AM,
    lang_AR,
    lang_AS,
    lang_AZ,
    lang_BA,
    lang_BE,
    lang_BG,
    lang_BN,
    lang_BO,
    lang_BR,
    lang_BS,
    lang_CA,
    lang_CE,
    lang_CS,
    lang_CY,
    lang_DA,
    lang_DE,
    lang_EL,
    lang_EN,
    lang_EN_IN,
    lang_EN_NG,
    lang_EO,
    lang_ES,
    lang_ES_CO,
    lang_ES_CR,
    lang_ES_GT,
    lang_ES_NI,
    lang_ES_VE,
    lang_ET,
    lang_EU,
    lang_FA,
    lang_FI,
    lang_FO,
    lang_FR,
    lang_FR_BE,
    lang_FR_CH,
    lang_FR_DZ,
    lang_GL,
    lang_GU,
    lang_HA,
    lang_HAW,
    lang_HE,
    lang_HI,
    lang_HR,
    lang_HT,
    lang_HU,
    lang_HY,
    lang_ID,
    lang_IS,
    lang_IT,
    lang_JA,
    lang_JW,
    lang_KA,
    lang_KK,
    lang_KM,
    lang_KN,
    lang_KO,
    lang_KZ,
    lang_LA,
    lang_LB,
    lang_LN,
    lang_LO,
    lang_LT,
    lang_LV,
    lang_MG,
    lang_MI,
    lang_MK,
    lang_ML,
    lang_MN,
    lang_MR,
    lang_MS,
    lang_MT,
    lang_MY,
    lang_NE,
    lang_NL,
    lang_NN,
    lang_NO,
    lang_OC,
    lang_PA,
    lang_PL,
    lang_PS,
    lang_PT,
    lang_PT_BR,
    lang_RO,
    lang_RU,
    lang_SA,
    lang_SD,
    lang_SI,
    lang_SK,
    lang_SL,
    lang_SN,
    lang_SO,
    lang_SQ,
    lang_SR,
    lang_SU,
    lang_SV,
    lang_SW,
    lang_TA,
    lang_TE,
    lang_TET,
    lang_TG,
    lang_TH,
    lang_TK,
    lang_TL,
    lang_TR,
    lang_TT,
    lang_UK,
    lang_UR,
    lang_UZ,
    lang_VI,
    lang_WO,
    lang_YI,
    lang_YO,
    lang_ZH,
    lang_ZH_CN,
    lang_ZH_HK,
    lang_ZH_TW,
)
from .base import Words2NumError

try:
    from ._version import __version__, __version_tuple__
except ImportError:
    __version__ = "unknown"
    __version_tuple__ = (0, 0, 0, "unknown", 0)

__all__ = [
    "words2num",
    "words2num_sentence",
    "convert_sentence",
    "sentence_to_words",
    "Words2NumError",
]


CONVERTER_CLASSES = {
    "af": lang_AF.Words2Num_AF(),
    "am": lang_AM.Words2Num_AM(),
    "ar": lang_AR.Words2Num_AR(),
    "as": lang_AS.Words2Num_AS(),
    "az": lang_AZ.Words2Num_AZ(),
    "ba": lang_BA.Words2Num_BA(),
    "be": lang_BE.Words2Num_BE(),
    "bg": lang_BG.Words2Num_BG(),
    "bn": lang_BN.Words2Num_BN(),
    "bo": lang_BO.Words2Num_BO(),
    "br": lang_BR.Words2Num_BR(),
    "bs": lang_BS.Words2Num_BS(),
    "ca": lang_CA.Words2Num_CA(),
    "ce": lang_CE.Words2Num_CE(),
    "cs": lang_CS.Words2Num_CS(),
    "cy": lang_CY.Words2Num_CY(),
    "da": lang_DA.Words2Num_DA(),
    "de": lang_DE.Words2Num_DE(),
    "el": lang_EL.Words2Num_EL(),
    "en": lang_EN.Words2Num_EN(),
    "en_IN": lang_EN_IN.Words2Num_EN_IN(),
    "en_NG": lang_EN_NG.Words2Num_EN_NG(),
    "eo": lang_EO.Words2Num_EO(),
    "es": lang_ES.Words2Num_ES(),
    "es_CO": lang_ES_CO.Words2Num_ES_CO(),
    "es_CR": lang_ES_CR.Words2Num_ES_CR(),
    "es_GT": lang_ES_GT.Words2Num_ES_GT(),
    "es_NI": lang_ES_NI.Words2Num_ES_NI(),
    "es_VE": lang_ES_VE.Words2Num_ES_VE(),
    "et": lang_ET.Words2Num_ET(),
    "eu": lang_EU.Words2Num_EU(),
    "fa": lang_FA.Words2Num_FA(),
    "fi": lang_FI.Words2Num_FI(),
    "fo": lang_FO.Words2Num_FO(),
    "fr": lang_FR.Words2Num_FR(),
    "fr_BE": lang_FR_BE.Words2Num_FR_BE(),
    "fr_CH": lang_FR_CH.Words2Num_FR_CH(),
    "fr_DZ": lang_FR_DZ.Words2Num_FR_DZ(),
    "gl": lang_GL.Words2Num_GL(),
    "gu": lang_GU.Words2Num_GU(),
    "ha": lang_HA.Words2Num_HA(),
    "haw": lang_HAW.Words2Num_HAW(),
    "he": lang_HE.Words2Num_HE(),
    "hi": lang_HI.Words2Num_HI(),
    "hr": lang_HR.Words2Num_HR(),
    "ht": lang_HT.Words2Num_HT(),
    "hu": lang_HU.Words2Num_HU(),
    "hy": lang_HY.Words2Num_HY(),
    "id": lang_ID.Words2Num_ID(),
    "is": lang_IS.Words2Num_IS(),
    "it": lang_IT.Words2Num_IT(),
    "ja": lang_JA.Words2Num_JA(),
    "jw": lang_JW.Words2Num_JW(),
    "ka": lang_KA.Words2Num_KA(),
    "kk": lang_KK.Words2Num_KK(),
    "km": lang_KM.Words2Num_KM(),
    "kn": lang_KN.Words2Num_KN(),
    "ko": lang_KO.Words2Num_KO(),
    "kz": lang_KZ.Words2Num_KZ(),
    "la": lang_LA.Words2Num_LA(),
    "lb": lang_LB.Words2Num_LB(),
    "ln": lang_LN.Words2Num_LN(),
    "lo": lang_LO.Words2Num_LO(),
    "lt": lang_LT.Words2Num_LT(),
    "lv": lang_LV.Words2Num_LV(),
    "mg": lang_MG.Words2Num_MG(),
    "mi": lang_MI.Words2Num_MI(),
    "mk": lang_MK.Words2Num_MK(),
    "ml": lang_ML.Words2Num_ML(),
    "mn": lang_MN.Words2Num_MN(),
    "mr": lang_MR.Words2Num_MR(),
    "ms": lang_MS.Words2Num_MS(),
    "mt": lang_MT.Words2Num_MT(),
    "my": lang_MY.Words2Num_MY(),
    "ne": lang_NE.Words2Num_NE(),
    "nl": lang_NL.Words2Num_NL(),
    "nn": lang_NN.Words2Num_NN(),
    "no": lang_NO.Words2Num_NO(),
    "oc": lang_OC.Words2Num_OC(),
    "pa": lang_PA.Words2Num_PA(),
    "pl": lang_PL.Words2Num_PL(),
    "ps": lang_PS.Words2Num_PS(),
    "pt": lang_PT.Words2Num_PT(),
    "pt_BR": lang_PT_BR.Words2Num_PT_BR(),
    "ro": lang_RO.Words2Num_RO(),
    "ru": lang_RU.Words2Num_RU(),
    "sa": lang_SA.Words2Num_SA(),
    "sd": lang_SD.Words2Num_SD(),
    "si": lang_SI.Words2Num_SI(),
    "sk": lang_SK.Words2Num_SK(),
    "sl": lang_SL.Words2Num_SL(),
    "sn": lang_SN.Words2Num_SN(),
    "so": lang_SO.Words2Num_SO(),
    "sq": lang_SQ.Words2Num_SQ(),
    "sr": lang_SR.Words2Num_SR(),
    "su": lang_SU.Words2Num_SU(),
    "sv": lang_SV.Words2Num_SV(),
    "sw": lang_SW.Words2Num_SW(),
    "ta": lang_TA.Words2Num_TA(),
    "te": lang_TE.Words2Num_TE(),
    "tet": lang_TET.Words2Num_TET(),
    "tg": lang_TG.Words2Num_TG(),
    "th": lang_TH.Words2Num_TH(),
    "tk": lang_TK.Words2Num_TK(),
    "tl": lang_TL.Words2Num_TL(),
    "tr": lang_TR.Words2Num_TR(),
    "tt": lang_TT.Words2Num_TT(),
    "uk": lang_UK.Words2Num_UK(),
    "ur": lang_UR.Words2Num_UR(),
    "uz": lang_UZ.Words2Num_UZ(),
    "vi": lang_VI.Words2Num_VI(),
    "wo": lang_WO.Words2Num_WO(),
    "yi": lang_YI.Words2Num_YI(),
    "yo": lang_YO.Words2Num_YO(),
    "zh": lang_ZH_CN.Words2Num_ZH_CN(),  # default zh -> simplified Chinese
    "zh_CN": lang_ZH_CN.Words2Num_ZH_CN(),
    "zh_HK": lang_ZH_HK.Words2Num_ZH_HK(),
    "zh_TW": lang_ZH_TW.Words2Num_ZH_TW(),
    # Aliases
    "jp": lang_JA.Words2Num_JA(),
    "cn": lang_ZH_CN.Words2Num_ZH_CN(),
}

CONVERTER_TYPES = ["cardinal", "ordinal", "ordinal_num", "year", "currency"]


def _resolve_lang(lang):
    if lang in CONVERTER_CLASSES:
        return lang
    normalized = lang.replace("-", "_")
    if normalized in CONVERTER_CLASSES:
        return normalized
    parts = normalized.split("_")
    if len(parts) >= 2:
        candidate = "{}_{}".format(parts[0].lower(), parts[1].upper())
        if candidate in CONVERTER_CLASSES:
            return candidate
        if parts[0] in CONVERTER_CLASSES:
            return parts[0]
    if normalized[:2] in CONVERTER_CLASSES:
        return normalized[:2]
    raise NotImplementedError("language %r is not supported" % lang)


def words2num(text, lang="en", to="cardinal", **kwargs):
    """Parse ``text`` (a number written in words) into a numeric value.

    Args:
        text: The words to parse, e.g. ``"forty-two"``.
        lang: Language code (default ``"en"``).
        to: Conversion type — ``cardinal``, ``ordinal``, ``ordinal_num``,
            ``year``, or ``currency``.

    Returns:
        ``int``, ``float``, or ``Decimal`` depending on the input.

    Raises:
        NotImplementedError: if ``lang`` is not registered.
        Words2NumError: if ``text`` cannot be parsed.
    """
    resolved = _resolve_lang(lang)
    converter = CONVERTER_CLASSES[resolved]

    if to not in CONVERTER_TYPES:
        raise NotImplementedError("conversion type %r unsupported" % to)

    return getattr(converter, "to_{}".format(to))(text, **kwargs)


def words2num_sentence(sentence, lang="en", to="cardinal", **kwargs):
    """Convert all word-numbers in ``sentence`` to numeric form.

    Walks the sentence and tries to match the longest possible run of
    number tokens at each position. Non-number tokens pass through.
    """
    from .converters.sentence import SentenceConverter

    return SentenceConverter().convert(sentence, lang=lang, to=to, **kwargs)


# Aliases (parity with num2words2)
convert_sentence = words2num_sentence
sentence_to_words = words2num_sentence
