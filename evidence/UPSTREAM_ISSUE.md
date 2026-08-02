# Upstream issue report (ready to file)

**Target:** https://github.com/SethMMorton/natsort/issues
**Library:** natsort 8.4.0
**Component:** `ns.REAL` / `ns.FLOAT` numeric parsing
**Type:** correctness — silent precision loss produces incorrect ordering

---

## Title

`natsorted(..., alg=ns.REAL)` silently loses ordering for values above the
float maximum (overflow to `inf`)

## Body

### Summary

Under `ns.REAL` (and `ns.FLOAT`), numeric substrings are parsed with Python's
`float()`. Any value larger than the IEEE-754 double maximum (~1.8e308)
overflows to `inf`. Two distinct, clearly-ordered values that both overflow
therefore compare **equal**, and `natsorted` cannot order them — the output
depends on input order (sort stability), not on the actual numeric magnitude.

### Reproduction

```python
>>> from natsort import natsorted, ns
>>> natsorted(["1e400", "1e500"], alg=ns.REAL)
['1e400', '1e500']
>>> natsorted(["1e500", "1e400"], alg=ns.REAL)
['1e500', '1e400']
```

Both calls "succeed", but the two results disagree about whether `1e400` or
`1e500` is larger — because both parsed to `inf`:

```python
>>> from natsort import natsort_keygen
>>> k = natsort_keygen(alg=ns.REAL)
>>> k("1e400"), k("1e500")
(('', inf), ('', inf))
```

Mathematically `1e500 > 1e400`, but the sort treats them as equal.

### Why this matters

The failure is silent — no exception, no warning — and the incorrect order is
plausible-looking. Any dataset containing very large numeric tokens (scientific
data, identifiers with large exponents, synthetic stress inputs) can be sorted
incorrectly with no indication that anything went wrong.

### Suggested fix

Two reasonable options:

1. **Document the limit.** Note in the `ns.REAL`/`ns.FLOAT` docs that values are
   limited to IEEE-754 double range and overflow to `inf`, collapsing order.

2. **Parse to an arbitrary-precision type when overflow is detected.** When
   `float()` yields `inf` but the source string is finite, fall back to
   `decimal.Decimal` (or compare the normalized string form) so ordering is
   preserved. This is heavier but removes the silent-error class entirely.

### How this was found

While building and differentially testing a Rust port of natsort. The Rust port
uses `f64` and therefore reproduces the exact same behavior (both values become
`inf`), so this is not a port defect — it is an upstream characteristic that the
differential testing surfaced. Happy to open a PR for the documentation option
if that is preferred.

### Environment

- natsort 8.4.0 (latest release on PyPI at time of writing; confirmed no newer
  version exists)
- Python 3.12
- Reproducible on a clean install; no other dependencies involved.

### Prior art check

Searched existing issues on the repository before filing; found no existing
report of this specific overflow-to-`inf` ordering loss under `ns.REAL`.
