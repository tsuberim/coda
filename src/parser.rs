use chumsky::prelude::*;

use crate::ast::*;

const SYM_CHARS: &str = "!@#$%^&*-+=|<>?/~.:";

fn hws() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    filter(|c: &char| *c == ' ' || *c == '\t')
        .repeated()
        .ignored()
}

fn ws1() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    filter(|c: &char| c.is_whitespace())
        .repeated()
        .at_least(1)
        .ignored()
}

fn sep() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    hws()
        .then(just(';').or(just('\n')))
        .then(filter(|c: &char| c.is_whitespace()).repeated())
        .ignored()
}

/// Matches exactly `=` (not `==`, `=>`, etc.) by greedily consuming the full
/// symbolic token and checking it's a single `=`.
fn assign() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    filter(|c: &char| SYM_CHARS.contains(*c))
        .repeated()
        .at_least(1)
        .collect::<String>()
        .try_map(|s, span| {
            if s == "=" {
                Ok(())
            } else {
                Err(Simple::custom(span, "expected `=`"))
            }
        })
}

fn sym_name() -> impl Parser<char, String, Error = Simple<char>> + Clone {
    filter(|c: &char| SYM_CHARS.contains(*c))
        .repeated()
        .at_least(1)
        .collect::<String>()
        .try_map(|s, span| {
            if s == "->" || s == "=" {
                Err(Simple::custom(span, format!("'{}' is reserved", s)))
            } else {
                Ok(s)
            }
        })
}

pub fn expr_parser() -> impl Parser<char, Expr, Error = Simple<char>> {
    recursive(|expr: Recursive<char, Expr, Simple<char>>| {
        let ident = filter(|c: &char| c.is_alphabetic() || *c == '_')
            .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
            .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>());

        let digits = text::digits::<char, Simple<char>>(10);

        let float_lit = digits
            .clone()
            .then_ignore(just('.'))
            .then(digits.clone())
            .map(|(i, f): (String, String)| {
                Expr::Lit(Lit::Float(format!("{}.{}", i, f).parse().unwrap()))
            });

        let int_lit = digits
            .clone()
            .map(|s: String| Expr::Lit(Lit::Int(s.parse().unwrap())));

        // Template string: `...{expr}...`
        // Desugars to ++(part, ...) or a plain Lit::Str for no interpolations.
        let tmpl_raw = filter(|c: &char| *c != '`' && *c != '{')
            .repeated()
            .at_least(1)
            .collect::<String>()
            .map(|s| Expr::Lit(Lit::Str(s)));

        let tmpl_interp = expr
            .clone()
            .padded()
            .delimited_by(just('{'), just('}'));

        let template = tmpl_raw
            .or(tmpl_interp)
            .repeated()
            .delimited_by(just('`'), just('`'))
            .map(|parts| match parts.len() {
                0 => Expr::Lit(Lit::Str(String::new())),
                1 => parts.into_iter().next().unwrap(),
                _ => Expr::App(Box::new(Expr::Var("++".into())), parts),
            });

        // Lambda: \x y z -> expr
        let lambda = just('\\')
            .ignore_then(
                ident
                    .clone()
                    .padded_by(hws())
                    .repeated()
                    .at_least(1)
                    .then_ignore(just("->")),
            )
            .then(expr.clone())
            .map(|(params, body)| Expr::Lam(params, Box::new(body)));

        // Block body: (name = expr ;)* expr
        // Empty bindings → just the body expression (grouping).
        let binding = ident
            .clone()
            .then_ignore(hws())
            .then_ignore(assign())
            .then_ignore(hws())
            .then(expr.clone());

        let block_body = binding
            .clone()
            .then_ignore(sep())
            .repeated()
            .then(expr.clone())
            .map(|(bindings, body)| {
                if bindings.is_empty() {
                    body
                } else {
                    Expr::Block(bindings, Box::new(body))
                }
            });

        let paren_block = block_body.padded().delimited_by(just('('), just(')'));

        // Atom: strips LEADING whitespace only (not trailing).
        // Trailing whitespace must remain for the infix `ws1` check.
        let leading_ws = filter(|c: &char| c.is_whitespace()).repeated().ignored();

        let atom = leading_ws.ignore_then(choice((
            float_lit,
            int_lit,
            template,
            lambda,
            paren_block,
            ident.clone().map(Expr::Var),
            sym_name().map(Expr::Var),
        )));

        // Application: atom(arg, ...)* — no whitespace allowed before `(`
        let args = expr
            .clone()
            .padded()
            .separated_by(just(','))
            .allow_trailing()
            .delimited_by(just('('), just(')'));

        let app = atom
            .then(args.repeated())
            .map(|(f, arg_lists)| {
                arg_lists
                    .into_iter()
                    .fold(f, |f, args| Expr::App(Box::new(f), args))
            });

        // Infix: app (ws1 sym ws1 app)* — desugars to App(Var(op), [lhs, rhs])
        // ws1 before sym is required; if sym_name fails, repeated() backtracks ws1.
        app.clone()
            .then(
                ws1()
                    .ignore_then(sym_name())
                    .then_ignore(ws1())
                    .then(app.clone())
                    .repeated(),
            )
            .map(|(first, rest)| {
                rest.into_iter().fold(first, |lhs, (op, rhs)| {
                    Expr::App(Box::new(Expr::Var(op)), vec![lhs, rhs])
                })
            })
    })
}

/// A single REPL input: either a binding (`name = expr`) or a bare expression.
pub enum ReplInput {
    Binding(String, Expr),
    Expr(Expr),
}

pub fn repl_parser() -> impl Parser<char, ReplInput, Error = Simple<char>> {
    let ident = filter(|c: &char| c.is_alphabetic() || *c == '_')
        .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
        .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>());

    let binding = ident
        .then_ignore(hws())
        .then_ignore(assign())
        .then_ignore(hws())
        .then(expr_parser())
        .map(|(name, expr)| ReplInput::Binding(name, expr));

    binding
        .or(expr_parser().map(ReplInput::Expr))
        .padded()
        .then_ignore(end())
}

/// Parse a complete file (top-level block without surrounding parens).
pub fn file_parser() -> impl Parser<char, Expr, Error = Simple<char>> {
    let binding = {
        let ident = filter(|c: &char| c.is_alphabetic() || *c == '_')
            .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
            .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>());
        ident
            .then_ignore(hws())
            .then_ignore(assign())
            .then_ignore(hws())
            .then(expr_parser())
    };

    binding
        .then_ignore(sep())
        .repeated()
        .then(expr_parser())
        .map(|(bindings, body)| {
            if bindings.is_empty() {
                body
            } else {
                Expr::Block(bindings, Box::new(body))
            }
        })
        .padded()
        .then_ignore(end())
}
