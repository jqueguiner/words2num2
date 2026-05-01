# -*- coding: utf-8 -*-
"""words2num2 CLI entrypoint."""
from __future__ import unicode_literals

import sys

from . import __version__, words2num


def main():
    args = sys.argv[1:]
    if not args or args[0] in ("-h", "--help"):
        print(
            "usage: words2num2 <text> [--lang=LANG] [--to=TYPE]\n"
            "  LANG defaults to 'en'\n"
            "  TYPE is cardinal | ordinal | ordinal_num | year | currency"
        )
        return 0
    if args[0] == "--version":
        print("words2num2 {}".format(__version__))
        return 0

    lang = "en"
    to = "cardinal"
    text_parts = []
    for arg in args:
        if arg.startswith("--lang="):
            lang = arg.split("=", 1)[1]
        elif arg.startswith("--to="):
            to = arg.split("=", 1)[1]
        else:
            text_parts.append(arg)
    text = " ".join(text_parts)
    print(words2num(text, lang=lang, to=to))
    return 0


if __name__ == "__main__":
    sys.exit(main())
