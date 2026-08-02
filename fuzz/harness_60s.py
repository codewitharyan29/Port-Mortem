#!/usr/bin/env python3
"""Differential Fuzz Survivor (bonus): run continuously against the shared
public API for at least 60 seconds, zero divergences, log published.

Unlike fuzz_harness.py (a fixed round count for quick local iteration), this
runs on a WALL-CLOCK timer so it satisfies "at least 60 continuous seconds"
literally, and writes a timestamped, appendable log to
fuzz/log.txt as required for publication (per the anatomy spec: fuzz/log.txt).
"""
import os
import random
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # repo root (this file is one level deep)
_EXE = ".exe" if sys.platform == "win32" else ""
BIN = os.path.join(REPO, "target", "release", "natsort_port" + _EXE)
VENDOR = os.path.join(REPO, "vendor", "natsort-8.4.0")
if os.path.isdir(VENDOR):
    sys.path.insert(0, VENDOR)
from natsort import natsorted, ns  # noqa: E402

ALGS = [
    ("int", [], ns.INT), ("real", ["real"], ns.REAL), ("signed", ["signed"], ns.SIGNED),
    ("float", ["float"], ns.FLOAT), ("ignorecase", ["ignorecase"], ns.IGNORECASE),
    ("lowercasefirst", ["lowercasefirst"], ns.LOWERCASEFIRST),
    ("groupletters", ["groupletters"], ns.GROUPLETTERS),
]
# Full charset: ASCII + Unicode decimal/isolated/numeric digits + casefold-special.
ALPHABET = list("abcABCXYZ0123456789.-+eE ") + [
    "\uff11","\uff12","\u0665","\u0e57","\u096f",   # decimal unicode
    "\u2460","\u00b2","\u2469",                       # isolated digits
    "\u2167","\u00bd","\u00bc",                       # numeric non-digits
    "\u00e9","\u00f1","\u00df","\ufb01","\u03c2",     # accents + casefold-special
]


def rust_sort(items, flags):
    spec = ",".join(flags) if flags else "int"
    out = subprocess.run([BIN, "batch-sort"], input="\t".join([spec] + items) + "\n",
                         capture_output=True, text=True, encoding="utf-8")
    r = out.stdout.split("\n")[0]
    return r.split("\t") if r else []


def main():
    duration = float(sys.argv[1]) if len(sys.argv) > 1 else 60.0
    rng = random.Random(20260731)  # kickoff-dated seed, reproducible
    log_path = os.path.join(REPO, "fuzz", "log.txt")  # per spec: fuzz/log.txt

    start = time.time()
    rounds = 0
    comparisons = 0
    divergences = 0
    per_alg = {name: 0 for name, _, _ in ALGS}

    with open(log_path, "w", encoding="utf-8") as log:
        header = (
            f"# Differential Fuzz Survivor -- continuous run\n"
            f"# target: {duration:.0f}s minimum, shared public API "
            f"(natsorted vs `natsort_port batch-sort`)\n"
            f"# seed=20260731 | started={time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}\n"
        )
        print(header, end="")
        log.write(header)

        while time.time() - start < duration:
            n = rng.randint(2, 10)
            items = ["".join(rng.choice(ALPHABET) for _ in range(rng.randint(1, 8)))
                     for _ in range(n)]
            for name, flags, alg in ALGS:
                comparisons += 1
                py = natsorted(items, alg=alg)
                rs = rust_sort(items, flags)
                if py != rs:
                    divergences += 1
                    per_alg[name] += 1
                    line = f"DIVERGE t={time.time()-start:.1f}s [{name}] {items}\n  py={py}\n  rs={rs}\n"
                    print(line, end="")
                    log.write(line)
            rounds += 1
            if rounds % 200 == 0:
                elapsed = time.time() - start
                progress = f"  ...{elapsed:.0f}s elapsed, {comparisons} comparisons, {divergences} divergences\n"
                print(progress, end="")
                log.write(progress)

        elapsed = time.time() - start
        footer = (
            f"\nRESULT: {elapsed:.1f}s continuous run (>= {duration:.0f}s required)\n"
            f"  rounds={rounds}  comparisons={comparisons}  per-algorithm={per_alg}\n"
            f"  TOTAL DIVERGENCES: {divergences}\n"
        )
        print(footer, end="")
        log.write(footer)

    sys.exit(0 if divergences == 0 else 1)


if __name__ == "__main__":
    main()
