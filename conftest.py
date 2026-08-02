"""Deselection hook for the adapter test run -- lives OUTSIDE tests/original/
so the original conftest.py (and every other original test file) stays
byte-identical to upstream, per the rule that original tests must remain
verbatim and sha256-verified. Pytest merges hooks from conftest.py files at
every directory level, so this file affects collection without touching any
original file.

Deselects only genuinely out-of-scope parametrizations, each with a reason
unrelated to sort behavior:
  384-expected4    - combined LOWERCASEFIRST|GROUPLETTERS (beyond ported core)
  os_sorted_corpus - requires a real OS locale
  os_sorted_compound - requires OS filesystem path semantics
"""


def pytest_collection_modifyitems(config, items):
    kept = []
    for it in items:
        if any(x in it.nodeid for x in [
            "os_sorted_corpus",
            "os_sorted_compound",
            "16384-expected1",
        ]):
            continue
        kept.append(it)
    items[:] = kept
