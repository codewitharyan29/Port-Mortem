#![forbid(unsafe_code)]
//! A Rust port of the Python [natsort](https://github.com/SethMMorton/natsort)
//! library: natural ("human") sort ordering of strings.
//!
//! natsort works by a *key function* that splits a string into an alternating
//! sequence of text and numeric chunks, e.g. `"num10"` → `("num", 10)`, so the
//! numeric parts compare numerically rather than lexically. This crate
//! reproduces that key for the core algorithms selected by the `Ns` flags:
//! INT (default), REAL, FLOAT, SIGNED, and the text transforms IGNORECASE,
//! LOWERCASEFIRST and GROUPLETTERS.
//!
//! The scope note on numbers: Python natsort recognizes a large set of Unicode
//! numeric characters (Roman numerals, circled digits, CJK numerals). This port
//! handles ASCII digits `0-9`, which covers the overwhelming majority of real
//! inputs; the Unicode-numeral boundary is documented and the differential
//! fuzzers generate ASCII.

use std::cmp::Ordering;

mod unicode_digits;
mod unicode_numeric;
mod casefold;
use unicode_digits::{unicode_digit, unicode_isolated_digit};
use unicode_numeric::unicode_numeric;
use unicode_normalization::UnicodeNormalization;
use casefold::casefold_special;

/// True if `c` has a decimal-digit value (ASCII or Unicode) per Python's
/// `unicodedata.digit`. This is what makes the port recognize the same numeric
/// characters natsort does (fullwidth ０-９, Arabic-Indic ٠-٩, Thai ๐-๙, etc.).
fn is_digit_char(c: char) -> bool {
    unicode_digit(c).is_some()
}

/// Algorithm-selection flags, mirroring Python natsort's `ns` enum for the
/// subset of behaviors this port implements. Combine with `|`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ns {
    /// Parse numbers as signed floats (implies SIGNED). `ns.REAL`.
    pub real: bool,
    /// Parse numbers as floats without forcing a sign. `ns.FLOAT`.
    pub float: bool,
    /// A leading +/- directly before a digit attaches to the number. `ns.SIGNED`.
    pub signed: bool,
    /// Fold text to lowercase before comparing. `ns.IGNORECASE`.
    pub ignorecase: bool,
    /// Swap the case of each letter. `ns.LOWERCASEFIRST`.
    pub lowercasefirst: bool,
    /// Expand each letter to lowercase-then-original. `ns.GROUPLETTERS`.
    pub groupletters: bool,
    /// Do not treat a trailing e/E + digits as a float exponent. `ns.NOEXP`.
    pub noexp: bool,
    /// Presort the sequence lexicographically before the natural sort, so
    /// ties break by string order instead of original input order. `ns.PRESORT`.
    pub presort: bool,
}

impl Ns {
    /// The default algorithm, equivalent to `ns.INT` / `ns.DEFAULT`.
    pub const DEFAULT: Ns = Ns {
        real: false,
        float: false,
        signed: false,
        ignorecase: false,
        lowercasefirst: false,
        groupletters: false,
        noexp: false,
        presort: false,
    };

    /// `ns.REAL`: signed floats.
    pub fn real() -> Ns {
        Ns { real: true, signed: true, float: true, ..Ns::DEFAULT }
    }

    /// Whether numbers should be parsed as floating point.
    fn numbers_are_float(&self) -> bool {
        self.real || self.float
    }

    /// Whether a leading sign should attach to a following number.
    fn sign_attaches(&self) -> bool {
        self.signed || self.real
    }
}

/// One chunk of a split natural-sort key.
#[derive(Debug, Clone)]
pub enum Chunk {
    /// A run of transformed text.
    Text(String),
    /// An integer-valued number (used by INT-family algorithms).
    Int(i128),
    /// A float-valued number (used by REAL/FLOAT algorithms). Stored with a
    /// total ordering so keys are always comparable.
    Real(f64),
}

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.cmp_chunk(other) == Ordering::Equal
    }
}
impl Eq for Chunk {}

impl Chunk {
    fn cmp_chunk(&self, other: &Chunk) -> Ordering {
        match (self, other) {
            (Chunk::Text(a), Chunk::Text(b)) => a.cmp(b),
            (Chunk::Int(a), Chunk::Int(b)) => a.cmp(b),
            (Chunk::Real(a), Chunk::Real(b)) => {
                // Python compares floats with ==/<, where -0.0 == 0.0. Use the
                // partial order first (so signed zeros tie and input order is
                // kept by the stable sort); fall back to total_cmp only when a
                // NaN makes partial_cmp return None, keeping a deterministic
                // order for degenerate inputs.
                a.partial_cmp(b).unwrap_or_else(|| a.total_cmp(b))
            }
            // Mixed int/real can occur only if algorithms are mixed within a
            // comparison, which we never do; compare by float value as a safe
            // fallback.
            (Chunk::Int(a), Chunk::Real(b)) => {
                let x = *a as f64;
                x.partial_cmp(b).unwrap_or_else(|| x.total_cmp(b))
            }
            (Chunk::Real(a), Chunk::Int(b)) => {
                let y = *b as f64;
                a.partial_cmp(&y).unwrap_or_else(|| a.total_cmp(&y))
            }
            // Aligned natsort keys always compare like-with-like at each
            // position (both start with a text chunk, then alternate), so a
            // number chunk is never compared to a text chunk at the same index.
            // These arms exist only for totality; they are unreachable for keys
            // produced by `natsort_key`. We give them a deterministic rule
            // (numbers before text) rather than panicking, so the function is
            // total even if fed hand-built keys.
            (Chunk::Int(_) | Chunk::Real(_), Chunk::Text(_)) => Ordering::Less,
            (Chunk::Text(_), Chunk::Int(_) | Chunk::Real(_)) => Ordering::Greater,
        }
    }
}

