#!/usr/bin/env python3
"""Dedicated differential test for index_natsorted / index_realsorted /
index_humansorted specifically.

These functions once had a real adapter bug: index_natsorted computed the
correct Rust-routed order into a variable named `order`, then discarded it
and returned a separate, buggy pure-Python re-implementation (`_natkey`) that
did not understand signed numbers, REAL/FLOAT parsing, or Unicode digits.
Example: index_natsorted(["-5","-1","3"], alg=ns.REAL) returned [2,1,0]
(wrong) instead of [0,1,2] (correct, since -5 < -1 < 3). Fixed by having
index_natsorted route through natsorted's own key-path (real Rust binary),
with no separate Python-side sort logic. This script guards that fix
directly, seeded and reproducible like the other fuzz harnesses.
"""
import os
import random
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO, "adapter"))
sys.path.insert(0, os.path.join(REPO, "vendor", "natsort-8.4.0"))
from natsort import index_natsorted as our_index, ns  # noqa: E402

# Import the *real* natsort as a separate reference, avoiding module-cache
# collision with the adapter's same-named package.
import importlib
del sys.modules["natsort"]
sys.path.remove(os.path.join(REPO, "adapter"))
real = importlib.import_module("natsort")
sys.path.insert(0, os.path.join(REPO, "adapter"))

ALGS = [
    ("int", ns.INT), ("real", ns.REAL), ("signed", ns.SIGNED),
    ("float", ns.FLOAT), ("ignorecase", ns.IGNORECASE),
]
ALPHABET = "abAB0123456789.-+eE"


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 500
    rng = random.Random(99)  # fixed seed, reproducible
    total = 0
    div = 0
    print(f"# index_natsorted differential: seed=99 | {n} rounds | {len(ALGS)} algorithms")
    for _ in range(n):
        items = ["".join(rng.choice(ALPHABET) for _ in range(rng.randint(1, 6)))
                 for _ in range(rng.randint(2, 8))]
        for name, alg in ALGS:
            total += 1
            real_idx = real.index_natsorted(items, alg=alg)
            our_idx = our_index(items, alg=alg)
            if real_idx != our_idx:
                div += 1
                if div <= 5:
                    print(f"DIVERGE [{name}] {items}\n  real={real_idx}\n  our ={our_idx}")
    print(f"RESULT: {total} invocations, {div} divergences")
    sys.exit(0 if div == 0 else 1)


if __name__ == "__main__":
    main()
