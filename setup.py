# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
#
# This library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.

from io import open

from setuptools import find_packages, setup

PACKAGE_NAME = "words2num2"

CLASSIFIERS = [
    "Development Status :: 4 - Beta",
    "Intended Audience :: Developers",
    "Programming Language :: Python :: 3.8",
    "Programming Language :: Python :: 3.9",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Programming Language :: Python :: 3.13",
    "Topic :: Software Development :: Internationalization",
    "Topic :: Software Development :: Libraries :: Python Modules",
    "Topic :: Software Development :: Localization",
    "Topic :: Text Processing :: Linguistic",
]

LONG_DESC = open("README.rst", "rt", encoding="utf-8").read()


setup(
    name=PACKAGE_NAME,
    use_scm_version=True,
    setup_requires=["setuptools_scm"],
    description=(
        "Inverse of num2words2: convert spoken-form numbers back to "
        "numeric values across 100+ languages."
    ),
    long_description=LONG_DESC,
    long_description_content_type="text/x-rst",
    license="LGPL-2.1",
    author="Jean-Louis Queguiner",
    author_email="jlqueguiner@gladia.io",
    maintainer="Jean-Louis Queguiner",
    maintainer_email="jlqueguiner@gladia.io",
    keywords=(
        "number word numbers words parse parser convert conversion i18n "
        "localisation localization internationalisation internationalization "
        "asr speech llm"
    ),
    url="https://github.com/jqueguiner/words2num2",
    packages=find_packages(exclude=["tests"]),
    install_requires=["docopt>=0.6.2", "num2words2>=0.1.0.dev0"],
    classifiers=CLASSIFIERS,
    scripts=["bin/words2num2"],
)