/// Apply the case/letter transforms of the algorithm to a run of text.
fn transform_text(text: &str, alg: Ns) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if alg.groupletters {
            // each letter -> casefold(c) followed by c (natsort uses casefold)
            if let Some(folded) = casefold_special(c) {
                out.push_str(folded);
            } else {
                for lc in c.to_lowercase() {
                    out.push(lc);
                }
            }
            out.push(c);
        } else if alg.ignorecase {
            // natsort uses str.casefold() for IGNORECASE, which is more
            // aggressive than lowercasing (ß->ss, ﬁ->fi, ς->σ, ...).
            if let Some(folded) = casefold_special(c) {
                out.push_str(folded);
            } else {
                for lc in c.to_lowercase() {
                    out.push(lc);
                }
            }
        } else if alg.lowercasefirst {
            // swap case
            if c.is_uppercase() {
                out.extend(c.to_lowercase());
            } else if c.is_lowercase() {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Look at whether a number (possibly signed / float) begins at `chars[i..]`.
/// Returns the parsed chunk and the number of characters consumed, or None.
fn match_number(chars: &[char], start: usize, alg: Ns) -> Option<(Chunk, usize)> {
    let mut i = start;
    let n = chars.len();
    let mut sign = 1i128;
    let mut consumed_sign = 0;

    // Optional sign, only if it attaches and a digit (or . for real) follows.
    if alg.sign_attaches() && i < n && (chars[i] == '+' || chars[i] == '-') {
        let next = chars.get(i + 1).copied();
        let digit_follows = next.map(is_digit_char).unwrap_or(false);
        let dot_digit_follows = alg.numbers_are_float()
            && next == Some('.')
            && chars.get(i + 2).copied().map(is_digit_char).unwrap_or(false);
        if digit_follows || dot_digit_follows {
            if chars[i] == '-' {
                sign = -1;
            }
            i += 1;
            consumed_sign = 1;
        }
    }

    let int_start = i;
    while i < n && is_digit_char(chars[i]) {
        i += 1;
    }
    let has_int_digits = i > int_start;

    if alg.numbers_are_float() {
        // Fractional part: a '.' followed by zero or more digits, but only if
        // there were integer digits OR at least one fractional digit. Python
        // accepts "1." (-> 1.0) and ".5" (-> 0.5), and crucially "1." can still
        // take an exponent ("1.e133" -> 1e133).
        let mut has_frac_dot = false;
        let mut frac_end = i;
        if i < n && chars[i] == '.' {
            let digit_after_dot =
                chars.get(i + 1).copied().map(is_digit_char).unwrap_or(false);
            if has_int_digits || digit_after_dot {
                frac_end = i + 1;
                while frac_end < n && is_digit_char(chars[frac_end]) {
                    frac_end += 1;
                }
                has_frac_dot = true;
            }
        }
        if !has_int_digits && !has_frac_dot {
            return None;
        }
        let mut end = if has_frac_dot { frac_end } else { i };
        // Exponent: e/E, optional sign, then at least one digit -- unless
        // ns.NOEXP says to treat 'e'/'E' as ordinary text instead.
        if !alg.noexp && end < n && (chars[end] == 'e' || chars[end] == 'E') {
            let mut j = end + 1;
            if j < n && (chars[j] == '+' || chars[j] == '-') {
                j += 1;
            }
            let exp_start = j;
            while j < n && is_digit_char(chars[j]) {
                j += 1;
            }
            if j > exp_start {
                end = j;
            }
        }
        // Build a parseable float string. Python parses "1." as 1.0 and ".5"
        // as 0.5; Rust's f64::from_str rejects a trailing dot and a leading
        // bare dot in some forms, so normalize.
        // Normalize any Unicode decimal digits to ASCII so f64 parsing works
        // (e.g. fullwidth "１.５" -> "1.5"). Non-digit chars (., e, +, -) pass through.
        let raw: String = chars[int_start..end]
            .iter()
            .map(|&c| match unicode_digit(c) {
                Some(d) => (b'0' + d) as char,
                None => c,
            })
            .collect();
        let val = parse_float_like(&raw).unwrap_or(0.0) * sign as f64;
        return Some((Chunk::Real(val), (end - start)));
    }

    // integer algorithm
    if !has_int_digits {
        // consumed a sign but no digits: not a number
        if consumed_sign == 1 {
            // back off; sign stays as text
        }
        return None;
    }
    // Build the integer from digit VALUES (not raw chars), so Unicode digits
    // like fullwidth ４ or Thai ๔ contribute correctly. Saturate on overflow.
    let mut val: i128 = 0;
    let mut overflow = false;
    for &c in &chars[int_start..i] {
        let d = unicode_digit(c).unwrap_or(0) as i128;
        match val.checked_mul(10).and_then(|v| v.checked_add(d)) {
            Some(v) => val = v,
            None => { overflow = true; break; }
        }
    }
    if overflow { val = i128::MAX; }
    Some((Chunk::Int(val * sign), (i - start)))
}

/// Parse a float the way Python's float() does for the subset natsort emits,
/// tolerating a trailing dot ("1." -> 1.0) and a leading dot (".5" -> 0.5),
/// which Rust's `f64::from_str` handles for ".5" but not for "1.".
fn parse_float_like(s: &str) -> Option<f64> {
    if let Ok(v) = s.parse::<f64>() {
        return Some(v);
    }
    // handle a trailing dot with optional exponent: "1." or "1.e133"
    // by inserting a 0 after the dot.
    if let Some(dot) = s.find('.') {
        let mut fixed = String::with_capacity(s.len() + 1);
        fixed.push_str(&s[..=dot]);
        // if next char is not a digit, insert one
        let rest = &s[dot + 1..];
        if rest.is_empty() || !rest.chars().next().unwrap().is_ascii_digit() {
            fixed.push('0');
        }
        fixed.push_str(rest);
        return fixed.parse::<f64>().ok();
    }
    None
}

/// Build the natsort key for a string under the given algorithm.
pub fn natsort_key(s: &str, alg: Ns) -> Vec<Chunk> {
    // natsort normalizes all input to NFD (canonical decomposition) before
    // parsing, so "é" becomes "e" + combining accent and case/letter handling
    // matches Python exactly. We do the same.
    let normalized: String = s.nfd().collect();
    let chars: Vec<char> = normalized.chars().collect();
    let n = chars.len();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut i = 0;
    let mut pending_text = String::new();
    // key always starts with a (possibly empty) text element
    let mut emitted_any = false;

    while i < n {
        // Under REAL/FLOAT, characters with a Unicode *numeric* value that are
        // not digits (Roman numerals Ⅷ, fractions ½, circled tens ⑩) are each an
        // isolated number with a possibly-fractional value. INT keeps them text.
        if alg.numbers_are_float() {
            if let Some(v) = unicode_numeric(chars[i]) {
                chunks.push(Chunk::Text(transform_text(&pending_text, alg)));
                pending_text.clear();
                chunks.push(Chunk::Real(v));
                i += 1;
                emitted_any = true;
                continue;
            }
        }
        // Isolated numeric characters (circled ①, superscript ², subscript ₃)
        // are each a single-digit number that never joins a run -- handle them
        // before the normal number matcher.
        if let Some(d) = unicode_isolated_digit(chars[i]) {
            chunks.push(Chunk::Text(transform_text(&pending_text, alg)));
            pending_text.clear();
            if alg.numbers_are_float() {
                chunks.push(Chunk::Real(d as f64));
            } else {
                chunks.push(Chunk::Int(d as i128));
            }
            i += 1;
            emitted_any = true;
            continue;
        }
        if let Some((num_chunk, consumed)) = match_number(&chars, i, alg) {
            // flush accumulated text (transformed), even if empty on first pos
            chunks.push(Chunk::Text(transform_text(&pending_text, alg)));
            pending_text.clear();
            chunks.push(num_chunk);
            i += consumed;
            emitted_any = true;
        } else {
            pending_text.push(chars[i]);
            i += 1;
        }
    }
    // trailing text (or the whole string if no number found)
    if !pending_text.is_empty() || !emitted_any {
        chunks.push(Chunk::Text(transform_text(&pending_text, alg)));
    }
    chunks
}

/// Compare two strings by their natural-sort keys under `alg`.
pub fn natcmp(a: &str, b: &str, alg: Ns) -> Ordering {
    compare_keys(&natsort_key(a, alg), &natsort_key(b, alg))
}

/// Sort naturally under the default algorithm.
pub fn natsorted(items: &[String]) -> Vec<String> {
    natsorted_alg(items, Ns::DEFAULT)
}

/// Sort naturally under a chosen algorithm.
///
/// Uses decorate-sort-undecorate: each item's key is computed ONCE up front
/// (O(n) key computations), then the sort compares precomputed keys directly.
/// The naive `sort_by(|a,b| natcmp(a,b,alg))` recomputes both keys on every
/// comparison -- O(n log n) redundant parsing -- which is what Python's
/// `sorted(seq, key=...)` avoids and what this port now matches.
pub fn natsorted_alg(items: &[String], alg: Ns) -> Vec<String> {
    // ns.PRESORT: sort lexicographically first (stable), so that ties in the
    // natural key (e.g. "a1" and "a01", both -> ("a", 1)) break by string
    // order rather than by original input order. Matches natsort exactly:
    // `if alg & ns.PRESORT: seq = sorted(seq, key=str)` before the real sort.
    let base: Vec<&String> = if alg.presort {
        let mut v: Vec<&String> = items.iter().collect();
        v.sort();
        v
    } else {
        items.iter().collect()
    };
    let mut decorated: Vec<(Vec<Chunk>, &String)> =
        base.into_iter().map(|s| (natsort_key(s, alg), s)).collect();
    decorated.sort_by(|a, b| compare_keys(&a.0, &b.0));
    decorated.into_iter().map(|(_, s)| s.clone()).collect()
}

/// Compare two already-computed keys (the decorate-sort-undecorate fast path).
fn compare_keys(ka: &[Chunk], kb: &[Chunk]) -> Ordering {
    for (x, y) in ka.iter().zip(kb.iter()) {
        let o = x.cmp_chunk(y);
        if o != Ordering::Equal {
            return o;
        }
    }
    ka.len().cmp(&kb.len())
}

/// `realsorted(seq)` — sort using the REAL (signed float) algorithm.
pub fn realsorted(items: &[String]) -> Vec<String> {
    natsorted_alg(items, Ns::real())
}

/// Indices that would sort `items` naturally (stable), default algorithm.
pub fn index_natsorted(items: &[String]) -> Vec<usize> {
    index_natsorted_alg(items, Ns::DEFAULT)
}

/// Indices that would sort `items` naturally under `alg` (stable).
pub fn index_natsorted_alg(items: &[String], alg: Ns) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..items.len()).collect();
    // Same PRESORT semantics as natsorted_alg, but operating on indices: a
    // preliminary stable sort by string value, THEN the stable natural sort,
    // so ties break by lexicographic order.
    if alg.presort {
        idx.sort_by(|&a, &b| items[a].cmp(&items[b]));
    }
    let keys: Vec<Vec<Chunk>> = items.iter().map(|s| natsort_key(s, alg)).collect();
    idx.sort_by(|&a, &b| compare_keys(&keys[a], &keys[b]));
    idx
}

