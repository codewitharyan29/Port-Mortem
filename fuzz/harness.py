#!/usr/bin/env python3
"""Differential fuzz: the Rust natsort port vs the real Python natsort library.

Generates random strings and compares the sorted order produced by the Rust
binary against Python natsort, across every ported algorithm. Exits non-zero if
any divergence is found. ASCII inputs only, matching the port's documented scope.
"""
import os
import random
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # repo root (this file is one level deep)
_EXE = ".exe" if sys.platform == "win32" else ""
BIN = os.path.join(REPO, "target", "release", "natsort_port" + _EXE)

# Load the real natsort (vendored path or installed)
VENDOR = os.path.join(REPO, "vendor", "natsort-8.4.0")
if os.path.isdir(VENDOR):
    sys.path.insert(0, VENDOR)
from natsort import natsorted, ns  # noqa: E402

ALGS = [
    ("int", [], ns.INT),
    ("real", ["real"], ns.REAL),
    ("signed", ["signed"], ns.SIGNED),
    ("float", ["float"], ns.FLOAT),
    ("ignorecase", ["ignorecase"], ns.IGNORECASE),
    ("lowercasefirst", ["lowercasefirst"], ns.LOWERCASEFIRST),
    ("groupletters", ["groupletters"], ns.GROUPLETTERS),
    ("real_noexp", ["real", "noexp"], ns.REAL | ns.NOEXP),
    ("presort", ["presort"], ns.PRESORT),
]

ALPHABET = "abABxyz0123456789.-+eE"


def rust_sort(items, flags):
    spec = ",".join(flags) if flags else "int"
    line = "\t".join([spec] + items)
    out = subprocess.run([BIN, "batch-sort"], input=line + "\n",
                         capture_output=True, text=True, encoding="utf-8")
    res = out.stdout.split("\n")[0]
    return res.split("\t") if res else []


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 3000
    rng = random.Random(12345)

    EDGE = ["", "0", "-0", "00", "1.", ".1", "1.e5", "e", "-", "+",
            "1e999", "1e-999", "9"*40, "1.2.3", "a1b2c3", "007", "-0.0",
            # Unicode decimal digits (fullwidth, Arabic-Indic, Thai, Devanagari) --
            # these concatenate into multi-digit numbers, matching natsort.
            "１２３", "٥٦", "๗๘", "९", "１.５", "abc２３",
            # Isolated digit characters (circled, superscript, subscript) --
            # each is a SEPARATE single-digit number, never concatenated.
            "①②③", "²", "₃", "①a", "1①",
            # Numeric non-digits (Roman numerals, fractions, circled tens) --
            # numbers only under REAL/FLOAT, text under INT.
            "Ⅷ", "½", "¼", "⑩", "Ⅷ0.E",
            # Casefold-special characters (differ from simple to_lowercase) --
            # exercised under IGNORECASE/GROUPLETTERS.
            "ß", "ﬁ", "ς", "İ", "ẞ2", "Σﬁ",
            # Tie-forcing pairs (same natural key, different string) -- these
            # are what makes ns.PRESORT observably different from a plain
            # stable sort: "a1"/"a01" both -> ("a", 1).
            "a1", "a01", "a001", "b2", "b02"]

    def gen():
        if rng.random() < 0.25:
            return rng.choice(EDGE)
        return "".join(rng.choice(ALPHABET) for _ in range(rng.randint(1, 9)))

    print(f"# Differential fuzz: Rust natsort port vs Python natsort 8.4.0")
    print(f"# seed=12345 | {n} rounds | {len(ALGS)} algorithms")
    per_alg = {name: 0 for name, _, _ in ALGS}
    total = 0
    start = time.time()
    for i in range(n):
        items = [gen() for _ in range(rng.randint(2, 12))]
        for name, flags, alg in ALGS:
            py = natsorted(items, alg=alg)
            rs = rust_sort(items, flags)
            if py != rs:
                per_alg[name] += 1
                total += 1
                if total <= 5:
                    print(f"DIVERGE [{name}] {items}\n  py={py}\n  rs={rs}")
        # Progress line every 200 rounds -- subprocess spawn is much slower on
        # Windows than Linux, so a long silent run can look "stuck" when it
        # is actually just working through ~7x subprocess calls per round.
        if (i + 1) % 200 == 0:
            elapsed = time.time() - start
            print(f"  ...{i+1}/{n} rounds, {elapsed:.1f}s elapsed, {total} divergences so far")
    print(f"RESULT: per-algorithm divergences: {per_alg}")
    print(f"TOTAL DIVERGENCES: {total}")
    sys.exit(0 if total == 0 else 1)


if __name__ == "__main__":
    main()
