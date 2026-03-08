/// Corpus benchmark: compile + run each eligible .coda file N times.
/// Uses libc::wait4 for sub-millisecond wall time and rusage (CPU + peak RSS).
///
/// Flags:
///   --json <path>    Write results JSON to path (in addition to table output)
///   --compare <path> Compare against baseline JSON; exit 1 on regression
use chumsky::Parser as _;
use lang::{codegen, parser::file_parser, types::{infer, std_type_env, Type}};
use serde::{Deserialize, Serialize};
use std::{process::Command, time::Instant};

const RUNS: u32 = 20;
const RUNTIME_C: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c");
/// Regression threshold: flag if avg increases by more than this fraction AND more than MIN_ABS_MS.
const REGRESS_FRAC: f64 = 0.20;
const REGRESS_ABS_MS: f64 = 5.0;

// ── per-run sample ───────────────────────────────────────────────────────────

struct Sample {
    wall_ms: f64,
    cpu_ms:  f64,
    rss_kb:  f64,
}

fn run_once(bin_path: &str) -> Sample {
    let child = Command::new(bin_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {}", bin_path, e));

    let t0  = Instant::now();
    let pid = child.id() as libc::pid_t;

    let mut status: libc::c_int = 0;
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::wait4(pid, &mut status, 0, &mut usage); }

    let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let cpu_ms  = (usage.ru_utime.tv_sec  as f64 * 1_000_000.0 + usage.ru_utime.tv_usec as f64
                 + usage.ru_stime.tv_sec  as f64 * 1_000_000.0 + usage.ru_stime.tv_usec as f64)
                 / 1000.0;
    #[cfg(target_os = "macos")]
    let rss_kb = usage.ru_maxrss as f64 / 1024.0;
    #[cfg(not(target_os = "macos"))]
    let rss_kb = usage.ru_maxrss as f64; // Linux: already KB

    assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        "binary {} exited with status {}", bin_path, status);

    Sample { wall_ms, cpu_ms, rss_kb }
}

// ── stats ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
struct Stats { min: f64, avg: f64, max: f64, total: f64 }

fn stats(vals: &[f64]) -> Stats {
    let min   = vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let max   = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let total = vals.iter().sum::<f64>();
    Stats { min, avg: total / vals.len() as f64, max, total }
}

// ── JSON types ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct FileResult {
    name:       String,
    compile_ms: f64,
    wall_ms:    Stats,
    cpu_ms:     Stats,
    rss_kb:     Stats,
}

#[derive(Serialize, Deserialize)]
struct BenchReport {
    timestamp: String,
    runs:      u32,
    results:   Vec<FileResult>,
}

// ── compilation ──────────────────────────────────────────────────────────────

struct Compiled { name: String, bin_path: String, compile_ms: f64 }

fn try_compile(path: &str) -> Option<Compiled> {
    let src = std::fs::read_to_string(path).ok()?;
    if src.lines().any(|l| {
        let t = l.trim();
        t == "-- !> TYPE ERROR" || t == "-- !> TASK FAIL"
    }) { return None; }

    let ast = file_parser().parse(src.as_str()).ok()?;
    let t0  = Instant::now();
    let ty  = infer(&std_type_env(), &ast).ok()?;
    let is_task = matches!(&ty, Type::Con(n, _) if n == "Task");
    let ir  = codegen::compile(&ast, is_task).ok()?;
    let codegen_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let stem     = std::path::Path::new(path).file_stem()?.to_str()?;
    let ir_path  = format!("/tmp/coda_bench_{}.ll", stem);
    let bin_path = format!("/tmp/coda_bench_{}", stem);
    std::fs::write(&ir_path, &ir).ok()?;

    let t1    = Instant::now();
    let clang = Command::new("clang")
        .args([ir_path.as_str(), RUNTIME_C, "-o", &bin_path, "-O1"])
        .output().ok()?;
    let clang_ms = t1.elapsed().as_secs_f64() * 1000.0;
    if !clang.status.success() { return None; }

    Some(Compiled { name: stem.to_string(), bin_path, compile_ms: codegen_ms + clang_ms })
}

// ── formatting ───────────────────────────────────────────────────────────────

fn fmt_ms(v: f64) -> String { format!("{:7.2}", v) }
fn fmt_kb(v: f64) -> String { format!("{:7.0}", v) }

fn print_header() {
    println!(
        "{:<24} {:>8}  {:^25}  {:^25}  {:^23}  {:>10}",
        "file", "cmp(ms)", "wall ms (min/avg/max)", "cpu ms (min/avg/max)",
        "rss KB (min/avg/max)", "total(ms)",
    );
    println!("{}", "-".repeat(122));
}

fn print_row(r: &FileResult) {
    println!(
        "{:<24} {:>8.2}  {}/{}/{}  {}/{}/{}  {}/{}/{}  {:>10.2}",
        r.name, r.compile_ms,
        fmt_ms(r.wall_ms.min), fmt_ms(r.wall_ms.avg), fmt_ms(r.wall_ms.max),
        fmt_ms(r.cpu_ms.min),  fmt_ms(r.cpu_ms.avg),  fmt_ms(r.cpu_ms.max),
        fmt_kb(r.rss_kb.min),  fmt_kb(r.rss_kb.avg),  fmt_kb(r.rss_kb.max),
        r.wall_ms.total,
    );
}

