"""Minimal compat.locale for running natsort's original tests against the port.

Only the symbols referenced by the unmodified original test suite are provided.
Locale-specific ordering is the documented scope boundary of this ASCII-focused
port, so dumb_sort() reports False (the non-"dumb" path).
"""


def dumb_sort():
    return False


def get_strxfrm():
    return str


def null_string_locale(x=""):
    return x
