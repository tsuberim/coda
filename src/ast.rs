pub type Span = std::ops::Range<usize>;

use std::sync::atomic::{AtomicUsize, Ordering};

static NODE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn fresh_node_id() -> NodeId {
    NodeId(NODE_COUNTER.fetch_add(1, Ordering::Relaxed))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub usize);

#[derive(Clone, Debug)]
pub struct Node<T> {
    pub id: NodeId,
    pub span: Span,
    pub inner: T,
}

impl<T: PartialEq> PartialEq for Node<T> {
    fn eq(&self, other: &Self) -> bool {
        // Compare by inner content and span, ignoring node id
        self.inner == other.inner && self.span == other.span
    }
}

impl<T> Node<T> {
    pub fn new(id: NodeId, span: Span, inner: T) -> Self {
        Node { id, span, inner }
    }
}

pub trait AstNode {
    fn node_id(&self) -> NodeId;
    fn span(&self) -> &Span;
}

impl<T> AstNode for Node<T> {
    fn node_id(&self) -> NodeId { self.id }
    fn span(&self) -> &Span { &self.span }
}

pub type Spanned<T> = Node<T>;

/// Surface type syntax — used in annotations.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Var(String),
    Con(String),
    /// `Con(te, ...)` — applied type constructor, e.g. `Task(Int, Str)`.
    App(String, Vec<TypeExpr>),
    Fun(Vec<TypeExpr>, Box<TypeExpr>),
    Record(Vec<(String, TypeExpr)>, Option<String>),  // None=closed, Some(row_var)=open
    Union(Vec<(String, Option<TypeExpr>)>, Option<String>),
    /// `T[d1, d2, ...]` — shaped type with dimension annotations.
    Shaped(Box<TypeExpr>, Vec<DimExpr>),
    /// Numeric literal in type position — used for tensor dimension annotations.
    Nat(u64),
}

/// Dimension expression in type annotations.
#[derive(Debug, Clone, PartialEq)]
pub enum DimExpr {
    Nat(u64),
    Var(String),
}

/// A statement inside a block: either a value binding, type annotation, or monadic bind.
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
    /// `[e1, e2, ...]` — array literal (homogeneous).
    List(Vec<Spanned<Expr>>),
    /// `e[i]`, `e[i,j]`, `e[i:j]` — indexing.
    Index(Box<Spanned<Expr>>, Vec<IndexArg>),
}

/// An indexing argument for `e[...]`.
#[derive(Debug, Clone, PartialEq)]
pub enum IndexArg {
    /// Integer index — consumes one dimension.
    Scalar(Spanned<Expr>),
    /// Array index (gather) — replaces one dimension with index array shape.
    Fancy(Spanned<Expr>),
    /// Slice `i:j` — replaces one dimension with `j-i` (if literals) or fresh var.
    Slice(Option<Spanned<Expr>>, Option<Spanned<Expr>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i64),
    Float(f64),
    Str(String),
}