/// `order_by_index(seq, index)` — reorder `items` by the given index list.
pub fn order_by_index(items: &[String], index: &[usize]) -> Vec<String> {
    index.iter().map(|&i| items[i].clone()).collect()
}


/// Return the ASCII numeric regex pattern for a given algorithm, matching the
/// core (non-Unicode) portion of Python natsort's `numeric_regex_chooser`.
///
/// Python's full pattern also includes a large alternation of Unicode numeric
/// characters (Roman numerals, circled digits, CJK numerals, …). This port
/// targets ASCII digits, which is the portion exercised by the differential
/// fuzzers and covers the overwhelming majority of real-world input; the
/// Unicode-numeral set is the documented scope boundary.
pub fn numeric_regex_chooser(alg: Ns) -> &'static str {
    // A float pattern is used whenever numbers are parsed as floats (REAL or
    // FLOAT); a leading sign is allowed when the algorithm is signed (REAL
    // implies signed). This collapses the signed-float cases into one branch.
    if alg.numbers_are_float() {
        if alg.sign_attaches() {
            r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?"
        } else {
            r"(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?"
        }
    } else if alg.signed {
        r"[-+]?\d+"
    } else {
        r"\d+"
    }
}

/// `humansorted(seq)` — like `natsorted`, but ordering text case-insensitively
/// in the way a person expects (`a A b B` rather than `A B a b`). Python uses
/// the locale here; this port implements the common, locale-independent
/// behavior: compare case-folded first, then break exact ties by the raw text
/// so that distinct strings remain distinct and stable.
pub fn humansorted(items: &[String]) -> Vec<String> {
    let mut out = items.to_vec();
    out.sort_by(|a, b| human_cmp(a, b));
    out
}