// ── regression check ─────────────────────────────────────────────────────────

fn check_regressions(current: &BenchReport, baseline_path: &str) -> bool {
    let Ok(data) = std::fs::read_to_string(baseline_path) else {
        println!("No baseline at {baseline_path} — skipping comparison.");
        return true;
    };
    let Ok(baseline) = serde_json::from_str::<BenchReport>(&data) else {
        eprintln!("Failed to parse baseline JSON at {baseline_path}");
        return false;
    };

    let mut ok = true;
    println!("\nRegression check vs baseline ({}):", baseline_path);
    for cur in &current.results {
        let Some(base) = baseline.results.iter().find(|r| r.name == cur.name) else { continue; };
        let check = |label: &str, cur_avg: f64, base_avg: f64| -> bool {
            if base_avg < 0.001 { return true; } // skip sub-microsecond baselines
            let delta = cur_avg - base_avg;
            let frac  = delta / base_avg;
            if frac > REGRESS_FRAC && delta > REGRESS_ABS_MS {
                println!("  REGRESS  {:<24} {}: {:.2} → {:.2} ms  (+{:.0}%)",
                    cur.name, label, base_avg, cur_avg, frac * 100.0);
                false
            } else {
                true
            }
        };
        if !check("wall_avg", cur.wall_ms.avg, base.wall_ms.avg) { ok = false; }
        if !check("cpu_avg",  cur.cpu_ms.avg,  base.cpu_ms.avg)  { ok = false; }
    }
    if ok { println!("  All clear."); }
    ok
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get_flag = |flag: &str| -> Option<String> {
        args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
    };
    let json_out  = get_flag("--json");
    let compare   = get_flag("--compare");

    let corpus_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/corpus");
    let mut paths: Vec<_> = std::fs::read_dir(corpus_dir)
        .expect("cannot read corpus/")
        .filter_map(|e| {
            let e = e.ok()?;
            let p = e.path();
            if p.extension()? == "coda" { Some(p) } else { None }
        })
        .collect();
    paths.sort();

    let mut compiled: Vec<Compiled> = paths.iter()
        .filter_map(|p| try_compile(p.to_str()?))
        .collect();
    compiled.sort_by(|a, b| a.name.cmp(&b.name));

    if compiled.is_empty() {
        eprintln!("No corpus files compiled successfully.");
        std::process::exit(1);
    }

    println!("\nCorpus benchmark — {} runs per file\n", RUNS);
    print_header();

    let mut report = BenchReport {
        timestamp: chrono_now(),
        runs: RUNS,
        results: vec![],
    };

    let mut grand_wall = vec![];
    let mut grand_cpu  = vec![];
    let mut grand_rss  = vec![];

    for c in &compiled {
        let samples: Vec<Sample> = (0..RUNS).map(|_| run_once(&c.bin_path)).collect();
        let result = FileResult {
            name:       c.name.clone(),
            compile_ms: c.compile_ms,
            wall_ms:    stats(&samples.iter().map(|s| s.wall_ms).collect::<Vec<_>>()),
            cpu_ms:     stats(&samples.iter().map(|s| s.cpu_ms).collect::<Vec<_>>()),
            rss_kb:     stats(&samples.iter().map(|s| s.rss_kb).collect::<Vec<_>>()),
        };
        grand_wall.extend(samples.iter().map(|s| s.wall_ms));
        grand_cpu.extend(samples.iter().map(|s| s.cpu_ms));
        grand_rss.extend(samples.iter().map(|s| s.rss_kb));
        print_row(&result);
        report.results.push(result);
    }

    let grand_total = grand_wall.iter().sum::<f64>();
    let summary = FileResult {
        name:       "TOTAL / GRAND AVG".to_string(),
        compile_ms: 0.0,
        wall_ms:    stats(&grand_wall),
        cpu_ms:     stats(&grand_cpu),
        rss_kb:     stats(&grand_rss),
    };
    println!("{}", "-".repeat(122));
    print_row(&summary);
    println!("\n  {} files  ·  {} runs each  ·  grand total wall {:.2} ms",
        compiled.len(), RUNS, grand_total);

    if let Some(path) = &json_out {
        let json = serde_json::to_string_pretty(&report).expect("serialize");
        std::fs::write(path, json).unwrap_or_else(|e| eprintln!("failed to write JSON: {e}"));
        println!("\nResults written to {path}");
    }

    if let Some(path) = &compare {
        if !check_regressions(&report, path) {
            std::process::exit(1);
        }
    }
}

fn chrono_now() -> String {
    // Simple RFC 3339-ish timestamp without pulling in chrono.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as seconds since epoch — CI can replace with a real timestamp if desired.
    format!("{secs}")
}
