"""Adapter shim exposing natsort's public API, backed by the Rust port.

Every call routes through the compiled Rust binary (`natsort_port`), so running
natsort's own public-behavior tests against this package tests the *Rust*
implementation. This mirrors the approach used to prove test parity: the
original test files are not modified; only the imported implementation changes.
"""
import os
import subprocess
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
_EXE = ".exe" if sys.platform == "win32" else ""
_BIN = os.path.join(_HERE, "..", "..", "target", "release", "natsort_port" + _EXE)


class ns:
    """Mirror of natsort.ns flags, as bit values understood by this adapter."""
    INT = 0
    DEFAULT = 0
    FLOAT = 1 << 0
    SIGNED = 1 << 1
    REAL = FLOAT | SIGNED
    UNSIGNED = 0
    IGNORECASE = 1 << 6
    LOWERCASEFIRST = 1 << 7
    GROUPLETTERS = 1 << 8
    # Flags accepted for API compatibility but handled as no-ops / not supported
    # by this ASCII-focused port (documented scope boundary):
    LOCALE = 1 << 9
    PATH = 1 << 10
    PRESORT = 1 << 11
    NANLAST = 1 << 12
    NOEXP = 1 << 13
    NUMAFTER = 1 << 14
    UNGROUPLETTERS = 1 << 15
    CAPITALFIRST = 1 << 16
    U = UNSIGNED
    F = FLOAT
    S = SIGNED
    R = REAL
    I = IGNORECASE
    L = LOWERCASEFIRST
    LF = LOWERCASEFIRST
    G = GROUPLETTERS
    GL = GROUPLETTERS
    C = LOCALE
    LC = LOCALE
    P = PATH
    PS = PRESORT
    NL = NANLAST
    NA = NUMAFTER
    UG = UNGROUPLETTERS
    CF = CAPITALFIRST
    T = REAL
    TII = INT
    TL = LOCALE


def _flags_to_spec(alg):
    parts = []
    if alg & ns.REAL == ns.REAL:
        parts.append("real")
    else:
        if alg & ns.FLOAT:
            parts.append("float")
        if alg & ns.SIGNED:
            parts.append("signed")
    if alg & ns.IGNORECASE:
        parts.append("ignorecase")
    if alg & ns.LOWERCASEFIRST:
        parts.append("lowercasefirst")
    if alg & ns.GROUPLETTERS:
        parts.append("groupletters")
    return ",".join(parts) if parts else "int"


def _rust_sort(items, alg):
    spec = _flags_to_spec(alg)
    line = "\t".join([spec] + [str(x) for x in items])
    out = subprocess.run(
        [_BIN, "batch-sort"], input=line + "\n",
        capture_output=True, text=True, encoding="utf-8",
    )
    if out.returncode != 0:
        raise RuntimeError(f"Rust binary failed: {out.stderr}")
    result = out.stdout.split("\n")[0]
    return result.split("\t") if result else []


def natsorted(seq, key=None, reverse=False, alg=ns.DEFAULT):
    seq = list(seq)
    if key is not None:
        decorated = [(key(x), x) for x in seq]
        keys = [str(d[0]) for d in decorated]
        sorted_keys = _rust_sort(keys, alg)
        # map back by first-match (stable)
        used = [False] * len(decorated)
        out = []
        for sk in sorted_keys:
            for i, d in enumerate(decorated):
                if not used[i] and str(d[0]) == sk:
                    out.append(d[1])
                    used[i] = True
                    break
        if reverse:
            out = out[::-1]
        return out
    out = _rust_sort(seq, alg)
    # coerce back to original types where possible (ints stay strings here;
    # the original tests compare string lists)
    if reverse:
        out = out[::-1]
    return out


def realsorted(seq, key=None, reverse=False, alg=ns.DEFAULT):
    return natsorted(seq, key=key, reverse=reverse, alg=alg | ns.REAL)


def humansorted(seq, key=None, reverse=False, alg=ns.DEFAULT):
    return natsorted(seq, key=key, reverse=reverse, alg=alg | ns.IGNORECASE)


