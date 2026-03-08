use chumsky::{text::TextParser, Parser};
use lang::ast::*;
use lang::parser::{expr_parser, file_parser};

fn d() -> Span { 0..0 }

/// Strip spans from a `Spanned<Expr>` recursively so tests can compare structure only.
/// All spans are replaced with `0..0` (dummy) so comparisons are span-agnostic.
fn strip(se: Spanned<Expr>) -> Spanned<Expr> {
    let e = match se.0 {
        Expr::Var(n) => Expr::Var(n),
        Expr::Lit(l) => Expr::Lit(l),
        Expr::Import(p) => Expr::Import(p),
        Expr::Lam(params, body) => Expr::Lam(params, Box::new(strip(*body))),
        Expr::App(f, args) => Expr::App(
            Box::new(strip(*f)),
            args.into_iter().map(strip).collect(),
        ),
        Expr::Block(items, body) => Expr::Block(
            items.into_iter().map(strip_item).collect(),
            Box::new(strip(*body)),
        ),
        Expr::Record(fields) => Expr::Record(
            fields.into_iter().map(|(k, v)| (k, strip(v))).collect(),
        ),
        Expr::Field(e, name) => Expr::Field(Box::new(strip(*e)), name),
        Expr::Tag(name, payload) => Expr::Tag(name, payload.map(|p| Box::new(strip(*p)))),
        Expr::When(scrut, branches, otherwise) => Expr::When(
            Box::new(strip(*scrut)),
            branches.into_iter().map(|(tag, binding, body)| (tag, binding, Box::new(strip(*body)))).collect(),
            otherwise.map(|o| Box::new(strip(*o))),
        ),
        Expr::List(elems) => Expr::List(elems.into_iter().map(strip).collect()),
    };
    (e, d())
}

fn strip_item(item: BlockItem) -> BlockItem {
    match item {
        BlockItem::Bind(name, e) => BlockItem::Bind(name, strip(e)),
        BlockItem::Ann(name, te) => BlockItem::Ann(name, te),
        BlockItem::MonadicBind(name, e) => BlockItem::MonadicBind(name, strip(e)),
    }
}

fn parse_expr(src: &str) -> Expr {
    let spanned = expr_parser()
        .padded()
        .then_ignore(chumsky::primitive::end())
        .parse(src)
        .unwrap_or_else(|errs| panic!("parse failed for {:?}: {:?}", src, errs));
    strip(spanned).0
}

fn parse_file(src: &str) -> Expr {
    let spanned = file_parser()
        .parse(src)
        .unwrap_or_else(|errs| panic!("parse failed for {:?}: {:?}", src, errs));
    strip(spanned).0
}

// Helper constructors producing span-free Expr values for comparison.
// Spans are 0..0 (dummy) since we strip them in parse_expr/parse_file.

fn var(s: &str) -> Expr { Expr::Var(s.into()) }
fn int(n: i64) -> Expr { Expr::Lit(Lit::Int(n)) }
fn str_(s: &str) -> Expr { Expr::Lit(Lit::Str(s.into())) }
fn app(f: Expr, args: Vec<Expr>) -> Expr {
    Expr::App(
        Box::new((f, d())),
        args.into_iter().map(|a| (a, d())).collect(),
    )
}
fn lam(params: Vec<&str>, body: Expr) -> Expr {
    Expr::Lam(params.iter().map(|s| s.to_string()).collect(), Box::new((body, d())))
}
fn block(bindings: Vec<(&str, Expr)>, body: Expr) -> Expr {
    use lang::ast::BlockItem;
    Expr::Block(
        bindings.into_iter().map(|(k, v)| BlockItem::Bind(k.into(), (v, d()))).collect(),
        Box::new((body, d())),
    )
}

// ── vars ──────────────────────────────────────────────────────────────────────

#[test]
fn test_var_alpha() {
    assert_eq!(parse_expr("foo"), var("foo"));
}

#[test]
fn test_var_underscore() {
    assert_eq!(parse_expr("_x"), var("_x"));
}

#[test]
fn test_var_sym() {
    assert_eq!(parse_expr("+"), var("+"));
}

#[test]
fn test_var_sym_complex() {
    assert_eq!(parse_expr(">>="), var(">>="));
}

// ── literals ─────────────────────────────────────────────────────────────────

#[test]
fn test_int() {
    assert_eq!(parse_expr("42"), int(42));
}

#[test]
fn test_string_plain() {
    assert_eq!(parse_expr("`hello`"), str_("hello"));
}

#[test]
fn test_string_empty() {
    assert_eq!(parse_expr("``"), str_(""));
}

// ── template strings ──────────────────────────────────────────────────────────

#[test]
fn test_template_interp() {
    // `hi {name}` → ++(Lit("hi "), name)
    assert_eq!(
        parse_expr("`hi {name}`"),
        app(var("++"), vec![str_("hi "), var("name")])
    );
}

#[test]
fn test_template_multi_interp() {
    // `{a} and {b}` → ++( ++(a, " and "), b )  — left-folded binary ++
    assert_eq!(
        parse_expr("`{a} and {b}`"),
        app(var("++"), vec![
            app(var("++"), vec![var("a"), str_(" and ")]),
            var("b"),
        ])
    );
}