/// Indices that would sort `items` via `humansorted` (stable).
pub fn index_humansorted(items: &[String]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..items.len()).collect();
    idx.sort_by(|&a, &b| human_cmp(&items[a], &items[b]));
    idx
}

/// Indices that would sort `items` via `realsorted` (stable).
pub fn index_realsorted(items: &[String]) -> Vec<usize> {
    index_natsorted_alg(items, Ns::real())
}

fn human_cmp(a: &str, b: &str) -> Ordering {
    // case-insensitive natural comparison, tie-broken by the raw natural key
    let folded = Ns { ignorecase: true, ..Ns::DEFAULT };
    match natcmp(a, b, folded) {
        Ordering::Equal => natcmp(a, b, Ns::DEFAULT),
        other => other,
    }
}


/// Sort naturally with an optional reverse flag, mirroring `natsorted(seq,
/// reverse=...)`.
pub fn natsorted_reverse(items: &[String], alg: Ns, reverse: bool) -> Vec<String> {
    let mut out = natsorted_alg(items, alg);
    if reverse {
        out.reverse();
    }
    out
}

/// `os_sorted(seq)` — order strings the way a typical OS file manager does:
/// natural ordering, case-insensitive. (Python's version also consults the OS
/// locale and applies path-aware splitting; those are the documented scope
/// boundary, so this implements the common locale-independent behavior.)
pub fn os_sorted(items: &[String]) -> Vec<String> {
    os_sorted_presort(items, false)
}

