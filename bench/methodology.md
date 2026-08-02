# Benchmark methodology

Per the track's guidance: report honest p99 latency and RSS, not just hot-loop
throughput. Numbers below are from one reference run; re-run `bench/run.py` and
the startup measurement to reproduce on your own machine — timings vary by
hardware.

## Workload

50,000 filename-like strings, generated as `file{i}-v{i%37}.{i%5}.log` for
`i` in `0..50000`. This is a directory-listing shape rather than a synthetic
best case (e.g. all-identical or already-sorted input, which would flatter
either implementation unfairly).

## What is measured

**Sort latency** (`bench/run.py`): both sides sort the identical in-memory list.
2 warmup runs are discarded, then 15 timed runs. We report mean, p50, p95, and
p99 — not just mean/throughput, since tail latency is what the track asks for.

- Python side: `natsorted()` called in-process, timed with `time.perf_counter()`.
- Rust side: the compiled binary's own `Instant::now()` timer around the sort
  call only (`natsort_port bench N`), so process-spawn overhead is excluded
  from the *sort* number — but see Startup below, where that overhead is
  measured and reported separately, honestly, rather than hidden.

**Peak RSS**: `resource.getrusage` for the Python process; `/usr/bin/time -v`
"Maximum resident set size" for the Rust binary.

**Process startup** (separate from sort latency, measured explicitly because
the track asks for it): wall-clock time for a fresh process to become ready to
sort — `python3 -c "from natsort import natsorted"` vs `natsort_port bench 1` —
10 runs each, reporting mean/p50/max.

## Results (reference runs — two machines, reported separately)

See `bench/results.json` for the machine-readable version (Windows run).

**Windows 11, Python 3.14** (the machine this port was developed and tested on):

| Metric | Python natsort 8.4.0 | Rust port | Ratio |
|---|---|---|---|
| Sort, mean | 1309.3 ms | 217.2 ms | **6.03x** |
| Sort, p50 | 1231.1 ms | 204.7 ms | 6.01x |
| Sort, p95 | 1854.8 ms | 300.0 ms | 6.18x |
| Sort, p99 | 2180.6 ms | 313.2 ms | **6.96x** |
| Peak RSS | 42.2 MB | 24.2 MB | 1.74x less |

**Linux (sandbox reference run)**, for comparison — same code, different
machine, meaningfully different absolute numbers:

| Metric | Python natsort 8.4.0 | Rust port | Ratio |
|---|---|---|---|
| Sort, mean | 536.7 ms | 81.8 ms | 6.56x |
| Sort, p99 | 546.7 ms | 117.9 ms | 4.64x |
| Peak RSS | 31.8 MB | 27.0 MB | 1.18x less |
| Process startup, mean | 57.6 ms | 2.0 ms | 28.6x |

## Honest notes

- **Numbers genuinely differ by machine** — this is disclosed rather than
  picking whichever run looks best. Re-run `bench/run.py` on your own hardware;
  don't treat either table above as universal.
- **Not 10x on sort throughput, on either machine.** natsort's Python
  implementation is already reasonably optimized (cached regex, minimal
  per-key allocation), which caps the ceiling for a drop-in algorithmic port.
  Reported as measured rather than picking a workload that would flatter the
  number.
- **p99 speedup can exceed or trail mean speedup depending on the run** (6.96x
  p99 vs 6.03x mean on Windows; 4.64x p99 vs 6.56x mean on Linux) — tail
  latency is noisier than the median on both platforms. Reported as measured,
  not smoothed.
- **RSS advantage is real but modest, not dramatic** (1.74x on Windows, 1.18x
  on Linux) after the decorate-sort-undecorate optimization (see
  `DECISIONS.md` #18) that precomputes all sort keys up front — trading memory
  for the speed gain. This is the same tradeoff Python's own `sorted(key=...)`
  makes; it is disclosed, not hidden.
- **Startup time** (measured on Linux; Windows process-spawn overhead differs
  and was not separately isolated there): Rust binary starts ~28.6x faster
  than the Python interpreter + import, which matters more than sort
  throughput for short-lived CLI invocations.

## Reproduce

```bash
python3 bench/run.py 50000 15
```
