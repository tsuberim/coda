use chumsky::Parser as _;
use lang::{codegen, parser::file_parser, types::{infer, std_type_env}};
use std::process::Command;

/// Compile a `.coda` file to a native binary via LLVM IR, run it, and check output.
/// Files with `-- !> TYPE ERROR` are skipped (type errors, no binary produced).
/// Files with no `-- => VALUE` annotation are compiled and run (no output check).
fn run_compiled(path: &str) {
    let src = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path, e));

    if src.lines().any(|l| l.trim() == "-- !> TYPE ERROR") {
        return; // type-error tests: no binary to compile
    }
    if src.lines().any(|l| l.trim() == "-- !> TASK FAIL") {
        return; // task-fail tests: require runtime task support
    }

    let ast = file_parser()
        .parse(src.as_str())
        .unwrap_or_else(|e| panic!("parse error in {}: {:?}", path, e));

    infer(&std_type_env(), &ast)
        .unwrap_or_else(|e| panic!("type error in {}: {}", path, e));

    let ir = codegen::compile(&ast)
        .unwrap_or_else(|e| panic!("codegen error in {}: {}", path, e));

    // Write IR to a temp file.
    let stem = std::path::Path::new(path).file_stem().unwrap().to_str().unwrap();
    let ir_path = format!("/tmp/coda_compile_test_{}.ll", stem);
    let bin_path = format!("/tmp/coda_compile_test_{}", stem);
    std::fs::write(&ir_path, &ir)
        .unwrap_or_else(|e| panic!("failed to write IR for {}: {}", path, e));

    let runtime_c = concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c");
    let status = Command::new("clang")
        .args([&ir_path, runtime_c, "-o", &bin_path, "-O1"])
        .output()
        .unwrap_or_else(|e| panic!("clang not found for {}: {}", path, e));

    if !status.status.success() {
        panic!(
            "clang failed for {}:\n{}",
            path,
            String::from_utf8_lossy(&status.stderr)
        );
    }

    let run = Command::new(&bin_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to run binary for {}: {}", path, e));

    assert!(run.status.success(), "binary crashed for {}", path);

    let got = String::from_utf8_lossy(&run.stdout);
    let got = got.trim_end_matches('\n');

    if let Some(expected) = src
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("-- => "))
    {
        assert_eq!(got, expected, "output mismatch in {}", path);
    }
}

#[test] fn compiled_arithmetic()  { run_compiled("corpus/arithmetic.coda"); }
#[test] fn compiled_strings()     { run_compiled("corpus/strings.coda"); }
#[test] fn compiled_records()     { run_compiled("corpus/records.coda"); }
#[test] fn compiled_tags()        { run_compiled("corpus/tags.coda"); }
#[test] fn compiled_option()      { run_compiled("corpus/option.coda"); }
#[test] fn compiled_closures()    { run_compiled("corpus/closures.coda"); }
#[test] fn compiled_fix()         { run_compiled("corpus/fix.coda"); }
#[test] fn compiled_multiply()    { run_compiled("corpus/multiply.coda"); }
#[test] fn compiled_subtract()    { run_compiled("corpus/subtract.coda"); }
#[test] fn compiled_equality()    { run_compiled("corpus/equality.coda"); }
#[test] fn compiled_destructure() { run_compiled("corpus/destructure.coda"); }

#[test] fn compiled_list_literal() { run_compiled("corpus/list_literal.coda"); }
#[test] fn compiled_list_empty()   { run_compiled("corpus/list_empty.coda"); }
#[test] fn compiled_list_cons()    { run_compiled("corpus/list_cons.coda"); }
#[test] fn compiled_list_head()    { run_compiled("corpus/list_head.coda"); }
#[test] fn compiled_list_map()     { run_compiled("corpus/list_map.coda"); }
#[test] fn compiled_list_fold()    { run_compiled("corpus/list_fold.coda"); }
#[test] fn compiled_list_append()  { run_compiled("corpus/list_append.coda"); }
#[test] fn compiled_list_init()    { run_compiled("corpus/list_init.coda"); }
#[test] fn compiled_list_of()      { run_compiled("corpus/list_of.coda"); }