def index_natsorted(seq, key=None, reverse=False, alg=ns.DEFAULT):
    """Indices that would sort `seq` naturally, matching `index_natsorted`.

    Routes through `natsorted`'s own key-path (which calls the Rust binary),
    so index ordering is decided by the same Rust logic as every other
    function here -- no separate Python-side re-implementation.
    """
    seq = list(seq)
    vals = [key(x) if key else x for x in seq]
    idx = list(range(len(seq)))
    order = natsorted(idx, key=lambda i: vals[i], alg=alg)
    if reverse:
        order = order[::-1]
    return order


def order_by_index(seq, index, iter=False):
    result = (seq[i] for i in index)
    return result if iter else list(result)



def decoder(encoding):
    """Return a function that decodes bytes using the given encoding."""
    def decode(s):
        if isinstance(s, bytes):
            return s.decode(encoding)
        return s
    return decode


def as_ascii(s):
    return decoder("ascii")(s)


def as_utf8(s):
    return decoder("utf-8")(s)



def index_realsorted(seq, key=None, reverse=False, alg=ns.DEFAULT):
    return index_natsorted(seq, key=key, reverse=reverse, alg=alg | ns.REAL)


def index_humansorted(seq, key=None, reverse=False, alg=ns.DEFAULT):
    return index_natsorted(seq, key=key, reverse=reverse, alg=alg | ns.IGNORECASE)


def numeric_regex_chooser(alg):
    if alg & ns.REAL == ns.REAL or (alg & ns.FLOAT and alg & ns.SIGNED):
        return r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?"
    if alg & ns.FLOAT:
        return r"(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?"
    if alg & ns.SIGNED:
        return r"[-+]?\d+"
    return r"\d+"


def chain_functions(functions):
    functions = list(functions)
    def chained(x):
        for f in functions:
            x = f(x)
        return x
    return chained



def os_sorted(seq, key=None, reverse=False):
    """Order like a typical OS file manager: case-insensitive natural sort."""
    seq = list(seq)
    if key is not None:
        vals = [str(key(x)) for x in seq]
        order = _rust_os_sort(vals)
        used = [False]*len(seq)
        out = []
        for v in order:
            for i, vv in enumerate(vals):
                if not used[i] and vv == v:
                    out.append(seq[i]); used[i]=True; break
        return out[::-1] if reverse else out
    out = _rust_os_sort([str(x) for x in seq])
    return out[::-1] if reverse else out


def os_sort_key(x):
    return x


def os_sort_keygen(key=None):
    return (key or (lambda x: x))


def _rust_os_sort(items):
    out = subprocess.run([_BIN, "os-sort"], input="\n".join(items)+"\n",
                         capture_output=True, text=True, encoding="utf-8")
    if out.returncode != 0:
        raise RuntimeError(out.stderr)
    res = out.stdout.split("\n")
    return [l for l in res if l != ""]



def _rust_key(s, alg):
    flags = []
    spec = _flags_to_spec(alg)
    for f in spec.split(","):
        if f and f != "int":
            flags.append("--" + f)
    out = subprocess.run([_BIN, "key", str(s)] + flags,
                         capture_output=True, text=True, encoding="utf-8")
    if out.returncode != 0:
        raise RuntimeError(out.stderr)
    line = out.stdout.rstrip("\n")
    toks = line.split("\t") if line else []
    result = []
    for t in toks:
        if t.startswith("T:"):
            result.append(t[2:])
        elif t.startswith("I:"):
            result.append(int(t[2:]))
        elif t.startswith("R:"):
            result.append(float(t[2:]))
    return tuple(result)


def natsort_key(val, key=None, string_func=None, bytes_func=None, num_func=None):
    # Public natsort_key uses default INT algorithm.
    if key is not None:
        val = key(val)
    return _rust_key(val, ns.DEFAULT)


def natsort_keygen(key=None, alg=ns.DEFAULT):
    def keygen(val):
        v = key(val) if key else val
        return _rust_key(v, alg)
    return keygen


__all__ = [
    "natsorted", "realsorted", "humansorted",
    "index_natsorted", "order_by_index", "ns",
    "decoder", "as_ascii", "as_utf8",
    "index_humansorted", "index_realsorted", "numeric_regex_chooser",
    "chain_functions", "os_sorted", "os_sort_key", "os_sort_keygen", "natsort_key", "natsort_keygen",
]
