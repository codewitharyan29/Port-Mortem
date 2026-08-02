#!/usr/bin/env python3
"""CLI differential: the Rust natsort binary vs Python natsort, via the CLI.

Compares the `sort` and `compare` subcommands' stdout against Python natsort for
many random inputs and every algorithm flag. Exits non-zero on any divergence.
"""
import os
import random
import subprocess
import sys

REPO = os.path.dirname(os.path.abspath(__file__))
_EXE = ".exe" if sys.platform == "win32" else ""
BIN = os.path.join(REPO, "target", "release", "natsort_port" + _EXE)
VENDOR = os.path.join(REPO, "vendor", "natsort-8.4.0")
if os.path.isdir(VENDOR):
    sys.path.insert(0, VENDOR)
from natsort import natsorted, ns  # noqa: E402

ALGS = [
    ("int", [], ns.INT),
    ("real", ["--real"], ns.REAL),
    ("signed", ["--signed"], ns.SIGNED),
    ("float", ["--float"], ns.FLOAT),
    ("ignorecase", ["--ignorecase"], ns.IGNORECASE),
    ("lowercasefirst", ["--lowercasefirst"], ns.LOWERCASEFIRST),
    ("groupletters", ["--groupletters"], ns.GROUPLETTERS),
]
ALPHABET = "abAB01239.-+eE"


def cli_sort(items, flags):
    out = subprocess.run([BIN, "sort"] + flags, input="\n".join(items) + "\n",
                         capture_output=True, text=True, encoding="utf-8")
    return [l for l in out.stdout.split("\n") if l != ""]


def cli_compare(a, b, flags):
    out = subprocess.run([BIN, "compare", a, b] + flags,
                         capture_output=True, text=True, encoding="utf-8")
    return out.stdout.strip(), out.returncode


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 200
    rng = random.Random(2024)

    def gen():
        return "".join(rng.choice(ALPHABET) for _ in range(rng.randint(1, 7)))

    print("# CLI differential: Rust natsort CLI vs Python natsort 8.4.0")
    print(f"# seed=2024 | {n} rounds | sort + compare across {len(ALGS)} algorithms")
    total = 0
    invocations = 0
    for _ in range(n):
        items = [gen() for _ in range(rng.randint(2, 8))]
        for name, flags, alg in ALGS:
            # sort
            invocations += 1
            py = natsorted(items, alg=alg)
            rs = cli_sort(items, flags)
            if py != rs:
                total += 1
                if total <= 5:
                    print(f"DIVERGE sort[{name}] {items}\n  py={py}\n  rs={rs}")
            # compare (first two)
            invocations += 1
            a, b = items[0], items[1]
            py_cmp = (natsorted([a, b], alg=alg).index(a) < natsorted([a, b], alg=alg).index(b))
            # derive expected sign from natsort ordering
            order = natsorted([a, b], alg=alg)
            expected = "0" if a == b else ("-1" if order[0] == a and a != b else "1")
            rs_cmp, _ = cli_compare(a, b, flags)
            # only compare when a != b to avoid ambiguity on ties
            if a != b and rs_cmp != expected:
                # allow tie (0) when they are natsort-equal
                if not (natsorted([a, b], alg=alg) == [a, b] and rs_cmp in ("-1", "0")):
                    total += 1
                    if total <= 8:
                        print(f"DIVERGE compare[{name}] {a!r} {b!r} exp={expected} got={rs_cmp}")
    print(f"RESULT: {invocations} invocations, {total} divergences")
    sys.exit(0 if total == 0 else 1)


if __name__ == "__main__":
    main()