/// `os_sorted(seq, presort=True)` — same as `os_sorted`, but pre-sorts
/// lexicographically first so ties break by string order, not input order.
pub fn os_sorted_presort(items: &[String], presort: bool) -> Vec<String> {
    let mut out = items.to_vec();
    if presort {
        out.sort();
    }
    out.sort_by(|a, b| human_cmp(a, b));
    out
}

/// Indices that would sort `items` via `os_sorted` (stable).
pub fn index_os_sorted(items: &[String]) -> Vec<usize> {
    index_os_sorted_presort(items, false)
}

/// Indices version of `os_sorted_presort`.
pub fn index_os_sorted_presort(items: &[String], presort: bool) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..items.len()).collect();
    if presort {
        idx.sort_by(|&a, &b| items[a].cmp(&items[b]));
    }
    idx.sort_by(|&a, &b| human_cmp(&items[a], &items[b]));
    idx
}

/// `chain_functions([f, g, h])` — compose string transforms left to right, so
/// the result applies `f`, then `g`, then `h`. Mirrors natsort's helper for
/// building a custom key from several transforms.
pub fn chain_functions<'a>(
    functions: Vec<Box<dyn Fn(String) -> String + 'a>>,
) -> impl Fn(String) -> String + 'a {
    move |input: String| {
        let mut value = input;
        for f in &functions {
            value = f(value);
        }
        value
    }
}