#[test]
fn test_template_expr_interp() {
    // `val: {f(1)}` → ++(Lit("val: "), App(f, [1]))
    assert_eq!(
        parse_expr("`val: {f(1)}`"),
        app(var("++"), vec![str_("val: "), app(var("f"), vec![int(1)])])
    );
}

// ── lambda ────────────────────────────────────────────────────────────────────

#[test]
fn test_lam_single() {
    assert_eq!(parse_expr(r"\x -> x"), lam(vec!["x"], var("x")));
}

#[test]
fn test_lam_multi() {
    assert_eq!(
        parse_expr(r"\x y z -> x"),
        lam(vec!["x", "y", "z"], var("x"))
    );
}

#[test]
fn test_lam_body_app() {
    assert_eq!(
        parse_expr(r"\x -> f(x)"),
        lam(vec!["x"], app(var("f"), vec![var("x")]))
    );
}

// ── application ───────────────────────────────────────────────────────────────

#[test]
fn test_app_simple() {
    assert_eq!(parse_expr("f(x)"), app(var("f"), vec![var("x")]));
}

#[test]
fn test_app_multi_arg() {
    assert_eq!(
        parse_expr("f(x, y, z)"),
        app(var("f"), vec![var("x"), var("y"), var("z")])
    );
}

#[test]
fn test_app_no_args() {
    assert_eq!(parse_expr("f()"), app(var("f"), vec![]));
}

#[test]
fn test_app_chained() {
    // f(x)(y) → App(App(f,[x]),[y])
    assert_eq!(
        parse_expr("f(x)(y)"),
        app(app(var("f"), vec![var("x")]), vec![var("y")])
    );
}

#[test]
fn test_app_literal_fn() {
    assert_eq!(parse_expr("f(42)"), app(var("f"), vec![int(42)]));
}

// ── infix (desugars to App) ───────────────────────────────────────────────────

#[test]
fn test_infix_simple() {
    // a + b → App(Var("+"), [a, b])
    assert_eq!(
        parse_expr("a + b"),
        app(var("+"), vec![var("a"), var("b")])
    );
}

#[test]
fn test_infix_left_assoc() {
    // a + b + c → App("+", [App("+", [a, b]), c])
    assert_eq!(
        parse_expr("a + b + c"),
        app(var("+"), vec![app(var("+"), vec![var("a"), var("b")]), var("c")])
    );
}

#[test]
fn test_infix_custom_sym() {
    assert_eq!(
        parse_expr("5 @$ 6"),
        app(var("@$"), vec![int(5), int(6)])
    );
}

#[test]
fn test_infix_eq_eq() {
    // a == b → App(Var("=="), [a, b])
    assert_eq!(
        parse_expr("a == b"),
        app(var("=="), vec![var("a"), var("b")])
    );
}

// ── blocks ────────────────────────────────────────────────────────────────────

#[test]
fn test_block_paren_semicolon() {
    assert_eq!(
        parse_expr("(x = 1; x)"),
        block(vec![("x", int(1))], var("x"))
    );
}

#[test]
fn test_block_paren_multi() {
    assert_eq!(
        parse_expr("(x = 1; y = 2; x)"),
        block(vec![("x", int(1)), ("y", int(2))], var("x"))
    );
}

#[test]
fn test_block_no_bindings() {
    // (expr) is just grouping — no Block wrapper
    assert_eq!(parse_expr("(42)"), int(42));
}

#[test]
fn test_block_nested() {
    assert_eq!(
        parse_expr("(x = (y = 1; y); x)"),
        block(
            vec![("x", block(vec![("y", int(1))], var("y")))],
            var("x")
        )
    );
}

// ── file-level block ──────────────────────────────────────────────────────────

#[test]
fn test_file_single_expr() {
    assert_eq!(parse_file("42"), int(42));
}

#[test]
fn test_file_with_bindings() {
    let src = "x = 1\ny = 2\nx";
    assert_eq!(
        parse_file(src),
        block(vec![("x", int(1)), ("y", int(2))], var("x"))
    );
}

#[test]
fn test_file_semicolon_sep() {
    assert_eq!(
        parse_file("x = 1; x"),
        block(vec![("x", int(1))], var("x"))
    );
}

// ── combinations ──────────────────────────────────────────────────────────────

#[test]
fn test_lam_in_binding() {
    assert_eq!(
        parse_file("f = \\x -> x\nf"),
        block(vec![("f", lam(vec!["x"], var("x")))], var("f"))
    );
}

#[test]
fn test_infix_in_binding() {
    assert_eq!(
        parse_file("result = 1 + 2\nresult"),
        block(
            vec![("result", app(var("+"), vec![int(1), int(2)]))],
            var("result")
        )
    );
}

#[test]
fn test_app_of_lam() {
    assert_eq!(
        parse_expr(r"(\x -> x)(42)"),
        app(lam(vec!["x"], var("x")), vec![int(42)])
    );
}
