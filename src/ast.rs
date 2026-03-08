pub type Span = std::ops::Range<usize>;
pub type Spanned<T> = (T, Span);

/// Surface type syntax — used in annotations.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Var(String),
    Con(String),
    /// `Con(te, ...)` — applied type constructor, e.g. `List(Int)`, `Task(Int, Str)`.
    App(String, Vec<TypeExpr>),
    Fun(Vec<TypeExpr>, Box<TypeExpr>),
    Record(Vec<(String, TypeExpr)>, Option<String>),  // None=closed, Some(row_var)=open
    Union(Vec<(String, Option<TypeExpr>)>, Option<String>),
}

/// A statement inside a block: either a value binding, type annotation, or monadic bind.
/// `MonadicBind` is desugared to `then(e, \x -> rest)` at parse time in block/file context,
/// but kept as-is for the REPL which executes tasks step by step.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockItem {
    Bind(String, Spanned<Expr>),
    Ann(String, TypeExpr),
    MonadicBind(String, Spanned<Expr>),
}

/// Core AST. Infix `a + b` desugars to `App(Var("+"), [a, b])`.
/// Template strings `` `hi {name}` `` desugar to `App(Var("++"), [Lit(Str("hi ")), name])`.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Var(String),
    Lam(Vec<String>, Box<Spanned<Expr>>),
    App(Box<Spanned<Expr>>, Vec<Spanned<Expr>>),
    Lit(Lit),
    /// `(x = e1; y : T; body)` — bindings and annotations scoped to body.
    Block(Vec<BlockItem>, Box<Spanned<Expr>>),
    /// `{field: expr, ...}`
    Record(Vec<(String, Spanned<Expr>)>),
    /// `expr.field`
    Field(Box<Spanned<Expr>>, String),
    /// `Tag` or `Tag expr`
    Tag(String, Option<Box<Spanned<Expr>>>),
    /// `when scrutinee is (Tag binding? -> body)+ (otherwise body)?`
    When(Box<Spanned<Expr>>, Vec<(String, Option<String>, Box<Spanned<Expr>>)>, Option<Box<Spanned<Expr>>>),
    /// `import \`path\`` — statically known path, resolved at type-check and eval time.
    Import(String),
    /// `[e1, e2, ...]` — list literal.
    List(Vec<Spanned<Expr>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i64),
    Str(String),
}
