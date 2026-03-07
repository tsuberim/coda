use chumsky::Parser as _;
use lang::{
    eval::{eval, std_env},
    parser::file_parser,
    types::{infer, std_type_env},
};

/// Run a `.coda` file: parse, type-check, eval. Panic on any error.
/// - `-- => VALUE`     asserts the evaluated result equals VALUE.
/// - `-- !> TYPE ERROR` asserts the file produces a type error (no eval).
fn run_corpus(path: &str) {
    let src = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path, e));

    let expects_type_error = src.lines()
        .any(|l| l.trim() == "-- !> TYPE ERROR");

    let ast = file_parser()
        .parse(src.as_str())
        .unwrap_or_else(|e| panic!("parse error in {}: {:?}", path, e));

    if expects_type_error {
        infer(&std_type_env(), &ast)
            .err()
            .unwrap_or_else(|| panic!("expected a type error in {} but inference succeeded", path));
        return;
    }

    infer(&std_type_env(), &ast)
        .unwrap_or_else(|e| panic!("type error in {}: {}", path, e));

    let value = eval(&ast, &std_env())
        .unwrap_or_else(|e| panic!("eval error in {}: {}", path, e));

    if let Some(expected) = src.lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("-- => "))
    {
        assert_eq!(
            value.to_string(), expected,
            "assertion failed in {}", path
        );
    }
}

#[test] fn test_arithmetic() { run_corpus("corpus/arithmetic.coda"); }
#[test] fn test_strings()    { run_corpus("corpus/strings.coda"); }
#[test] fn test_records()    { run_corpus("corpus/records.coda"); }
#[test] fn test_tags()       { run_corpus("corpus/tags.coda"); }
#[test] fn test_option()     { run_corpus("corpus/option.coda"); }
#[test] fn test_closures()   { run_corpus("corpus/closures.coda"); }

// type annotation tests
#[test] fn test_ann_forward_decl()  { run_corpus("corpus/ann_forward_decl.coda"); }
#[test] fn test_ann_constrain()     { run_corpus("corpus/ann_constrain.coda"); }
#[test] fn test_ann_record()        { run_corpus("corpus/ann_record.coda"); }
#[test] fn test_ann_tag()           { run_corpus("corpus/ann_tag.coda"); }
#[test] fn test_ann_conflict()      { run_corpus("corpus/ann_conflict.coda"); }
#[test] fn test_ann_bind_conflict() { run_corpus("corpus/ann_bind_conflict.coda"); }
#[test] fn test_ann_wrong_arg()     { run_corpus("corpus/ann_wrong_arg.coda"); }
