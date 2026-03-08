use chumsky::Parser as _;
use lang::{
    eval::{eval, run_task, std_env, Value},
    parser::file_parser,
    types::{infer, std_type_env},
};

/// Run a `.coda` file: parse, type-check, eval (and run tasks). Panic on any error.
/// - `-- => VALUE`      asserts the result equals VALUE (runs task if needed).
/// - `-- !> TYPE ERROR` asserts a type error (no eval).
/// - `-- !> TASK FAIL`  asserts the task fails (no value check).
fn run_corpus(path: &str) {
    let src = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path, e));

    let expects_type_error = src.lines().any(|l| l.trim() == "-- !> TYPE ERROR");
    let expects_task_fail  = src.lines().any(|l| l.trim() == "-- !> TASK FAIL");

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

    // Run tasks to completion.
    let result = match value {
        Value::Task(_) => {
            let outcome = run_task(&value);
            if expects_task_fail {
                outcome.err().unwrap_or_else(|| panic!("expected task failure in {} but it succeeded", path));
                return;
            }
            outcome.unwrap_or_else(|e| panic!("task failed in {}: {}", path, e))
        }
        other => {
            if expects_task_fail {
                panic!("expected task failure in {} but got a non-task value", path);
            }
            other
        }
    };

    if let Some(expected) = src.lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("-- => "))
    {
        assert_eq!(result.to_string(), expected, "assertion failed in {}", path);
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
#[test] fn test_modules()           { run_corpus("corpus/modules.coda"); }
#[test] fn test_destructure()       { run_corpus("corpus/destructure.coda"); }
#[test] fn test_ann_wrong_arg()     { run_corpus("corpus/ann_wrong_arg.coda"); }

// task monad tests
#[test] fn test_task_ok()             { run_corpus("corpus/task_ok.coda"); }
#[test] fn test_task_then()           { run_corpus("corpus/task_then.coda"); }
#[test] fn test_task_bind()           { run_corpus("corpus/task_bind.coda"); }
#[test] fn test_task_bind_multi()     { run_corpus("corpus/task_bind_multi.coda"); }
#[test] fn test_task_fail()           { run_corpus("corpus/task_fail.coda"); }
#[test] fn test_task_fail_propagate() { run_corpus("corpus/task_fail_propagate.coda"); }
#[test] fn test_task_mixed_bind()     { run_corpus("corpus/task_mixed_bind.coda"); }
#[test] fn test_task_type_error()     { run_corpus("corpus/task_type_error.coda"); }
#[test] fn test_task_discard()           { run_corpus("corpus/task_discard.coda"); }
#[test] fn test_task_toplevel_bind()     { run_corpus("corpus/task_toplevel_bind.coda"); }
#[test] fn test_task_catch()             { run_corpus("corpus/task_catch.coda"); }
#[test] fn test_task_catch_inspect()     { run_corpus("corpus/task_catch_inspect.coda"); }
#[test] fn test_task_catch_passthrough() { run_corpus("corpus/task_catch_passthrough.coda"); }
#[test] fn test_subtract()            { run_corpus("corpus/subtract.coda"); }
#[test] fn test_fix()                 { run_corpus("corpus/fix.coda"); }
#[test] fn test_multiply()            { run_corpus("corpus/multiply.coda"); }
#[test] fn test_equality()            { run_corpus("corpus/equality.coda"); }

// list tests
#[test] fn test_list_literal()    { run_corpus("corpus/list_literal.coda"); }
#[test] fn test_list_empty()      { run_corpus("corpus/list_empty.coda"); }
#[test] fn test_list_cons()       { run_corpus("corpus/list_cons.coda"); }
#[test] fn test_list_head()       { run_corpus("corpus/list_head.coda"); }
#[test] fn test_list_map()        { run_corpus("corpus/list_map.coda"); }
#[test] fn test_list_fold()       { run_corpus("corpus/list_fold.coda"); }
#[test] fn test_list_append()     { run_corpus("corpus/list_append.coda"); }
#[test] fn test_list_ann()        { run_corpus("corpus/list_ann.coda"); }
#[test] fn test_list_type_error() { run_corpus("corpus/list_type_error.coda"); }
#[test] fn test_list_init()       { run_corpus("corpus/list_init.coda"); }
#[test] fn test_list_of()         { run_corpus("corpus/list_of.coda"); }

// tensor tests
#[test] fn test_tensors() { run_corpus("corpus/tensors.coda"); }
