use chumsky::prelude::*;

use crate::ast::*;

const SYM_CHARS: &str = "!@$%^&*-+=|<>?/~.:";

fn line_comment() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    just("--")
        .then(filter(|c: &char| *c != '\n').repeated())
        .ignored()
}

fn block_comment() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    just("---")
        .then(
            just("---").not()
                .then(filter(|_| true))
                .repeated()
        )
        .then(just("---"))
        .ignored()
}

fn comment() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    block_comment().or(line_comment())
}

/// Skips any mix of whitespace and comments.
fn padding() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    filter(|c: &char| c.is_whitespace())
        .ignored()
        .or(comment())
        .repeated()
        .ignored()
}

fn hws() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    filter(|c: &char| *c == ' ' || *c == '\t')
        .repeated()
        .ignored()
}

fn hws1() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    filter(|c: &char| *c == ' ' || *c == '\t')
        .repeated()
        .at_least(1)
        .ignored()
}

fn ws1() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    // Any whitespace or comment counts; at least one whitespace char required.
    filter(|c: &char| c.is_whitespace())
        .ignored()
        .or(comment())
        .repeated()
        .at_least(1)
        .ignored()
}

fn sep() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    hws()
        .then(comment().or_not())
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

const KEYWORDS: &[&str] = &["when", "is", "otherwise"];

pub fn type_expr_parser() -> impl Parser<char, crate::ast::TypeExpr, Error = Simple<char>> {
    use crate::ast::TypeExpr;
    recursive(|te: Recursive<char, TypeExpr, Simple<char>>| {
        let type_var = filter(|c: &char| c.is_lowercase())
            .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
            .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>())
            .map(TypeExpr::Var);

        let type_con = filter(|c: &char| c.is_uppercase())
            .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
            .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>())
            .map(TypeExpr::Con);

        let row_var = just('|')
            .padded_by(hws())
            .ignore_then(
                just('*').map(|_| "*".to_string())
                    .or(
                        filter(|c: &char| c.is_alphabetic())
                            .then(filter(|c: &char| c.is_alphanumeric()).repeated())
                            .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>())
                    )
            );

        let type_record_field = filter(|c: &char| c.is_alphabetic() || *c == '_')
            .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
            .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>())
            .then_ignore(hws())
            .then_ignore(just(':'))
            .then_ignore(hws())
            .then(te.clone());

        let type_record = type_record_field
            .padded()
            .separated_by(just(','))
            .allow_trailing()
            .then(row_var.clone().or_not())
            .delimited_by(just('{'), just('}'))
            .map(|(fields, row)| TypeExpr::Record(fields, row));

        let type_union_tag = filter(|c: &char| c.is_uppercase())
            .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
            .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>())
            .then(
                // Optional payload type — separated by horizontal whitespace only.
                hws1()
                    .ignore_then(
                        filter(|c: &char| !c.is_whitespace())
                            .rewind()
                    )
                    .ignore_then(te.clone())
                    .or_not()
            );

        let type_union = type_union_tag
            .padded()
            .separated_by(just(','))
            .allow_trailing()
            .then(row_var.or_not())
            .delimited_by(just('['), just(']'))
            .map(|(tags, row)| TypeExpr::Union(tags, row));

        let type_parens = te.clone().padded().delimited_by(just('('), just(')'));

        // Atom type: no function arrows.
        let atom = choice((
            type_record,
            type_union,
            type_parens,
            type_con,
            type_var,
        ));

        // Function type: atom+ -> te, or just atom.
        // Use hws1() (horizontal-only) between chained atoms so we never consume
        // a newline and accidentally eat the identifier on the next line.
        atom.clone()
            .then(
                hws1()
                    .ignore_then(atom.clone())
                    .repeated()
            )
            .then(
                hws()
                    .ignore_then(just("->"))
                    .ignore_then(ws1())
                    .ignore_then(te.clone())
                    .or_not()
            )
            .map(|((first, rest), ret)| {
                match ret {
                    None => {
                        // No arrow — must be a single atom (extra atoms would be ambiguous).
                        first
                    }
                    Some(ret_ty) => {
                        let mut params = vec![first];
                        params.extend(rest);
                        TypeExpr::Fun(params, Box::new(ret_ty))
                    }
                }
            })
    })
}

