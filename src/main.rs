//! Thin CLI for the natsort port. All logic lives in lib.rs.
use natsort_core::{natcmp, natsorted_alg, Ns};
use std::io::{self, BufRead};
use std::process::ExitCode;

fn parse_alg(args: &[String]) -> Ns {
    let mut alg = Ns::DEFAULT;
    for a in args {
        match a.as_str() {
            "--real" => alg = Ns::real(),
            "--signed" => alg.signed = true,
            "--float" => alg.float = true,
            "--ignorecase" => alg.ignorecase = true,
            "--lowercasefirst" => alg.lowercasefirst = true,
            "--groupletters" => alg.groupletters = true,
            _ => {}
        }
    }
    alg
}


/// Format a comparison result as the CLI's numeric string ("-1"/"0"/"1").
fn compare_to_str(a: &str, b: &str, alg: Ns) -> String {
    match natcmp(a, b, alg) {
        std::cmp::Ordering::Less => "-1",
        std::cmp::Ordering::Equal => "0",
        std::cmp::Ordering::Greater => "1",
    }
    .to_string()
}

/// Format a natsort key as tab-separated typed tokens (T:text, I:int, R:real).
fn format_key(s: &str, alg: Ns) -> String {
    let key = natsort_core::natsort_key(s, alg);
    let toks: Vec<String> = key
        .iter()
        .map(|c| match c {
            natsort_core::Chunk::Text(t) => format!("T:{t}"),
            natsort_core::Chunk::Int(n) => format!("I:{n}"),
            natsort_core::Chunk::Real(f) => format!("R:{f}"),
        })
        .collect();
    toks.join("\t")
}

/// Parse a batch-sort flag spec (comma-separated) into an `Ns`.
fn alg_from_spec(spec: &str) -> Ns {
    let mut a = Ns::DEFAULT;
    for f in spec.split(',') {
        match f {
            "real" => a = Ns::real(),
            "signed" => a.signed = true,
            "float" => a.float = true,
            "ignorecase" => a.ignorecase = true,
            "lowercasefirst" => a.lowercasefirst = true,
            "groupletters" => a.groupletters = true,
            _ => {}
        }
    }
    a
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("compare") if args.len() >= 4 => {
            let alg = parse_alg(&args[4..]);
            println!("{}", compare_to_str(&args[2], &args[3], alg));
            ExitCode::SUCCESS
        }
        Some("key") => {
            // Output the natsort key for a string as tab-separated typed tokens:
            // T:text or I:int or R:float, so the adapter can rebuild the tuple.
            let alg = parse_alg(&args[3..]);
            let s = args.get(2).cloned().unwrap_or_default();
            println!("{}", format_key(&s, alg));
            ExitCode::SUCCESS
        }
        Some("os-sort") => {
            // os_sorted: case-insensitive natural order (a superset default).
            let stdin = io::stdin();
            let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
            for line in natsort_core::os_sorted(&lines) {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Some("bench") => {
            // Sort N generated filename-like strings, print elapsed microseconds
            // to stdout. Used by bench.py for throughput/latency measurement.
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10_000);
            let mut items: Vec<String> = Vec::with_capacity(n);
            for i in 0..n {
                items.push(format!("file{}-v{}.{}.log", i, i % 37, i % 5));
            }
            let start = std::time::Instant::now();
            let _sorted = natsort_core::natsorted(&items);
            let elapsed = start.elapsed();
            println!("{}", elapsed.as_micros());
            ExitCode::SUCCESS
        }
        Some("batch-sort") => {
            // Each stdin line: FLAGS\tITEM1\tITEM2\t...  -> sorted items tab-joined
            let stdin = io::stdin();
            for line in stdin.lock().lines().map_while(Result::ok) {
                let mut parts = line.split('\t');
                let spec = parts.next().unwrap_or("");
                let items: Vec<String> = parts.map(|s| s.to_string()).collect();
                let sorted = natsorted_alg(&items, alg_from_spec(spec));
                println!("{}", sorted.join("\t"));
            }
            ExitCode::SUCCESS
        }
        Some("sort") => {
            let alg = parse_alg(&args[2..]);
            let stdin = io::stdin();
            let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
            for line in natsorted_alg(&lines, alg) {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: natsort_port [compare A B [flags] | sort [flags] < lines]");
            eprintln!("flags: --real --signed --float --ignorecase --lowercasefirst --groupletters");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    fn a(flags: &[&str]) -> Vec<String> {
        flags.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_alg_reads_flags() {
        assert_eq!(parse_alg(&a(&["--real"])), Ns::real());
        assert!(parse_alg(&a(&["--ignorecase"])).ignorecase);
        assert!(parse_alg(&a(&["--groupletters"])).groupletters);
        assert_eq!(parse_alg(&a(&[])), Ns::DEFAULT);
    }

    #[test]
    fn compare_to_str_matches_cli_contract() {
        assert_eq!(compare_to_str("num2", "num10", Ns::DEFAULT), "-1");
        assert_eq!(compare_to_str("num10", "num2", Ns::DEFAULT), "1");
        assert_eq!(compare_to_str("a", "a", Ns::DEFAULT), "0");
    }

    #[test]
    fn format_key_emits_typed_tokens() {
        assert_eq!(format_key("num10", Ns::DEFAULT), "T:num\tI:10");
        assert_eq!(format_key("a-5.034e2", Ns::DEFAULT), "T:a-\tI:5\tT:.\tI:34\tT:e\tI:2");
    }

    #[test]
    fn format_key_real_emits_float_token() {
        assert_eq!(format_key("1.5", Ns::real()), "T:\tR:1.5");
    }

    #[test]
    fn alg_from_spec_parses_comma_flags() {
        assert_eq!(alg_from_spec("real"), Ns::real());
        assert!(alg_from_spec("ignorecase").ignorecase);
        assert_eq!(alg_from_spec("int"), Ns::DEFAULT);
        assert_eq!(alg_from_spec(""), Ns::DEFAULT);
    }
}
