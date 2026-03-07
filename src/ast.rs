/// Core AST. Infix `a + b` desugars to `App(Var("+"), [a, b])`.
/// Template strings `` `hi {name}` `` desugar to `App(Var("concat"), [Lit(Str("hi ")), name])`.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Var(String),
    Lam(Vec<String>, Box<Expr>),
    App(Box<Expr>, Vec<Expr>),
    Lit(Lit),
    /// `(x = e1; y = e2; body)` — bindings scoped to body.
    Block(Vec<(String, Expr)>, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i64),
    Float(f64),
    Str(String),
}
