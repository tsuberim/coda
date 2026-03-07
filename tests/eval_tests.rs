use chumsky::Parser as _;
use lang::{
    eval::{eval, std_env, Value},
    parser::file_parser,
};

fn run(src: &str) -> Value {
    let ast = file_parser()
        .parse(src)
        .unwrap_or_else(|e| panic!("parse error in {:?}: {:?}", src, e));
    eval(&ast, &std_env()).unwrap_or_else(|e| panic!("eval error in {:?}: {:?}", src, e))
}

fn int(n: i64) -> Value { Value::Int(n) }
fn float(f: f64) -> Value { Value::Float(f) }
fn str_(s: &str) -> Value { Value::Str(s.into()) }

// ── literals ──────────────────────────────────────────────────────────────────

#[test]
fn test_int_lit() { assert_eq!(run("42"), int(42)); }

#[test]
fn test_float_lit() { assert_eq!(run("3.14"), float(3.14)); }

#[test]
fn test_str_lit() { assert_eq!(run("`hello`"), str_("hello")); }

#[test]
fn test_empty_str() { assert_eq!(run("``"), str_("")); }

// ── addition ──────────────────────────────────────────────────────────────────

#[test]
fn test_add_int() { assert_eq!(run("1 + 2"), int(3)); }

#[test]
fn test_add_float() { assert_eq!(run("1.5 + 2.5"), float(4.0)); }

#[test]
fn test_add_left_assoc() { assert_eq!(run("1 + 2 + 3"), int(6)); }

#[test]
fn test_add_nested() { assert_eq!(run("(1 + 2) + (3 + 4)"), int(10)); }

// ── string concat ─────────────────────────────────────────────────────────────

#[test]
fn test_str_concat() {
    assert_eq!(run("`hello` ++ ` world`"), str_("hello world"));
}

#[test]
fn test_template_plain() {
    assert_eq!(run("`hello`"), str_("hello"));
}

#[test]
fn test_template_interp() {
    assert_eq!(run("(name = `world`; `hello {name}`)"), str_("hello world"));
}

#[test]
fn test_template_int_interp() {
    assert_eq!(run("`n = {42}`"), str_("n = 42"));
}

#[test]
fn test_template_expr_interp() {
    assert_eq!(run("`sum = {1 + 2}`"), str_("sum = 3"));
}

// ── lambda & application ──────────────────────────────────────────────────────

#[test]
fn test_identity() { assert_eq!(run(r"(\x -> x)(42)"), int(42)); }

#[test]
fn test_multi_param() { assert_eq!(run(r"(\x y -> x + y)(3, 4)"), int(7)); }

#[test]
fn test_apply_builtin_as_value() {
    assert_eq!(run(r"(\f -> f(1, 2))(+)"), int(3));
}

// ── block scoping ─────────────────────────────────────────────────────────────

#[test]
fn test_block_single() { assert_eq!(run("(x = 1; x)"), int(1)); }

#[test]
fn test_block_multi() { assert_eq!(run("(x = 1; y = x + 1; y)"), int(2)); }

#[test]
fn test_block_shadowing() { assert_eq!(run("(x = 1; x = x + 1; x)"), int(2)); }

#[test]
fn test_nested_block() {
    assert_eq!(run("(x = 1; y = (z = x + 1; z + 1); y)"), int(3));
}

// ── closures ──────────────────────────────────────────────────────────────────

#[test]
fn test_closure_captures() {
    assert_eq!(run(r"(x = 10; f = \y -> x + y; f(5))"), int(15));
}

#[test]
fn test_currying() {
    assert_eq!(run(r"(add = \x -> \y -> x + y; add(3)(4))"), int(7));
}

#[test]
fn test_higher_order() {
    assert_eq!(run(r"(apply = \f x -> f(x); apply(\x -> x + 1, 5))"), int(6));
}

// ── file-level block ──────────────────────────────────────────────────────────

#[test]
fn test_file_multiline() {
    assert_eq!(run("x = 10\ny = 20\nx + y"), int(30));
}

#[test]
fn test_file_with_fn() {
    assert_eq!(run("double = \\x -> x + x\ndouble(21)"), int(42));
}