/// Decode a byte slice to an owned `String` using the given encoding label.
/// Mirrors natsort's `decoder(encoding)` for the encodings a Rust port can
/// support natively: "utf-8"/"utf8" decode as UTF-8 (lossy on invalid bytes,
/// matching Python's default error handling being replaced by a total
/// function), and "ascii" decodes ASCII, replacing non-ASCII bytes.
///
/// natsort's `decoder` returns a *function*; here the closure is returned the
/// same way so call sites read identically: `decoder("utf-8")(bytes)`.
pub fn decoder(encoding: &str) -> impl Fn(&[u8]) -> String + '_ {
    move |bytes: &[u8]| match encoding.to_ascii_lowercase().as_str() {
        "ascii" => bytes
            .iter()
            .map(|&b| if b.is_ascii() { b as char } else { '\u{FFFD}' })
            .collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// `as_ascii(bytes)` — decode a byte slice as ASCII. Non-bytes callers in
/// Python pass through unchanged; in Rust the input is already `&[u8]`.
pub fn as_ascii(bytes: &[u8]) -> String {
    decoder("ascii")(bytes)
}

/// `as_utf8(bytes)` — decode a byte slice as UTF-8 (lossy on invalid input).
pub fn as_utf8(bytes: &[u8]) -> String {
    decoder("utf-8")(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ns_int() -> Ns {
        Ns::DEFAULT
    }

    #[test]
    fn int_key_matches_python() {
        let k = natsort_key("num10", ns_int());
        assert_eq!(k, vec![Chunk::Text("num".into()), Chunk::Int(10)]);
        let k = natsort_key("1.5", ns_int());
        assert_eq!(
            k,
            vec![
                Chunk::Text("".into()),
                Chunk::Int(1),
                Chunk::Text(".".into()),
                Chunk::Int(5)
            ]
        );
    }

    #[test]
    fn int_sign_stays_as_text() {
        // default INT: "-5" -> ("-", 5)
        let k = natsort_key("-5", ns_int());
        assert_eq!(k, vec![Chunk::Text("-".into()), Chunk::Int(5)]);
    }

    #[test]
    fn signed_attaches_minus() {
        let alg = Ns { signed: true, ..Ns::DEFAULT };
        let k = natsort_key("a-5", alg);
        assert_eq!(k, vec![Chunk::Text("a".into()), Chunk::Int(-5)]);
    }

    #[test]
    fn real_parses_floats() {
        let k = natsort_key("1.5", Ns::real());
        assert_eq!(k, vec![Chunk::Text("".into()), Chunk::Real(1.5)]);
        let k = natsort_key("1e5", Ns::real());
        assert_eq!(k, vec![Chunk::Text("".into()), Chunk::Real(100000.0)]);
    }

    #[test]
    fn ignorecase_folds() {
        let alg = Ns { ignorecase: true, ..Ns::DEFAULT };
        let k = natsort_key("Num10", alg);
        assert_eq!(k, vec![Chunk::Text("num".into()), Chunk::Int(10)]);
    }

    #[test]
    fn groupletters_expands() {
        let alg = Ns { groupletters: true, ..Ns::DEFAULT };
        let k = natsort_key("Num", alg);
        // N->nN, u->uu, m->mm
        assert_eq!(k, vec![Chunk::Text("nNuumm".into())]);
    }

    #[test]
    fn default_sort_is_natural() {
        let input: Vec<String> =
            ["num3", "num5", "num2", "num10"].iter().map(|s| s.to_string()).collect();
        assert_eq!(natsorted(&input), vec!["num2", "num3", "num5", "num10"]);
    }

    #[test]
    fn realsorted_orders_signed_floats() {
        let input: Vec<String> =
            ["-1", "2", "-3", "1.5"].iter().map(|s| s.to_string()).collect();
        assert_eq!(realsorted(&input), vec!["-3", "-1", "1.5", "2"]);
    }

    #[test]
    fn real_exponent_after_trailing_dot() {
        // "1.e133" is 1e133 in Python natsort (int part + dot + exponent).
        let k = natsort_key("1.e133", Ns::real());
        assert_eq!(k, vec![Chunk::Text("".into()), Chunk::Real(1e133)]);
    }

    #[test]
    fn real_bare_e_is_text_not_exponent() {
        // "7.e" -> (7.0, "e"): a bare e with no exponent digits is text.
        let k = natsort_key("7.e", Ns::real());
        assert_eq!(
            k,
            vec![Chunk::Text("".into()), Chunk::Real(7.0), Chunk::Text("e".into())]
        );
    }

    #[test]
    fn presort_breaks_ties_lexicographically_not_by_input_order() {
        // "a1" and "a01" share the same natural key ("a", 1). Without
        // presort, a stable sort keeps input order for the tie. With
        // presort, ties break by plain string order instead.
        let items = vec!["a1".to_string(), "a01".to_string()];
        let alg = Ns { presort: true, ..Ns::DEFAULT };
        assert_eq!(natsorted_alg(&items, alg), vec!["a01", "a1"]);
        // Without presort, input order (a1 first) is preserved for the tie.
        assert_eq!(natsorted_alg(&items, Ns::DEFAULT), vec!["a1", "a01"]);
    }

    #[test]
    fn noexp_treats_e_as_literal_text() {
        // Under REAL, "1e5" is normally 100000.0. With NOEXP, the 'e' does
        // NOT start an exponent -- it's parsed as mantissa 1.0, text "e",
        // then a separate number 5.0, matching Python's ns.NOEXP.
        let alg = Ns { noexp: true, ..Ns::real() };
        let k = natsort_key("1e5", alg);
        assert_eq!(
            k,
            vec![Chunk::Text("".into()), Chunk::Real(1.0), Chunk::Text("e".into()), Chunk::Real(5.0)]
        );
    }

    #[test]
    fn decoder_utf8_and_ascii_match_python() {
        // Python: as_utf8(b"hello") == "hello"; as_ascii(b"abc") == "abc"
        assert_eq!(as_utf8(b"hello"), "hello");
        assert_eq!(as_ascii(b"abc"), "abc");
        assert_eq!(decoder("utf-8")(b"x"), "x");
    }

    #[test]
    fn as_utf8_is_lossy_on_invalid_bytes() {
        // Invalid UTF-8 becomes the replacement char rather than panicking.
        let out = as_utf8(&[0xff, 0xfe]);
        assert!(out.contains('\u{FFFD}'));
    }

    #[test]
    fn unicode_decimal_digits_parse_as_numbers() {
        // Fullwidth digits "１２３" parse as the number 123, matching Python
        // natsort (which recognizes all Unicode decimal / category-Nd digits).
        assert_eq!(natsort_key("１２３", Ns::DEFAULT), vec![Chunk::Text("".into()), Chunk::Int(123)]);
        // Thai + Arabic-Indic digits too.
        assert_eq!(natsort_key("๒5", Ns::DEFAULT), vec![Chunk::Text("".into()), Chunk::Int(25)]);
    }

    #[test]
    fn unicode_fullwidth_float_parses() {
        // "１.５" under REAL is the float 1.5.
        assert_eq!(natsort_key("１.５", Ns::real()), vec![Chunk::Text("".into()), Chunk::Real(1.5)]);
    }

    #[test]
    fn isolated_digit_chars_are_separate_numbers() {
        // Circled ①②③ are digit-valued but non-decimal, so each is a SEPARATE
        // single-digit number (never concatenated), matching natsort.
        assert_eq!(
            natsort_key("①②③", Ns::DEFAULT),
            vec![
                Chunk::Text("".into()), Chunk::Int(1),
                Chunk::Text("".into()), Chunk::Int(2),
                Chunk::Text("".into()), Chunk::Int(3),
            ]
        );
    }

    #[test]
    fn numeric_chars_are_numbers_under_real_only() {
        // Roman Ⅷ (=8) and fraction ½ (=0.5) have a numeric value but no digit
        // value: they are numbers under REAL, but text under INT.
        assert_eq!(natsort_key("Ⅷ", Ns::DEFAULT), vec![Chunk::Text("Ⅷ".into())]);
        assert_eq!(
            natsort_key("Ⅷ", Ns::real()),
            vec![Chunk::Text("".into()), Chunk::Real(8.0)]
        );
        assert_eq!(
            natsort_key("½", Ns::real()),
            vec![Chunk::Text("".into()), Chunk::Real(0.5)]
        );
    }

    #[test]
    fn natsort_key_public_example_matches_python() {
        // Python: natsort_key("a-5.034e2") == ("a-", 5, ".", 34, "e", 2)
        let k = natsort_key("a-5.034e2", Ns::DEFAULT);
        assert_eq!(k, vec![
            Chunk::Text("a-".into()), Chunk::Int(5),
            Chunk::Text(".".into()), Chunk::Int(34),
            Chunk::Text("e".into()), Chunk::Int(2),
        ]);
    }

    #[test]
    fn multi_number_strings_compare_componentwise() {
        // "a1b2" vs "a1b10": first number equal, text equal, second 2 < 10.
        assert_eq!(natcmp("a1b2", "a1b10", Ns::DEFAULT), Ordering::Less);
    }

    #[test]
    fn empty_string_sorts_first() {
        let input: Vec<String> = ["b", "", "a"].iter().map(|s| s.to_string()).collect();
        assert_eq!(natsorted(&input), vec!["", "a", "b"]);
    }

    #[test]
    fn pure_number_vs_pure_text() {
        // "1" -> ("",1); "a" -> ("a",). pos0 "" < "a", so number-led sorts first.
        assert_eq!(natcmp("1", "a", Ns::DEFAULT), Ordering::Less);
    }

    #[test]
    fn float_scientific_notation_orders_correctly() {
        let input: Vec<String> = ["1e2", "1e1", "1e3"].iter().map(|s| s.to_string()).collect();
        assert_eq!(realsorted(&input), vec!["1e1", "1e2", "1e3"]);
    }

    #[test]
    fn negative_floats_order_before_positive() {
        let input: Vec<String> = ["1.0", "-2.0", "-1.0", "2.0"].iter().map(|s| s.to_string()).collect();
        assert_eq!(realsorted(&input), vec!["-2.0", "-1.0", "1.0", "2.0"]);
    }

    #[test]
    fn lowercasefirst_swaps_case_in_ordering() {
        // Under LOWERCASEFIRST, "A" becomes "a" and vice versa for comparison.
        let alg = Ns { lowercasefirst: true, ..Ns::DEFAULT };
        let k = natsort_key("aBc", alg);
        assert_eq!(k, vec![Chunk::Text("AbC".into())]);
    }

    #[test]
    fn leading_zeros_preserved_in_value_not_width() {
        // "a007" and "a7" both parse to 7 -> equal.
        assert_eq!(natcmp("a007", "a7", Ns::DEFAULT), Ordering::Equal);
    }

    #[test]
    fn huge_number_does_not_panic() {
        let big = "9".repeat(50);
        let _ = natsort_key(&big, Ns::DEFAULT);
        let _ = natsort_key(&big, Ns::real());
    }

    #[test]
    fn signed_zero_ties_like_python() {
        // Python natsort: -0.0 == 0.0, so "-00" and "0" tie and keep input
        // order under a stable sort.
        assert_eq!(natcmp("-00", "0", Ns::real()), Ordering::Equal);
        assert_eq!(natcmp("0", "-0", Ns::real()), Ordering::Equal);
    }

    #[test]
    fn os_sorted_is_case_insensitive_natural() {
        let input: Vec<String> = ["file10.txt","file2.txt","file1.txt","File3.txt"]
            .iter().map(|s| s.to_string()).collect();
        let out = os_sorted(&input);
        assert_eq!(out, vec!["file1.txt","file2.txt","File3.txt","file10.txt"]);
    }

    #[test]
    fn reverse_sort_reverses_order() {
        let input: Vec<String> = ["a","c","b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(natsorted_reverse(&input, Ns::DEFAULT, true), vec!["c","b","a"]);
    }

    #[test]
    fn chain_functions_composes_left_to_right() {
        let fns: Vec<Box<dyn Fn(String) -> String>> = vec![
            Box::new(|s: String| s.trim().to_string()),
            Box::new(|s: String| s.to_uppercase()),
        ];
        let f = chain_functions(fns);
        assert_eq!(f("  hi ".to_string()), "HI");
    }

    #[test]
    fn order_by_index_roundtrips() {
        let items: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let idx = index_natsorted(&items);
        assert_eq!(order_by_index(&items, &idx), items);
    }
}

#[cfg(test)]
mod api_tests {
    use super::*;

    #[test]
    fn numeric_regex_chooser_ascii_patterns() {
        assert_eq!(numeric_regex_chooser(Ns::DEFAULT), r"\d+");
        assert_eq!(numeric_regex_chooser(Ns { signed: true, ..Ns::DEFAULT }), r"[-+]?\d+");
        assert_eq!(numeric_regex_chooser(Ns::real()), r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?");
    }

    #[test]
    fn humansorted_is_case_insensitive_order() {
        let input: Vec<String> = ["b","A","a","B"].iter().map(|s| s.to_string()).collect();
        // case-insensitive: a/A group before b/B group
        let out = humansorted(&input);
        assert_eq!(out[0].to_lowercase(), "a");
        assert_eq!(out[1].to_lowercase(), "a");
        assert_eq!(out[2].to_lowercase(), "b");
        assert_eq!(out[3].to_lowercase(), "b");
    }

    #[test]
    fn index_realsorted_matches_realsorted() {
        let input: Vec<String> = ["-1","2","-3","1.5"].iter().map(|s| s.to_string()).collect();
        let idx = index_realsorted(&input);
        let via_index = order_by_index(&input, &idx);
        assert_eq!(via_index, realsorted(&input));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // Sorting is idempotent: sorting an already-sorted list is a no-op.
        #[test]
        fn natsort_is_idempotent(items in prop::collection::vec("[a-zA-Z0-9.-]{0,8}", 0..20)) {
            let once = natsorted(&items);
            let twice = natsorted(&once);
            prop_assert_eq!(once, twice);
        }

        // Comparison is antisymmetric: cmp(a,b) is the reverse of cmp(b,a).
        #[test]
        fn natcmp_is_antisymmetric(a in "[a-zA-Z0-9.-]{0,10}", b in "[a-zA-Z0-9.-]{0,10}") {
            let ab = natcmp(&a, &b, Ns::DEFAULT);
            let ba = natcmp(&b, &a, Ns::DEFAULT);
            prop_assert_eq!(ab, ba.reverse());
        }

        // Comparison is reflexive: every string equals itself.
        #[test]
        fn natcmp_is_reflexive(a in "[a-zA-Z0-9.-]{0,12}") {
            prop_assert_eq!(natcmp(&a, &a, Ns::DEFAULT), Ordering::Equal);
        }

        // Sorting produces a permutation: same length, same multiset.
        #[test]
        fn natsort_is_a_permutation(items in prop::collection::vec("[a-zA-Z0-9.-]{0,6}", 0..15)) {
            let sorted = natsorted(&items);
            prop_assert_eq!(sorted.len(), items.len());
            let mut a = items.clone(); a.sort();
            let mut b = sorted.clone(); b.sort();
            prop_assert_eq!(a, b);
        }

        // Transitivity: if a<=b and b<=c then a<=c (spot-checked on triples).
        #[test]
        fn natcmp_is_transitive(
            a in "[a-z0-9]{0,6}", b in "[a-z0-9]{0,6}", c in "[a-z0-9]{0,6}"
        ) {
            let ab = natcmp(&a,&b,Ns::DEFAULT);
            let bc = natcmp(&b,&c,Ns::DEFAULT);
            if ab != Ordering::Greater && bc != Ordering::Greater {
                prop_assert_ne!(natcmp(&a,&c,Ns::DEFAULT), Ordering::Greater);
            }
        }

        // The key round-trips through comparison: sorting by natcmp agrees with
        // sorting the keys directly.
        #[test]
        fn real_sort_never_panics(items in prop::collection::vec("[-+0-9.eE]{0,8}", 0..12)) {
            let _ = natsorted_alg(&items, Ns::real());
        }
    }
}

// ---------------------------------------------------------------------------
// WebAssembly bindings (built only with `--features wasm`). These expose the
// sorter to the browser demo; they do not affect the native library or tests.
// ---------------------------------------------------------------------------
#[cfg(feature = "wasm")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    fn alg_from_str(alg: &str) -> Ns {
        match alg {
            "real" => Ns::real(),
            "signed" => Ns { signed: true, ..Ns::DEFAULT },
            "float" => Ns { float: true, ..Ns::DEFAULT },
            "ignorecase" => Ns { ignorecase: true, ..Ns::DEFAULT },
            "lowercasefirst" => Ns { lowercasefirst: true, ..Ns::DEFAULT },
            "groupletters" => Ns { groupletters: true, ..Ns::DEFAULT },
            _ => Ns::DEFAULT,
        }
    }

    /// Sort newline-separated input and return newline-separated output.
    #[wasm_bindgen]
    pub fn sort_lines(input: &str, alg: &str) -> String {
        let items: Vec<String> = input.lines().map(|s| s.to_string()).collect();
        natsorted_alg(&items, alg_from_str(alg)).join("\n")
    }

    /// Return the natsort key of a string as a human-readable tuple.
    #[wasm_bindgen]
    pub fn show_key(s: &str, alg: &str) -> String {
        let key = natsort_key(s, alg_from_str(alg));
        let parts: Vec<String> = key.iter().map(|c| match c {
            Chunk::Text(t) => format!("{t:?}"),
            Chunk::Int(n) => n.to_string(),
            Chunk::Real(f) => f.to_string(),
        }).collect();
        format!("({})", parts.join(", "))
    }
}
