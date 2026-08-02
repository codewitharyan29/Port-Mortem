#!/usr/bin/env python3
"""Lightweight mutation testing for the natsort port.

Applies a set of deliberate, semantics-changing edits to src/lib.rs one at a
time, rebuilds, and checks that the differential fuzzer (which compares against
Python natsort) catches each one. A mutation that survives = a gap in the tests.
Restores the original source afterward.
"""
import os
import shutil
import subprocess
import sys

REPO = os.path.dirname(os.path.abspath(__file__))
LIB = os.path.join(REPO, "src", "lib.rs")

# (description, find, replace) — each flips a real behavior.
MUTATIONS = [
    ("int compare direction", "(Chunk::Int(a), Chunk::Int(b)) => a.cmp(b),",
     "(Chunk::Int(a), Chunk::Int(b)) => b.cmp(a),"),
    ("text compare direction", "(Chunk::Text(a), Chunk::Text(b)) => a.cmp(b),",
     "(Chunk::Text(a), Chunk::Text(b)) => b.cmp(a),"),
    ("real float compare direction", "a.partial_cmp(b).unwrap_or_else(|| a.total_cmp(b))",
     "b.partial_cmp(a).unwrap_or_else(|| b.total_cmp(a))"),
    ("length tiebreak flipped", "ka.len().cmp(&kb.len())", "kb.len().cmp(&ka.len())"),
    ("sign negation dropped", "Some((Chunk::Int(val * sign), (i - start)))",
     "Some((Chunk::Int(val), (i - start)))"),
    # NOTE: the number-vs-text-at-same-position branch is provably unreachable
    # for aligned keys produced by natsort_key (both keys start with text and
    # alternate), so a mutation there is not observable via behavior. It is
    # documented as defensive-only in lib.rs rather than covered by a test.
]


def build():
    r = subprocess.run(["cargo", "build", "--release"], cwd=REPO,
                       capture_output=True, text=True)
    return r.returncode == 0


def fuzz_catches():
    r = subprocess.run([sys.executable, "fuzz_harness.py", "300"], cwd=REPO,
                       capture_output=True, text=True)
    return r.returncode != 0  # non-zero = divergence found = mutation caught


def main():
    original = open(LIB, encoding="utf-8").read()
    caught = 0
    total = len(MUTATIONS)
    try:
        for desc, find, repl in MUTATIONS:
            if find not in original:
                print(f"  SKIP (pattern not found): {desc}")
                total -= 1
                continue
            open(LIB, "w", encoding="utf-8").write(original.replace(find, repl, 1))
            if not build():
                print(f"  (mutation didn't compile, counts as caught): {desc}")
                caught += 1
            elif fuzz_catches():
                print(f"  CAUGHT: {desc}")
                caught += 1
            else:
                print(f"  *** SURVIVED (test gap!): {desc}")
            open(LIB, "w", encoding="utf-8").write(original)
    finally:
        open(LIB, "w", encoding="utf-8").write(original)
        build()
    print(f"\nMUTATION SCORE: {caught}/{total} caught")
    sys.exit(0 if caught == total else 1)


if __name__ == "__main__":
    main()