pub fn expr_parser() -> impl Parser<char, Expr, Error = Simple<char>> {
    recursive(|expr: Recursive<char, Expr, Simple<char>>| {
        let ident = filter(|c: &char| c.is_alphabetic() || *c == '_')
            .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
            .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>())
            .try_map(|s, span| {
                if KEYWORDS.contains(&s.as_str()) {
                    Err(Simple::custom(span, format!("`{}` is a keyword", s)))
                } else {
                    Ok(s)
                }
            });

        // Tag names: capitalized identifier.
        let tag_name = filter(|c: &char| c.is_uppercase())
            .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
            .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>());

        let digits = text::digits::<char, Simple<char>>(10);

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
                // Left-fold into binary ++ calls so the type is Str -> Str -> Str.
                _ => parts
                    .into_iter()
                    .reduce(|acc, part| {
                        Expr::App(Box::new(Expr::Var("++".into())), vec![acc, part])
                    })
                    .unwrap(),
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

        // Block body: (name = expr | name : type ;)* expr
        let val_binding = ident
            .clone()
            .then_ignore(hws())
            .then_ignore(assign())
            .then_ignore(hws())
            .then(expr.clone())
            .map(|(name, e)| BlockItem::Bind(name, e));

        let ann_binding = ident
            .clone()
            .then_ignore(hws())
            .then_ignore(just(':'))
            .then_ignore(hws())
            .then(type_expr_parser())
            .map(|(name, te)| BlockItem::Ann(name, te))
            .boxed();

        let block_item = val_binding.boxed().or(ann_binding);

        let block_body = block_item
            .clone()
            .then_ignore(sep())
            .repeated()
            .then(expr.clone())
            .map(|(items, body)| {
                if items.is_empty() {
                    body
                } else {
                    Expr::Block(items, Box::new(body))
                }
            });

        let paren_block = block_body.padded().delimited_by(just('('), just(')'));

        // Record literal: {field: expr, ...}
        let record_field = ident
            .clone()
            .then_ignore(hws())
            .then_ignore(just(':'))
            .then_ignore(hws())
            .then(expr.clone());

        let record = record_field
            .padded()
            .separated_by(just(','))
            .allow_trailing()
            .delimited_by(just('{'), just('}'))
            .map(Expr::Record);

        // Tag expression: `Tag` or `Tag atom` (uppercase name + optional atom payload).
        // If followed by whitespace + non-uppercase, consume as payload.
        let tag_expr = tag_name.clone()
            .then(
                hws1()
                    .ignore_then(
                        filter(|c: &char| !c.is_uppercase() && !c.is_whitespace())
                            .rewind()
                    )
                    .ignore_then(expr.clone())
                    .or_not()
            )
            .map(|(name, payload)| Expr::Tag(name, payload.map(Box::new)));

        // Helper: parse a specific keyword (alphabetic token that equals kw).
        let kw = |kw: &'static str| {
            filter(|c: &char| c.is_alphabetic() || *c == '_')
                .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
                .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>())
                .try_map(move |s, span| {
                    if s == kw { Ok(()) } else { Err(Simple::custom(span, format!("expected `{}`", kw))) }
                })
        };

        let branch = tag_name.clone()
            .then_ignore(hws())
            .then(ident.clone().then_ignore(hws()).or_not())
            .then_ignore(just("->"))
            .then_ignore(hws())
            .then(expr.clone())
            .map(|((tag, binding), body)| (tag, binding, Box::new(body)));

        let otherwise_branch = kw("otherwise")
            .ignore_then(ws1())
            .ignore_then(expr.clone());

        let when_expr = kw("when")
            .ignore_then(ws1())
            .ignore_then(expr.clone())
            .then_ignore(ws1())
            .then_ignore(kw("is"))
            // First branch: sep (`;`/newline) or plain whitespace (inline).
            .then_ignore(sep().or(ws1()))
            .then(
                branch.clone()
                    .then(sep().ignore_then(branch.clone()).repeated())
                    .map(|(first, rest)| std::iter::once(first).chain(rest).collect::<Vec<_>>())
            )
            .then(sep().ignore_then(otherwise_branch).or_not())
            .map(|((scrutinee, branches), otherwise)| {
                Expr::When(Box::new(scrutinee), branches, otherwise.map(Box::new))
            });

        // Atom: strips LEADING whitespace and comments only (not trailing).
        // Trailing whitespace must remain for the infix `ws1` check.
        let leading_ws = filter(|c: &char| c.is_whitespace())
            .ignored()
            .or(comment())
            .repeated()
            .ignored();

        let atom = leading_ws.ignore_then(choice((
            int_lit,
            template,
            lambda,
            when_expr,
            record,
            paren_block,
            tag_expr,
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

        // Field access: .field — no whitespace before `.`
        let field = just('.').ignore_then(ident.clone());

        // Postfix chain: application and field access, left-associative.
        enum Postfix { Call(Vec<Expr>), Field(String) }
        let app = atom
            .then(
                choice((
                    args.map(Postfix::Call),
                    field.map(Postfix::Field),
                ))
                .repeated(),
            )
            .map(|(e, ops)| {
                ops.into_iter().fold(e, |acc, op| match op {
                    Postfix::Call(args) => Expr::App(Box::new(acc), args),
                    Postfix::Field(name) => Expr::Field(Box::new(acc), name),
                })
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

/// A single REPL input: a value binding, a type annotation, a bare expression, or nothing.
pub enum ReplInput {
    Binding(String, Expr),
    Ann(String, crate::ast::TypeExpr),
    Expr(Expr),
    Nop,
}

pub fn repl_parser() -> impl Parser<char, ReplInput, Error = Simple<char>> {
    let ident = filter(|c: &char| c.is_alphabetic() || *c == '_')
        .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
        .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>());

    let binding = ident
        .clone()
        .then_ignore(hws())
        .then_ignore(assign())
        .then_ignore(hws())
        .then(expr_parser())
        .map(|(name, expr)| ReplInput::Binding(name, expr));

    let ann = ident
        .then_ignore(hws())
        .then_ignore(just(':'))
        .then_ignore(hws())
        .then(type_expr_parser())
        .map(|(name, te)| ReplInput::Ann(name, te));

    padding().ignore_then(
        binding
            .or(ann)
            .or(expr_parser().map(ReplInput::Expr))
            .or(end().map(|_| ReplInput::Nop))
    )
    .then_ignore(padding())
    .then_ignore(end())
}

/// Parse a complete file (top-level block without surrounding parens).
pub fn file_parser() -> impl Parser<char, Expr, Error = Simple<char>> {
    let ident = filter(|c: &char| c.is_alphabetic() || *c == '_')
        .then(filter(|c: &char| c.is_alphanumeric() || *c == '_').repeated())
        .map(|(h, t): (char, Vec<char>)| std::iter::once(h).chain(t).collect::<String>());

    let val_item = ident
        .clone()
        .then_ignore(hws())
        .then_ignore(assign())
        .then_ignore(hws())
        .then(expr_parser())
        .map(|(name, e)| BlockItem::Bind(name, e));

    let ann_item = ident
        .clone()
        .then_ignore(hws())
        .then_ignore(just(':'))
        .then_ignore(hws())
        .then(type_expr_parser())
        .map(|(name, te)| BlockItem::Ann(name, te));

    let item = val_item.or(ann_item);

    item
        .then_ignore(sep())
        .repeated()
        .then(expr_parser())
        .map(|(items, body)| {
            if items.is_empty() {
                body
            } else {
                Expr::Block(items, Box::new(body))
            }
        })
        .padded_by(padding())
        .then_ignore(end())
}
