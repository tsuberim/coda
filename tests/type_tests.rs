use chumsky::Parser as _;
use lang::{
    parser::file_parser,
    types::{infer, std_type_env, Type, TypeError},
};


fn infer_ok(src: &str) -> Type {
    let ast = file_parser()
        .parse(src)
        .unwrap_or_else(|e| panic!("parse error in {:?}: {:?}", src, e));
    infer(&std_type_env(), &ast)
        .unwrap_or_else(|e| panic!("type error in {:?}: {}", src, e))
}

fn infer_err(src: &str) -> TypeError {
    let ast = file_parser()
        .parse(src)
        .unwrap_or_else(|e| panic!("parse error in {:?}: {:?}", src, e));
    infer(&std_type_env(), &ast).expect_err("expected type error")
}

fn con(s: &str) -> Type { Type::Con(s.into(), vec![]) }
fn var(s: &str) -> Type { Type::Var(s.into()) }
fn fun(params: Vec<Type>, ret: Type) -> Type { Type::fun(params, ret) }
fn int() -> Type { con("Int") }
fn str_() -> Type { con("Str") }

// ── literals ──────────────────────────────────────────────────────────────────

#[test] fn test_int() { assert_eq!(infer_ok("42"),      int()); }
#[test] fn test_str() { assert_eq!(infer_ok("`hello`"), str_()); }

// ── arithmetic ────────────────────────────────────────────────────────────────

#[test]
fn test_add_int() { assert_eq!(infer_ok("1 + 2"), int()); }

// ── string concat ─────────────────────────────────────────────────────────────

#[test]
fn test_concat_str() { assert_eq!(infer_ok("`a` ++ `b`"), str_()); }

#[test]
fn test_concat_non_str_error() {
    assert!(matches!(infer_err("1 ++ 2"), TypeError::UnificationFail(..)));
}

#[test]
fn test_template_plain() { assert_eq!(infer_ok("`hello`"), str_()); }

#[test]
fn test_template_str_interp() {
    assert_eq!(infer_ok("(name = `world`; `hello {name}`)"), str_());
}

#[test]
fn test_template_int_interp_error() {
    // Interpolating an Int is a type error — ++ requires Str
    assert!(matches!(infer_err("`n = {42}`"), TypeError::UnificationFail(..)));
}

// ── lambda ────────────────────────────────────────────────────────────────────

#[test]
fn test_identity() {
    // \x -> x  :  a -> a
    assert_eq!(infer_ok(r"\x -> x"), fun(vec![var("a")], var("a")));
}

#[test]
fn test_const_fn() {
    // \x y -> x  :  a b -> a
    assert_eq!(
        infer_ok(r"\x y -> x"),
        fun(vec![var("a"), var("b")], var("a"))
    );
}

#[test]
fn test_add_fn() {
    // \x -> x + 1  :  Int -> Int
    assert_eq!(infer_ok(r"\x -> x + 1"), fun(vec![int()], int()));
}

#[test]
fn test_higher_order() {
    // \f x -> f(x)  :  (a -> b) a -> b
    assert_eq!(
        infer_ok(r"\f x -> f(x)"),
        fun(vec![fun(vec![var("a")], var("b")), var("a")], var("b"))
    );
}

#[test]
fn test_curried_add() {
    // \x -> \y -> x + y  :  Int -> Int Int -> Int
    assert_eq!(
        infer_ok(r"\x -> \y -> x + y"),
        fun(vec![int()], fun(vec![int()], int()))
    );
}

// ── application ───────────────────────────────────────────────────────────────

#[test]
fn test_apply_identity() {
    assert_eq!(infer_ok(r"(\x -> x)(42)"), int()); // identity applied
}

#[test]
fn test_apply_wrong_type() {
    // (\x -> x + 1)(`) — applying Int fn to Str
    assert!(matches!(infer_err(r"(\x -> x + 1)(`hello`)"), TypeError::UnificationFail(..)));
}

// ── block / let-polymorphism ──────────────────────────────────────────────────

#[test]
fn test_block_binding() {
    assert_eq!(infer_ok("(x = 1; x + 1)"), int());
}

#[test]
fn test_let_polymorphism() {
    // id is used at two different types in the same block
    assert_eq!(
        infer_ok(r"(id = \x -> x; id(1) + id(2))"),
        int()
    );
}

#[test]
fn test_nested_block() {
    assert_eq!(infer_ok("(x = 1; y = x + 1; y)"), int());
}

// ── unbound variable ──────────────────────────────────────────────────────────

#[test]
fn test_unbound_var() {
    assert!(matches!(infer_err("x"), TypeError::UnboundVar(..)));
}

// ── tags & unions ─────────────────────────────────────────────────────────────

fn union(tags: Vec<(&str, Type)>, row: Option<&str>) -> Type {
    Type::Union(tags.into_iter().map(|(k, v)| (k.into(), v)).collect(), row.map(Into::into))
}

#[test]
fn test_tag_with_payload() {
    // Some 42  :  [Some Int | *]
    assert_eq!(infer_ok("Some 42"), union(vec![("Some", int())], Some("a")));
}

#[test]
fn test_tag_no_payload() {
    // None  :  [None | *]  (unit payload elided)
    assert_eq!(infer_ok("None"), union(vec![("None", Type::unit())], Some("a")));
}

#[test]
fn test_when_closed() {
    // \x -> when x is; Some n -> n + 1; None -> 0  :  [Some Int, None] -> Int
    assert_eq!(
        infer_ok(r"\x -> when x is; Some n -> n + 1; None -> 0"),
        fun(
            vec![union(vec![("None", Type::unit()), ("Some", int())], None)],
            int()
        )
    );
}

#[test]
fn test_when_open_otherwise() {
    // \x -> when x is; Some n -> n; otherwise 0  :  [Some Int | *] -> Int
    assert_eq!(
        infer_ok(r"\x -> when x is; Some n -> n; otherwise 0"),
        fun(vec![union(vec![("Some", int())], Some("a"))], int())
    );
}

#[test]
fn test_when_apply_open_to_closed() {
    // (\x -> when x is; Some n -> n + 1; None -> 0)(Some 5)
    // Passing [Some Int | *] to fn expecting [Some Int, None] — row closes.
    assert_eq!(
        infer_ok(r"(\x -> when x is; Some n -> n + 1; None -> 0)(Some 5)"),
        int()
    );
}
