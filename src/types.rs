use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use colored::Colorize;

use crate::ast::{BlockItem, DimExpr, Expr, IndexArg, Lit, Node, NodeId, Span, Spanned, TypeExpr};

// ── Step 1: Dim and Shape ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Dim {
    Nat(u64),    // literal dimension — known statically
    Var(String), // unification variable
}

impl Dim {
    pub fn mul(a: &Dim, b: &Dim) -> Option<Dim> {
        match (a, b) {
            (Dim::Nat(m), Dim::Nat(n)) => Some(Dim::Nat(m * n)),
            _ => None,
        }
    }
    pub fn add(a: &Dim, b: &Dim) -> Option<Dim> {
        match (a, b) {
            (Dim::Nat(m), Dim::Nat(n)) => Some(Dim::Nat(m + n)),
            _ => None,
        }
    }
    pub fn sub(a: &Dim, b: &Dim) -> Option<Dim> {
        match (a, b) {
            (Dim::Nat(m), Dim::Nat(n)) if n <= m => Some(Dim::Nat(m - n)),
            _ => None,
        }
    }
}

impl fmt::Display for Dim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Dim::Nat(n) => write!(f, "{}", n),
            Dim::Var(v) => write!(f, "{}", v),
        }
    }
}

pub type Shape = Vec<Dim>;

// ── Step 2: BaseType and Type enums ──────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum BaseType {
    Int,
    F64,
    Str,
    Bool,
    Record(Vec<(String, Type)>, Option<String>),
    Union(Vec<(String, Type)>, Option<String>),
    Fun(Vec<Type>, Box<Type>),
    Task(Box<Type>, Box<Type>),
    Var(String),  // element type variable, ranges over scalar base types
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Every value type: base + shape ([] = scalar).
    Shaped(BaseType, Shape),
    /// HM type variable.
    Var(String),
}

impl Type {
    pub fn scalar(b: BaseType) -> Self { Type::Shaped(b, vec![]) }
    pub fn int() -> Self { Type::scalar(BaseType::Int) }
    pub fn f64_() -> Self { Type::scalar(BaseType::F64) }
    pub fn str_() -> Self { Type::scalar(BaseType::Str) }
    pub fn bool_() -> Self { Type::scalar(BaseType::Bool) }
    pub fn unit() -> Self { Type::scalar(BaseType::Record(vec![], None)) }
    pub fn never() -> Self { Type::scalar(BaseType::Union(vec![], None)) }
    pub fn fun(params: Vec<Type>, ret: Type) -> Self {
        Type::scalar(BaseType::Fun(params, Box::new(ret)))
    }
    pub fn shaped(b: BaseType, shape: Shape) -> Self { Type::Shaped(b, shape) }
    pub fn task(ok: Type, err: Type) -> Self {
        Type::scalar(BaseType::Task(Box::new(ok), Box::new(err)))
    }

    fn is_fun(&self) -> bool {
        matches!(self, Type::Shaped(BaseType::Fun(..), _))
    }

    /// Extract (params, ret) if this is a Fun type (shape must be []).
    fn as_fun(&self) -> Option<(&Vec<Type>, &Type)> {
        match self {
            Type::Shaped(BaseType::Fun(ps, r), sh) if sh.is_empty() => Some((ps, r)),
            _ => None,
        }
    }

    /// Get the shape of this type.
    pub fn shape(&self) -> &Shape {
        match self {
            Type::Shaped(_, s) => s,
            Type::Var(_) => &EMPTY_SHAPE,
        }
    }
}

static EMPTY_SHAPE: Shape = vec![];

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Var(n) => write!(f, "{}", n),
            Type::Shaped(base, shape) => {
                fmt_base(base, f)?;
                if !shape.is_empty() {
                    let dims: Vec<String> = shape.iter().map(|d| d.to_string()).collect();
                    write!(f, "[{}]", dims.join(", "))?;
                }
                Ok(())
            }
        }
    }
}

fn fmt_base(base: &BaseType, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match base {
        BaseType::Var(v) => write!(f, "{}", v),
        BaseType::Int => write!(f, "Int"),
        BaseType::F64 => write!(f, "F64"),
        BaseType::Str => write!(f, "Str"),
        BaseType::Bool => write!(f, "Bool"),
        BaseType::Record(fields, row) => {
            let pairs: Vec<_> = fields.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
            match row {
                None => write!(f, "{{{}}}", pairs.join(", ")),
                Some(_) if pairs.is_empty() => write!(f, "{{*}}"),
                Some(_) => write!(f, "{{{} | *}}", pairs.join(", ")),
            }
        }
        BaseType::Union(tags, row) => {
            let unit = Type::unit();
            let tag_strs: Vec<_> = tags.iter().map(|(tag, ty)| {
                if ty == &unit { tag.clone() } else { format!("{} {}", tag, ty) }
            }).collect();
            match row {
                None if tag_strs.is_empty() => write!(f, "[]"),
                None => write!(f, "[{}]", tag_strs.join(", ")),
                Some(_) if tag_strs.is_empty() => write!(f, "[*]"),
                Some(_) => write!(f, "[{} | *]", tag_strs.join(", ")),
            }
        }
        BaseType::Fun(params, ret) => {
            let t = Type::fun(params.clone(), (**ret).clone());
            if t.is_fun() {
                let params_str = params.iter().map(|p| {
                    if p.is_fun() { format!("({})", p) } else { p.to_string() }
                }).collect::<Vec<_>>().join(" ");
                write!(f, "{} -> {}", params_str, ret)
            } else {
                write!(f, "{}", t)
            }
        }
        BaseType::Task(ok, err) => write!(f, "Task({}, {})", ok, err),
    }
}

impl Type {
    pub fn pretty(&self) -> String {
        match self {
            Type::Var(n) => n.italic().to_string(),
            Type::Shaped(base, shape) => {
                let base_str = pretty_base(base);
                if shape.is_empty() {
                    base_str
                } else {
                    let dims: Vec<String> = shape.iter().map(|d| {
                        match d {
                            Dim::Nat(n) => n.to_string().bright_blue().bold().to_string(),
                            Dim::Var(v) => v.italic().to_string(),
                        }
                    }).collect();
                    format!("{}[{}]", base_str, dims.join(", "))
                }
            }
        }
    }
}

fn pretty_base(base: &BaseType) -> String {
    match base {
        BaseType::Var(v) => v.italic().to_string(),
        BaseType::Int => "Int".bright_blue().bold().to_string(),
        BaseType::F64 => "F64".bright_blue().bold().to_string(),
        BaseType::Str => "Str".bright_blue().bold().to_string(),
        BaseType::Bool => "Bool".bright_blue().bold().to_string(),
        BaseType::Record(fields, row) => {
            let pairs: Vec<_> = fields.iter()
                .map(|(k, v)| format!("{}: {}", k.bright_white(), v.pretty()))
                .collect();
            match row {
                None => format!("{{{}}}", pairs.join(", ")),
                Some(_) if pairs.is_empty() => "{*}".to_string(),
                Some(_) => format!("{{{} | *}}", pairs.join(", ")),
            }
        }
        BaseType::Union(tags, row) => {
            let unit = Type::unit();
            let tag_strs: Vec<_> = tags.iter().map(|(tag, ty)| {
                if ty == &unit {
                    tag.bright_yellow().to_string()
                } else {
                    format!("{} {}", tag.bright_yellow(), ty.pretty())
                }
            }).collect();
            match row {
                None if tag_strs.is_empty() => "[]".to_string(),
                None => format!("[{}]", tag_strs.join(", ")),
                Some(_) if tag_strs.is_empty() => "[*]".to_string(),
                Some(_) => format!("[{} | *]", tag_strs.join(", ")),
            }
        }
        BaseType::Fun(params, ret) => {
            let arrow = "->".dimmed().to_string();
            let params_str = params.iter().map(|p| {
                if p.is_fun() { format!("({})", p.pretty()) } else { p.pretty() }
            }).collect::<Vec<_>>().join(" ");
            format!("{} {} {}", params_str, arrow, ret.pretty())
        }
        BaseType::Task(ok, err) => format!("Task({}, {})", ok.pretty(), err.pretty()),
    }
}

// ── Scheme ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Scheme {
    pub vars: Vec<String>,       // type variables
    pub dim_vars: Vec<String>,   // dim variables
    pub base_vars: Vec<String>,  // base type variables
    pub ty: Type,
}

impl Scheme {
    pub fn mono(ty: Type) -> Self {
        Scheme { vars: vec![], dim_vars: vec![], base_vars: vec![], ty }
    }
}

// ── Step 1 continued: Substitution with dims ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct Subst {
    pub types: HashMap<String, Type>,
    pub dims:  HashMap<String, Dim>,
    pub bases: HashMap<String, BaseType>,  // base type variable substitutions
}

impl Subst {
    pub fn new() -> Self {
        Subst { types: HashMap::new(), dims: HashMap::new(), bases: HashMap::new() }
    }

    pub fn single_type(k: String, v: Type) -> Self {
        let mut s = Subst::new();
        s.types.insert(k, v);
        s
    }

    pub fn single_dim(k: String, v: Dim) -> Self {
        let mut s = Subst::new();
        s.dims.insert(k, v);
        s
    }
}

fn apply_dim_subst(s: &Subst, d: &Dim) -> Dim {
    match d {
        Dim::Nat(_) => d.clone(),
        Dim::Var(v) => s.dims.get(v).cloned().unwrap_or_else(|| d.clone()),
    }
}

fn apply_subst_shape(s: &Subst, shape: &Shape) -> Shape {
    shape.iter().map(|d| apply_dim_subst(s, d)).collect()
}

fn apply_subst(s: &Subst, ty: &Type) -> Type {
    match ty {
        Type::Var(n) => s.types.get(n).cloned().unwrap_or_else(|| ty.clone()),
        Type::Shaped(base, shape) => {
            let new_shape = apply_subst_shape(s, shape);
            let new_base = apply_subst_base(s, base);
            Type::Shaped(new_base, new_shape)
        }
    }
}

fn apply_subst_base(s: &Subst, base: &BaseType) -> BaseType {
    match base {
        BaseType::Var(v) => s.bases.get(v).cloned().unwrap_or_else(|| base.clone()),
        BaseType::Int => BaseType::Int,
        BaseType::F64 => BaseType::F64,
        BaseType::Str => BaseType::Str,
        BaseType::Bool => BaseType::Bool,
        BaseType::Record(fields, row) => {
            let new_fields: Vec<(String, Type)> = fields.iter()
                .map(|(k, v)| (k.clone(), apply_subst(s, v)))
                .collect();
            match row {
                None => BaseType::Record(new_fields, None),
                Some(r) => match s.types.get(r) {
                    None => BaseType::Record(new_fields, Some(r.clone())),
                    Some(bound) => match apply_subst(s, bound) {
                        Type::Shaped(BaseType::Record(extra, extra_row), sh) if sh.is_empty() => {
                            let mut merged = new_fields;
                            for (k, v) in extra {
                                if !merged.iter().any(|(mk, _)| mk == &k) {
                                    merged.push((k, v));
                                }
                            }
                            merged.sort_by(|a, b| a.0.cmp(&b.0));
                            BaseType::Record(merged, extra_row)
                        }
                        Type::Var(v) => BaseType::Record(new_fields, Some(v)),
                        _ => BaseType::Record(new_fields, Some(r.clone())),
                    },
                },
            }
        }
        BaseType::Union(tags, row) => {
            let new_tags: Vec<(String, Type)> = tags.iter()
                .map(|(k, v)| (k.clone(), apply_subst(s, v)))
                .collect();
            match row {
                None => BaseType::Union(new_tags, None),
                Some(r) => match s.types.get(r) {
                    None => BaseType::Union(new_tags, Some(r.clone())),
                    Some(bound) => match apply_subst(s, bound) {
                        Type::Shaped(BaseType::Union(extra, extra_row), sh) if sh.is_empty() => {
                            let mut merged = new_tags;
                            for (k, v) in extra {
                                if !merged.iter().any(|(mk, _)| mk == &k) {
                                    merged.push((k, v));
                                }
                            }
                            merged.sort_by(|a, b| a.0.cmp(&b.0));
                            BaseType::Union(merged, extra_row)
                        }
                        Type::Var(v) => BaseType::Union(new_tags, Some(v)),
                        _ => BaseType::Union(new_tags, Some(r.clone())),
                    },
                },
            }
        }
        BaseType::Fun(params, ret) => {
            BaseType::Fun(
                params.iter().map(|p| apply_subst(s, p)).collect(),
                Box::new(apply_subst(s, ret)),
            )
        }
        BaseType::Task(ok, err) => {
            BaseType::Task(Box::new(apply_subst(s, ok)), Box::new(apply_subst(s, err)))
        }
    }
}

fn apply_subst_scheme(s: &Subst, scheme: &Scheme) -> Scheme {
    let mut s2 = s.clone();
    for v in &scheme.vars { s2.types.remove(v); }
    for v in &scheme.dim_vars { s2.dims.remove(v); }
    for v in &scheme.base_vars { s2.bases.remove(v); }
    Scheme {
        vars: scheme.vars.clone(),
        dim_vars: scheme.dim_vars.clone(),
        base_vars: scheme.base_vars.clone(),
        ty: apply_subst(&s2, &scheme.ty),
    }
}

fn apply_subst_env(s: &Subst, env: &TypeEnv) -> TypeEnv {
    env.iter().map(|(k, v)| (k.clone(), apply_subst_scheme(s, v))).collect()
}

/// s1 ∘ s2: apply s1 to the range of s2, then union.
fn compose(s1: &Subst, s2: &Subst) -> Subst {
    let types: HashMap<String, Type> = s2.types.iter()
        .map(|(k, v)| (k.clone(), apply_subst(s1, v)))
        .chain(s1.types.iter().map(|(k, v)| (k.clone(), v.clone())))
        .collect();
    let dims: HashMap<String, Dim> = s2.dims.iter()
        .map(|(k, v)| (k.clone(), apply_dim_subst(s1, v)))
        .chain(s1.dims.iter().map(|(k, v)| (k.clone(), v.clone())))
        .collect();
    let bases: HashMap<String, BaseType> = s2.bases.iter()
        .map(|(k, v)| (k.clone(), apply_subst_base(s1, v)))
        .chain(s1.bases.iter().map(|(k, v)| (k.clone(), v.clone())))
        .collect();
    Subst { types, dims, bases }
}

// ── Free type/dim variables ───────────────────────────────────────────────────

fn fdv(d: &Dim) -> HashSet<String> {
    match d {
        Dim::Nat(_) => HashSet::new(),
        Dim::Var(v) => HashSet::from([v.clone()]),
    }
}

fn fdv_shape(shape: &Shape) -> HashSet<String> {
    shape.iter().flat_map(fdv).collect()
}

fn ftv(ty: &Type) -> HashSet<String> {
    match ty {
        Type::Var(n) => HashSet::from([n.clone()]),
        Type::Shaped(base, _shape) => ftv_base(base),
    }
}

fn ftv_base(base: &BaseType) -> HashSet<String> {
    match base {
        BaseType::Var(_) => HashSet::new(),
        BaseType::Int | BaseType::F64 | BaseType::Str | BaseType::Bool => HashSet::new(),
        BaseType::Record(fields, row) => {
            let mut vars: HashSet<String> = fields.iter().flat_map(|(_, v)| ftv(v)).collect();
            if let Some(r) = row { vars.insert(r.clone()); }
            vars
        }
        BaseType::Union(tags, row) => {
            let mut vars: HashSet<String> = tags.iter().flat_map(|(_, v)| ftv(v)).collect();
            if let Some(r) = row { vars.insert(r.clone()); }
            vars
        }
        BaseType::Fun(params, ret) => {
            params.iter().flat_map(ftv).chain(ftv(ret)).collect()
        }
        BaseType::Task(ok, err) => ftv(ok).union(&ftv(err)).cloned().collect(),
    }
}

fn ftv_all(ty: &Type) -> (HashSet<String>, HashSet<String>) {
    // Returns (type_vars, dim_vars)
    match ty {
        Type::Var(n) => (HashSet::from([n.clone()]), HashSet::new()),
        Type::Shaped(base, shape) => {
            let tvs = ftv_base(base);
            let dvs = fdv_shape(shape);
            // Also collect dim vars from inside base
            let inner_dvs = fdv_base(base);
            (tvs, dvs.union(&inner_dvs).cloned().collect())
        }
    }
}

fn fdv_base(base: &BaseType) -> HashSet<String> {
    match base {
        BaseType::Var(_) => HashSet::new(),
        BaseType::Int | BaseType::F64 | BaseType::Str | BaseType::Bool => HashSet::new(),
        BaseType::Record(fields, _) => fields.iter().flat_map(|(_, v)| fdv_shape(v.shape())).collect(),
        BaseType::Union(tags, _) => tags.iter().flat_map(|(_, v)| fdv_shape(v.shape())).collect(),
        BaseType::Fun(params, ret) => {
            params.iter().flat_map(|p| fdv_shape(p.shape()))
                .chain(fdv_shape(ret.shape()))
                .collect()
        }
        BaseType::Task(ok, err) => {
            fdv_shape(ok.shape()).union(&fdv_shape(err.shape())).cloned().collect()
        }
    }
}

fn ftv_scheme(s: &Scheme) -> HashSet<String> {
    ftv(&s.ty).into_iter().filter(|v| !s.vars.contains(v)).collect()
}

fn fdv_scheme(s: &Scheme) -> HashSet<String> {
    let (_, dvs) = ftv_all(&s.ty);
    dvs.into_iter().filter(|v| !s.dim_vars.contains(v)).collect()
}

fn ftv_env(env: &TypeEnv) -> HashSet<String> {
    env.values().flat_map(ftv_scheme).collect()
}

fn fdv_env(env: &TypeEnv) -> HashSet<String> {
    env.values().flat_map(fdv_scheme).collect()
}

// ── Free base variables ───────────────────────────────────────────────────────

fn fbv_base(b: &BaseType) -> HashSet<String> {
    match b {
        BaseType::Var(v) => HashSet::from([v.clone()]),
        BaseType::Record(fields, _) => fields.iter().flat_map(|(_, t)| fbv(t)).collect(),
        BaseType::Union(tags, _) => tags.iter().flat_map(|(_, t)| fbv(t)).collect(),
        BaseType::Fun(params, ret) => params.iter().flat_map(fbv).chain(fbv(ret)).collect(),
        BaseType::Task(ok, err) => fbv(ok).into_iter().chain(fbv(err)).collect(),
        _ => HashSet::new(),
    }
}

fn fbv(ty: &Type) -> HashSet<String> {
    match ty {
        Type::Shaped(base, _) => fbv_base(base),
        Type::Var(_) => HashSet::new(),
    }
}

fn fbv_scheme(s: &Scheme) -> HashSet<String> {
    fbv(&s.ty).into_iter().filter(|v| !s.base_vars.contains(v)).collect()
}

fn fbv_env(env: &TypeEnv) -> HashSet<String> {
    env.values().flat_map(fbv_scheme).collect()
}

// ── Type environment ──────────────────────────────────────────────────────────

pub type TypeEnv = HashMap<String, Scheme>;

// ── Step 3: Errors ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TypeError {
    UnificationFail(Type, Type),
    InfiniteType(String, Type),
    UnboundVar(String),
    NotARecord(Type),
    NoSuchField(String, Type),
    NotAUnion(Type),
    DeadOtherwise,
    ModuleError(String),
    // New tensor errors:
    RankMismatch(Shape, Shape),
    DimMismatch(Dim, Dim),
    DimArithmeticRequiresLiterals,
    RankPolymorphicInnerMismatch { expected: Shape, got: Shape },
    BroadcastFail(Shape, Shape),
    IndexOutOfRank { rank: usize, n_indices: usize },
    SliceRequiresLiterals,
    NotAnArray(Type),
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::UnificationFail(a, b) => write!(f, "type mismatch: expected `{}`, got `{}`", a, b),
            TypeError::InfiniteType(v, t) => write!(f, "recursive type: `{}` expands to `{}`", v, t),
            TypeError::UnboundVar(n) => write!(f, "unknown variable `{}`", n),
            TypeError::NotARecord(t) => write!(f, "not a record — has type `{}`", t),
            TypeError::NoSuchField(name, t) => write!(f, "no field `{}` on `{}`", name, t),
            TypeError::NotAUnion(t) => write!(f, "not a tagged union — has type `{}`", t),
            TypeError::DeadOtherwise => write!(f, "`otherwise` is unreachable: all tags already handled"),
            TypeError::ModuleError(msg) => write!(f, "module error: {}", msg),
            TypeError::RankMismatch(s1, s2) => {
                let fmt_shape = |s: &Shape| -> String {
                    if s.is_empty() { "[]".to_string() }
                    else { format!("[{}]", s.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ")) }
                };
                write!(f, "rank mismatch: {} vs {}", fmt_shape(s1), fmt_shape(s2))
            }
            TypeError::DimMismatch(d1, d2) => write!(f, "dimension mismatch: {} vs {}", d1, d2),
            TypeError::DimArithmeticRequiresLiterals => write!(f, "dimension arithmetic requires literal dimensions"),
            TypeError::RankPolymorphicInnerMismatch { expected, got } => {
                let fmt_s = |s: &Shape| -> String {
                    format!("[{}]", s.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", "))
                };
                write!(f, "rank-polymorphic inner shape mismatch: expected {}, got {}", fmt_s(expected), fmt_s(got))
            }
            TypeError::BroadcastFail(s1, s2) => {
                let fmt_s = |s: &Shape| -> String {
                    format!("[{}]", s.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", "))
                };
                write!(f, "broadcast fail: {} vs {}", fmt_s(s1), fmt_s(s2))
            }
            TypeError::IndexOutOfRank { rank, n_indices } => {
                write!(f, "too many indices: array has rank {}, got {} indices", rank, n_indices)
            }
            TypeError::SliceRequiresLiterals => write!(f, "slice bounds must be literal integers for static shape"),
            TypeError::NotAnArray(t) => write!(f, "not an array — has type `{}`", t),
        }
    }
}

// ── InferError ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct InferError {
    pub kind: TypeError,
    pub span: Span,
}

impl fmt::Display for InferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl InferError {
    pub fn render(&self, filename: &str, src: &str) -> String {
        use ariadne::{Color, Label, Report, ReportKind, sources};
        let mut out = Vec::<u8>::new();
        let span = (filename.to_string(), self.span.clone());
        Report::build(ReportKind::Error, span.clone())
            .with_message(self.kind.to_string())
            .with_label(
                Label::new(span)
                    .with_message(self.kind.to_string())
                    .with_color(Color::Red),
            )
            .finish()
            .write(sources([(filename.to_string(), src.to_string())]), &mut out)
            .unwrap_or(());
        String::from_utf8_lossy(&out).into_owned()
    }
}

// ── Step 3: Unification ───────────────────────────────────────────────────────

fn unify_dim(_ctx: &mut Ctx, d1: &Dim, d2: &Dim) -> Result<Subst, TypeError> {
    match (d1, d2) {
        (Dim::Nat(a), Dim::Nat(b)) => {
            if a == b { Ok(Subst::new()) }
            else { Err(TypeError::DimMismatch(d1.clone(), d2.clone())) }
        }
        (Dim::Var(a), Dim::Var(b)) if a == b => Ok(Subst::new()),
        (Dim::Var(a), d) | (d, Dim::Var(a)) => {
            Ok(Subst::single_dim(a.clone(), d.clone()))
        }
    }
}

fn unify_shapes(ctx: &mut Ctx, s1: &Shape, s2: &Shape) -> Result<Subst, TypeError> {
    if s1.len() != s2.len() {
        return Err(TypeError::RankMismatch(s1.clone(), s2.clone()));
    }
    s1.iter().zip(s2.iter()).try_fold(Subst::new(), |s, (d1, d2)| {
        let d1 = apply_dim_subst(&s, d1);
        let d2 = apply_dim_subst(&s, d2);
        let s2 = unify_dim(ctx, &d1, &d2)?;
        Ok(compose(&s2, &s))
    })
}

fn unify(ctx: &mut Ctx, t1: &Type, t2: &Type) -> Result<Subst, TypeError> {
    match (t1, t2) {
        (Type::Var(a), Type::Var(b)) if a == b => Ok(Subst::new()),
        (Type::Var(a), t) | (t, Type::Var(a)) => bind_type(a, t),
        (Type::Shaped(b1, s1), Type::Shaped(b2, s2)) => {
            let ss = unify_shapes(ctx, s1, s2)?;
            let b1s = apply_subst_base(&ss, b1);
            let b2s = apply_subst_base(&ss, b2);
            let sb = unify_base(ctx, &b1s, &b2s, t1, t2)?;
            Ok(compose(&sb, &ss))
        }
    }
}

fn unify_base(ctx: &mut Ctx, b1: &BaseType, b2: &BaseType, orig1: &Type, orig2: &Type) -> Result<Subst, TypeError> {
    match (b1, b2) {
        (BaseType::Var(a), BaseType::Var(b)) if a == b => Ok(Subst::new()),
        (BaseType::Var(a), other) => {
            if fbv_base(other).contains(a) {
                return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone()));
            }
            let mut s = Subst::new();
            s.bases.insert(a.clone(), other.clone());
            Ok(s)
        }
        (other, BaseType::Var(a)) => {
            if fbv_base(other).contains(a) {
                return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone()));
            }
            let mut s = Subst::new();
            s.bases.insert(a.clone(), other.clone());
            Ok(s)
        }
        (BaseType::Int, BaseType::Int) => Ok(Subst::new()),
        (BaseType::F64, BaseType::F64) => Ok(Subst::new()),
        (BaseType::Str, BaseType::Str) => Ok(Subst::new()),
        (BaseType::Bool, BaseType::Bool) => Ok(Subst::new()),
        (BaseType::Fun(ps1, r1), BaseType::Fun(ps2, r2)) => {
            if ps1.len() != ps2.len() {
                return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone()));
            }
            let mut s = Subst::new();
            for (p1, p2) in ps1.iter().zip(ps2.iter()) {
                let s2 = unify(ctx, &apply_subst(&s, p1), &apply_subst(&s, p2))?;
                s = compose(&s2, &s);
            }
            let sr = unify(ctx, &apply_subst(&s, r1), &apply_subst(&s, r2))?;
            Ok(compose(&sr, &s))
        }
        (BaseType::Task(ok1, err1), BaseType::Task(ok2, err2)) => {
            let s1 = unify(ctx, ok1, ok2)?;
            let s2 = unify(ctx, &apply_subst(&s1, err1), &apply_subst(&s1, err2))?;
            Ok(compose(&s2, &s1))
        }
        (BaseType::Record(f1, row1), BaseType::Record(f2, row2)) => {
            let t1_rec = Type::scalar(BaseType::Record(f1.clone(), row1.clone()));
            let t2_rec = Type::scalar(BaseType::Record(f2.clone(), row2.clone()));
            unify_rows(ctx, f1, row1, f2, row2,
                |fields, row| Type::scalar(BaseType::Record(fields, row)),
                &t1_rec, &t2_rec)
        }
        (BaseType::Union(t1_tags, row1), BaseType::Union(t2_tags, row2)) => {
            let t1_union = Type::scalar(BaseType::Union(t1_tags.clone(), row1.clone()));
            let t2_union = Type::scalar(BaseType::Union(t2_tags.clone(), row2.clone()));
            unify_rows(ctx, t1_tags, row1, t2_tags, row2,
                |tags, row| Type::scalar(BaseType::Union(tags, row)),
                &t1_union, &t2_union)
        }
        _ => Err(TypeError::UnificationFail(orig1.clone(), orig2.clone())),
    }
}

/// Shared row-unification logic for both Record and Union.
fn unify_rows(
    ctx: &mut Ctx,
    f1: &[(String, Type)],
    row1: &Option<String>,
    f2: &[(String, Type)],
    row2: &Option<String>,
    make: impl Fn(Vec<(String, Type)>, Option<String>) -> Type,
    orig1: &Type,
    orig2: &Type,
) -> Result<Subst, TypeError> {
    let map1: HashMap<&str, &Type> = f1.iter().map(|(k, v)| (k.as_str(), v)).collect();
    let map2: HashMap<&str, &Type> = f2.iter().map(|(k, v)| (k.as_str(), v)).collect();

    let mut s = f1.iter().try_fold(Subst::new(), |s, (k, ty)| {
        if let Some(ty2) = map2.get(k.as_str()) {
            let s2 = unify(ctx, &apply_subst(&s, ty), &apply_subst(&s, ty2))?;
            Ok(compose(&s2, &s))
        } else { Ok(s) }
    })?;

    let only1: Vec<(String, Type)> = f1.iter()
        .filter(|(k, _)| !map2.contains_key(k.as_str()))
        .map(|(k, v)| (k.clone(), apply_subst(&s, v)))
        .collect();
    let only2: Vec<(String, Type)> = f2.iter()
        .filter(|(k, _)| !map1.contains_key(k.as_str()))
        .map(|(k, v)| (k.clone(), apply_subst(&s, v)))
        .collect();

    match (!only1.is_empty(), !only2.is_empty()) {
        (true, true) => {
            let shared = ctx.fresh_name();
            match row2 {
                None => return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone())),
                Some(r2) => {
                    let su = bind_type(r2, &make(only1.clone(), Some(shared.clone())))?;
                    s = compose(&su, &s);
                }
            }
            match row1 {
                None => return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone())),
                Some(r1) => {
                    let su = bind_type(r1, &make(only2.clone(), Some(shared)))?;
                    s = compose(&su, &s);
                }
            }
        }
        (true, false) => match row2 {
            None => return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone())),
            Some(r2) => {
                let su = bind_type(r2, &make(only1.clone(), row1.clone()))?;
                s = compose(&su, &s);
            }
        },
        (false, true) => match row1 {
            None => return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone())),
            Some(r1) => {
                let rest = apply_subst(&s, &make(only2.clone(), row2.clone()));
                let su = bind_type(r1, &rest)?;
                s = compose(&su, &s);
            }
        },
        (false, false) => {
            let su = match (row1, row2) {
                (Some(r1), Some(r2)) => unify(ctx, &Type::Var(r1.clone()), &Type::Var(r2.clone()))?,
                (Some(r1), None) => bind_type(r1, &make(vec![], None))?,
                (None, Some(r2)) => bind_type(r2, &make(vec![], None))?,
                (None, None) => Subst::new(),
            };
            s = compose(&su, &s);
        }
    }

    Ok(s)
}

fn bind_type(var: &str, ty: &Type) -> Result<Subst, TypeError> {
    if let Type::Var(v) = ty {
        if v == var { return Ok(Subst::new()); }
    }
    if ftv(ty).contains(var) {
        return Err(TypeError::InfiniteType(var.into(), ty.clone()));
    }
    Ok(Subst::single_type(var.into(), ty.clone()))
}

// ── Step 8: Broadcasting ──────────────────────────────────────────────────────

fn broadcast(s1: &Shape, s2: &Shape) -> Result<Shape, TypeError> {
    // s1 == s2 → s1
    if s1 == s2 { return Ok(s1.clone()); }
    // s1 is prefix of s2 → s2
    if s2.starts_with(s1.as_slice()) { return Ok(s2.clone()); }
    // s2 is prefix of s1 → s1
    if s1.starts_with(s2.as_slice()) { return Ok(s1.clone()); }
    // Scalar [] is prefix of everything already handled above
    Err(TypeError::BroadcastFail(s1.clone(), s2.clone()))
}

// ── Step 7: Lifting ───────────────────────────────────────────────────────────

/// Attempt lifting: given a fun type and actual arg types, compute the result type with lifting.
/// Returns None if lifting is not applicable (no args have higher rank than params).
fn try_lift(
    ctx: &mut Ctx,
    f_ty: &Type,
    arg_tys: &[Type],
    span: &Span,
) -> Result<Option<(Subst, Type)>, (TypeError, Span)> {
    let (params, ret) = match f_ty.as_fun() {
        Some(x) => x,
        None => return Ok(None),
    };

    if params.len() != arg_tys.len() {
        return Ok(None); // arity mismatch handled elsewhere
    }

    // Check if any arg has higher rank than its param.
    let mut any_lifted = false;
    for (param, arg) in params.iter().zip(arg_tys.iter()) {
        let p_rank = param.shape().len();
        let a_rank = arg.shape().len();
        if a_rank > p_rank {
            any_lifted = true;
            break;
        }
    }
    if !any_lifted {
        return Ok(None);
    }

    // Lifting algorithm (§5.2):
    // 1. For each (pi, ai): strip s_inner from tail of arg shape → get s_outer_i
    // 2. s_outer = broadcast over all s_outer_i
    // 3. Unify param base with arg base
    // 4. Result = Shaped(ret_base, s_outer ++ ret_shape)

    let mut s = Subst::new();
    let mut outer_shapes: Vec<Shape> = Vec::new();
    let mut base_unifications: Vec<(Type, Type)> = Vec::new(); // (param_scalar, arg_scalar)

    for (param, arg) in params.iter().zip(arg_tys.iter()) {
        let param = apply_subst(&s, param);
        let arg = apply_subst(&s, arg);

        let p_shape = param.shape().clone();
        let a_shape = arg.shape().clone();

        if a_shape.len() < p_shape.len() {
            // arg has lower rank than param — can't lift
            return Err((
                TypeError::RankPolymorphicInnerMismatch {
                    expected: p_shape,
                    got: a_shape,
                },
                span.clone(),
            ));
        }

        let outer_len = a_shape.len() - p_shape.len();
        let s_outer_i: Shape = a_shape[..outer_len].to_vec();
        let s_inner_i: Shape = a_shape[outer_len..].to_vec();

        // Check inner matches param shape
        let us = unify_shapes(ctx, &s_inner_i, &p_shape)
            .map_err(|_| (TypeError::RankPolymorphicInnerMismatch { expected: p_shape.clone(), got: s_inner_i.clone() }, span.clone()))?;
        s = compose(&us, &s);

        outer_shapes.push(s_outer_i);

        // Collect base type unification pairs
        let param_base_ty = match &param {
            Type::Shaped(b, _) => Type::scalar(b.clone()),
            v => v.clone(),
        };
        let arg_base_ty = match &arg {
            Type::Shaped(b, _) => Type::scalar(b.clone()),
            v => v.clone(),
        };
        base_unifications.push((param_base_ty, arg_base_ty));
    }

    // Unify base types
    for (p_base, a_base) in &base_unifications {
        let p_base = apply_subst(&s, p_base);
        let a_base = apply_subst(&s, a_base);
        let su = unify(ctx, &p_base, &a_base).map_err(|e| (e, span.clone()))?;
        s = compose(&su, &s);
    }

    // Compute s_outer = fold broadcast over all s_outer_i
    let s_outer = outer_shapes.into_iter().try_fold(vec![], |acc, shape| {
        broadcast(&acc, &shape)
    }).map_err(|e| (e, span.clone()))?;

    // Result = Shaped(ret_base, s_outer ++ ret_shape)
    let ret_applied = apply_subst(&s, ret);
    let (ret_base, ret_shape) = match ret_applied {
        Type::Shaped(b, sh) => (b, sh),
        Type::Var(_v) => {
            // ret is still a type variable — produce a fresh result with s_outer
            // (the var will be resolved by caller's unification context)
            let result_ty = if s_outer.is_empty() {
                Type::Var(_v)
            } else {
                // Fresh base; unification will fix it
                Type::Shaped(BaseType::Int, s_outer.clone())
            };
            return Ok(Some((s, result_ty)));
        }
    };

    let mut result_shape = s_outer;
    result_shape.extend(ret_shape);
    let result_ty = Type::Shaped(ret_base, result_shape);

    Ok(Some((s, result_ty)))
}

// ── TypeMap ───────────────────────────────────────────────────────────────────

/// Maps each AST node to its inferred (post-substitution) type.
#[derive(Debug, Clone)]
pub struct TypeMap {
    pub types: HashMap<NodeId, Type>,
    pub lifted_apps: HashSet<NodeId>,
}

impl TypeMap {
    fn new() -> Self {
        TypeMap {
            types: HashMap::new(),
            lifted_apps: HashSet::new(),
        }
    }
}

// ── Inference context (fresh variable supply) ─────────────────────────────────

struct Ctx {
    counter: usize,
    type_map: TypeMap,
}

impl Ctx {
    fn new() -> Self { Ctx { counter: 0, type_map: TypeMap::new() } }

    fn fresh_name(&mut self) -> String {
        let n = self.counter;
        self.counter += 1;
        format!("t{}", n)
    }

    fn fresh_dim_name(&mut self) -> String {
        let n = self.counter;
        self.counter += 1;
        format!("d{}", n)
    }

    fn fresh_base_name(&mut self) -> String {
        let n = self.counter;
        self.counter += 1;
        format!("b{}", n)
    }

    fn fresh(&mut self) -> Type {
        Type::Var(self.fresh_name())
    }

    fn fresh_dim(&mut self) -> Dim {
        Dim::Var(self.fresh_dim_name())
    }

    fn instantiate(&mut self, scheme: &Scheme) -> Type {
        let mut s = Subst::new();
        for v in &scheme.vars {
            s.types.insert(v.clone(), self.fresh());
        }
        for v in &scheme.dim_vars {
            s.dims.insert(v.clone(), self.fresh_dim());
        }
        for v in &scheme.base_vars {
            s.bases.insert(v.clone(), BaseType::Var(self.fresh_base_name()));
        }
        apply_subst(&s, &scheme.ty)
    }
}

// ── Step 6: type_expr_to_type ─────────────────────────────────────────────────

pub fn type_expr_to_type(te: &TypeExpr, counter: &mut usize) -> Type {
    match te {
        TypeExpr::Var(v) => Type::Var(v.clone()),
        TypeExpr::Con(n) => match n.as_str() {
            "Int" => Type::int(),
            "F64" => Type::f64_(),
            "Str" => Type::str_(),
            "Bool" => Type::bool_(),
            _ => Type::scalar(BaseType::Union(vec![], None)), // unknown con → never
        },
        TypeExpr::App(name, args) => match name.as_str() {
            "Task" if args.len() == 2 => {
                let ok = type_expr_to_type(&args[0], counter);
                let err = type_expr_to_type(&args[1], counter);
                Type::task(ok, err)
            }
            _ => Type::Var(format!("_{}", name)), // unknown app
        },
        TypeExpr::Fun(params, ret) => {
            let ps = params.iter().map(|p| type_expr_to_type(p, counter)).collect();
            let r = type_expr_to_type(ret, counter);
            Type::fun(ps, r)
        }
        TypeExpr::Record(fields, row) => {
            let fs = fields.iter().map(|(k, v)| (k.clone(), type_expr_to_type(v, counter))).collect();
            let r = row.as_ref().map(|name| {
                if name == "*" { let n = format!("_r{}", { *counter += 1; *counter }); n }
                else { name.clone() }
            });
            Type::scalar(BaseType::Record(fs, r))
        }
        TypeExpr::Union(tags, row) => {
            let ts = tags.iter().map(|(tag, payload)| {
                let ty = payload.as_ref()
                    .map(|p| type_expr_to_type(p, counter))
                    .unwrap_or(Type::unit());
                (tag.clone(), ty)
            }).collect();
            let r = row.as_ref().map(|name| {
                if name == "*" { let n = format!("_r{}", { *counter += 1; *counter }); n }
                else { name.clone() }
            });
            Type::scalar(BaseType::Union(ts, r))
        }
        TypeExpr::Shaped(inner, dim_exprs) => {
            let inner_ty = type_expr_to_type(inner, counter);
            let shape: Shape = dim_exprs.iter().map(|d| match d {
                DimExpr::Nat(n) => Dim::Nat(*n),
                DimExpr::Var(v) => Dim::Var(v.clone()),
            }).collect();
            match inner_ty {
                Type::Shaped(base, _) => Type::Shaped(base, shape),
                Type::Var(_v) => Type::Shaped(BaseType::Int, shape), // can't attach shape to var easily
            }
        }
        TypeExpr::Nat(n) => Type::Shaped(BaseType::Int, vec![Dim::Nat(*n)]),
    }
}

fn generalize(env: &TypeEnv, ty: &Type) -> Scheme {
    let free_tvs = ftv(ty);
    let env_tvs = ftv_env(env);
    let vars: Vec<String> = free_tvs.difference(&env_tvs).cloned().collect();

    let (_, free_dvs) = ftv_all(ty);
    let env_dvs = fdv_env(env);
    let dim_vars: Vec<String> = free_dvs.difference(&env_dvs).cloned().collect();

    let free_bvs = fbv(ty);
    let env_bvs = fbv_env(env);
    let base_vars: Vec<String> = free_bvs.difference(&env_bvs).cloned().collect();

    Scheme { vars, dim_vars, base_vars, ty: ty.clone() }
}

// ── Algorithm W ───────────────────────────────────────────────────────────────

fn infer_inner(ctx: &mut Ctx, env: &TypeEnv, expr: &Spanned<Expr>) -> Result<(Subst, Type), (TypeError, Span)> {
    let Node { id, inner: expr, span } = expr;
    let result = infer_inner_impl(ctx, env, *id, expr, span);
    if let Ok((ref s, ref ty)) = result {
        ctx.type_map.types.insert(*id, apply_subst(s, ty));
    }
    result
}

fn infer_inner_impl(ctx: &mut Ctx, env: &TypeEnv, node_id: NodeId, expr: &Expr, span: &Span) -> Result<(Subst, Type), (TypeError, Span)> {
    match expr {
        Expr::Lit(Lit::Int(_)) => Ok((Subst::new(), Type::int())),
        Expr::Lit(Lit::Float(_)) => Ok((Subst::new(), Type::f64_())),
        Expr::Lit(Lit::Str(_)) => Ok((Subst::new(), Type::str_())),

        Expr::Var(name) => {
            let scheme = env.get(name)
                .ok_or_else(|| (TypeError::UnboundVar(name.clone()), span.clone()))?;
            Ok((Subst::new(), ctx.instantiate(scheme)))
        }

        Expr::Lam(params, body) => {
            let param_tys: Vec<Type> = params.iter().map(|_| ctx.fresh()).collect();
            let mut env2 = env.clone();
            for (p, t) in params.iter().zip(&param_tys) {
                env2.insert(p.clone(), Scheme::mono(t.clone()));
            }
            let (s, body_ty) = infer_inner(ctx, &env2, body)?;
            let param_tys_subst: Vec<Type> = param_tys.iter().map(|pt| apply_subst(&s, pt)).collect();
            Ok((s, Type::fun(param_tys_subst, body_ty)))
        }

        Expr::App(f, args) => {
            let ret = ctx.fresh();
            let (s0, f_ty) = infer_inner(ctx, env, f)?;

            let mut s = s0;
            let mut arg_tys = Vec::new();
            for arg in args {
                let (sa, arg_ty) = infer_inner(ctx, &apply_subst_env(&s, env), arg)?;
                s = compose(&sa, &s);
                arg_tys.push(arg_ty);
            }

            let f_ty_current = apply_subst(&s, &f_ty);

            // Step 7: Try lifting first (only for non-empty args).
            if !args.is_empty() {
                let arg_tys_subst: Vec<Type> = arg_tys.iter().map(|at| apply_subst(&s, at)).collect();

                match try_lift(ctx, &f_ty_current, &arg_tys_subst, span) {
                    Ok(Some((s_lift, result_ty))) => {
                        ctx.type_map.lifted_apps.insert(node_id);
                        let s_final = compose(&s_lift, &s);
                        return Ok((s_final.clone(), apply_subst(&s_final, &result_ty)));
                    }
                    Err(e) => {
                        // Lifting was applicable but failed (e.g. BroadcastFail) — propagate error.
                        return Err(e);
                    }
                    Ok(None) => {} // Not applicable, fall through to normal application.
                }
            }

            // Normal (non-lifted) application.
            let expected = if arg_tys.is_empty() {
                apply_subst(&s, &ret)
            } else {
                let arg_tys_subst: Vec<Type> = arg_tys.iter().map(|at| apply_subst(&s, at)).collect();
                Type::fun(arg_tys_subst, apply_subst(&s, &ret))
            };

            let su = unify(ctx, &f_ty_current, &expected)
                .map_err(|e| (e, span.clone()))?;
            let s_final = compose(&su, &s);
            Ok((s_final.clone(), apply_subst(&s_final, &ret)))
        }

        Expr::Record(fields) => {
            let mut s = Subst::new();
            let mut typed_fields = Vec::new();
            for (name, expr) in fields {
                let (s1, ty) = infer_inner(ctx, &apply_subst_env(&s, env), expr)?;
                s = compose(&s1, &s);
                typed_fields.push((name.clone(), ty));
            }
            Ok((s, Type::scalar(BaseType::Record(typed_fields, None))))
        }

        Expr::Field(expr, name) => {
            let (s, expr_ty) = infer_inner(ctx, env, expr)?;
            let ty = apply_subst(&s, &expr_ty);
            match &ty {
                Type::Var(v) => {
                    let field_ty = ctx.fresh();
                    let row = ctx.fresh_name();
                    let record_ty = Type::scalar(BaseType::Record(
                        vec![(name.clone(), field_ty.clone())], Some(row)
                    ));
                    let su = bind_type(v, &record_ty).map_err(|e| (e, span.clone()))?;
                    let s_final = compose(&su, &s);
                    Ok((s_final, field_ty))
                }
                Type::Shaped(BaseType::Record(fields, row), sh) if sh.is_empty() => {
                    match fields.iter().find(|(k, _)| k == name) {
                        Some((_, field_ty)) => Ok((s, field_ty.clone())),
                        None => match row {
                            Some(r) => {
                                let field_ty = ctx.fresh();
                                let new_row = ctx.fresh_name();
                                let extension = Type::scalar(BaseType::Record(
                                    vec![(name.clone(), field_ty.clone())],
                                    Some(new_row),
                                ));
                                let su = bind_type(r, &extension).map_err(|e| (e, span.clone()))?;
                                let s_final = compose(&su, &s);
                                Ok((s_final, field_ty))
                            }
                            None => Err((TypeError::NoSuchField(
                                name.clone(),
                                ty.clone(),
                            ), span.clone())),
                        },
                    }
                }
                other => Err((TypeError::NotARecord(other.clone()), span.clone())),
            }
        }

        Expr::Tag(name, payload) => {
            let (s, payload_ty) = match payload {
                Some(e) => infer_inner(ctx, env, e)?,
                None => (Subst::new(), Type::unit()),
            };
            let row = ctx.fresh_name();
            Ok((s, Type::scalar(BaseType::Union(vec![(name.clone(), payload_ty)], Some(row)))))
        }

        Expr::When(scrutinee, branches, otherwise) => {
            let (s0, scrut_ty) = infer_inner(ctx, env, scrutinee)?;
            let mut s = s0;
            let ret = ctx.fresh();

            let payload_tys: Vec<Type> = branches.iter().map(|_| ctx.fresh()).collect();
            let branch_tags: Vec<(String, Type)> = branches.iter()
                .zip(&payload_tys)
                .map(|((tag, _, _), pty)| (tag.clone(), pty.clone()))
                .collect();

            let scrut_union = if otherwise.is_some() {
                Type::scalar(BaseType::Union(branch_tags, Some(ctx.fresh_name())))
            } else {
                Type::scalar(BaseType::Union(branch_tags, None))
            };

            let scrut_current = apply_subst(&s, &scrut_ty);
            let su = unify(ctx, &scrut_current, &scrut_union)
                .map_err(|e| (e, span.clone()))?;
            s = compose(&su, &s);

            if otherwise.is_some() {
                let scrut_final = apply_subst(&s, &scrut_ty);
                if let Type::Shaped(BaseType::Union(_, None), _) = scrut_final {
                    return Err((TypeError::DeadOtherwise, span.clone()));
                }
            }

            for ((_, binding, body), payload_ty) in branches.iter().zip(&payload_tys) {
                let mut env2 = apply_subst_env(&s, env);
                if let Some(b) = binding {
                    env2.insert(b.clone(), Scheme::mono(apply_subst(&s, payload_ty)));
                } else {
                    let su = unify(ctx, &apply_subst(&s, payload_ty), &Type::unit())
                        .map_err(|e| (e, span.clone()))?;
                    s = compose(&su, &s);
                }
                let (sb, body_ty) = infer_inner(ctx, &env2, body)?;
                s = compose(&sb, &s);
                let su = unify(ctx, &apply_subst(&s, &body_ty), &apply_subst(&s, &ret))
                    .map_err(|e| (e, body.span.clone()))?;
                s = compose(&su, &s);
            }

            if let Some(otherwise_body) = otherwise {
                let env2 = apply_subst_env(&s, env);
                let (sb, body_ty) = infer_inner(ctx, &env2, otherwise_body)?;
                s = compose(&sb, &s);
                let su = unify(ctx, &apply_subst(&s, &body_ty), &apply_subst(&s, &ret))
                    .map_err(|e| (e, otherwise_body.span.clone()))?;
                s = compose(&su, &s);
            }

            Ok((s.clone(), apply_subst(&s, &ret)))
        }

        // Step 12: Array literals — [1,2,3] infers as Int[3].
        Expr::List(elems) => {
            let n = elems.len();
            let elem_ty = ctx.fresh();
            let mut s = Subst::new();
            for elem in elems {
                let (se, te) = infer_inner(ctx, &apply_subst_env(&s, env), elem)?;
                s = compose(&se, &s);
                let su = unify(ctx, &apply_subst(&s, &te), &apply_subst(&s, &elem_ty))
                    .map_err(|e| (e, elem.span.clone()))?;
                s = compose(&su, &s);
            }
            let elem_ty_resolved = apply_subst(&s, &elem_ty);
            // Build array type: elem_base[n]
            let array_ty = match elem_ty_resolved {
                Type::Shaped(base, inner_shape) => {
                    // Prepend n to the element's shape: elem[d1,d2] → elem[n,d1,d2]
                    let mut shape = vec![Dim::Nat(n as u64)];
                    shape.extend(inner_shape);
                    Type::Shaped(base, shape)
                }
                Type::Var(_) => {
                    // elem type is unconstrained — use a fresh base var so it stays polymorphic
                    Type::Shaped(BaseType::Var(ctx.fresh_base_name()), vec![Dim::Nat(n as u64)])
                }
            };
            Ok((s, array_ty))
        }

        Expr::Import(path) => {
            let ty = crate::module::load_module(path)
                .map_err(|e| (TypeError::ModuleError(e), span.clone()))?.ty;
            Ok((Subst::new(), ty))
        }

        Expr::Block(items, body) => {
            let mut env2 = env.clone();
            let mut s = Subst::new();
            let mut row_counter = 0usize;
            for item in items {
                match item {
                    BlockItem::Bind(name, expr) => {
                        let env3 = apply_subst_env(&s, &env2);
                        let (s1, ty) = infer_inner(ctx, &env3, expr)?;
                        let s2 = if let Some(existing) = env3.get(name) {
                            let existing_ty = ctx.instantiate(existing);
                            compose(
                                &unify(ctx, &apply_subst(&s1, &ty), &existing_ty)
                                    .map_err(|e| (e, expr.span.clone()))?,
                                &s1,
                            )
                        } else {
                            s1
                        };
                        let scheme = generalize(&apply_subst_env(&s2, &env2), &apply_subst(&s2, &ty));
                        s = compose(&s2, &s);
                        env2 = apply_subst_env(&s, &env2);
                        env2.insert(name.clone(), scheme);
                    }
                    BlockItem::Ann(name, te) => {
                        let ann_ty = type_expr_to_type(te, &mut row_counter);
                        let env3 = apply_subst_env(&s, &env2);
                        if let Some(existing) = env3.get(name) {
                            let existing_ty = ctx.instantiate(existing);
                            let s1 = unify(ctx, &existing_ty, &ann_ty)
                                .map_err(|e| (e, span.clone()))?;
                            s = compose(&s1, &s);
                            env2 = apply_subst_env(&s, &env2);
                            let scheme = generalize(&apply_subst_env(&s, &env2), &apply_subst(&s, &ann_ty));
                            env2.insert(name.clone(), scheme);
                        } else {
                            let scheme = generalize(&env3, &ann_ty);
                            env2.insert(name.clone(), scheme);
                        }
                    }
                    BlockItem::MonadicBind(_, _) => unreachable!("desugared at parse time"),
                }
            }
            let env3 = apply_subst_env(&s, &env2);
            let (s2, body_ty) = infer_inner(ctx, &env3, body)?;
            Ok((compose(&s2, &s), body_ty))
        }

        // Step 9: Indexing
        Expr::Index(arr_expr, indices) => {
            let (s0, arr_ty) = infer_inner(ctx, env, arr_expr)?;
            let mut s = s0;
            let arr_ty_resolved = apply_subst(&s, &arr_ty);

            match arr_ty_resolved {
                Type::Shaped(base, shape) => {
                    if indices.len() > shape.len() {
                        return Err((TypeError::IndexOutOfRank {
                            rank: shape.len(),
                            n_indices: indices.len(),
                        }, span.clone()));
                    }

                    let mut result_shape: Vec<Dim> = vec![];
                    let mut remaining_shape = shape.clone();

                    for (_i, idx) in indices.iter().enumerate() {
                        match idx {
                            IndexArg::Scalar(idx_expr) => {
                                // Integer index: consume one dimension.
                                let (si, idx_ty) = infer_inner(ctx, &apply_subst_env(&s, env), idx_expr)?;
                                s = compose(&si, &s);
                                let idx_ty_resolved = apply_subst(&s, &idx_ty);
                                let su = unify(ctx, &idx_ty_resolved, &Type::int())
                                    .map_err(|e| (e, idx_expr.span.clone()))?;
                                s = compose(&su, &s);
                                // Consume the first remaining dim.
                                if remaining_shape.is_empty() {
                                    return Err((TypeError::IndexOutOfRank {
                                        rank: shape.len(),
                                        n_indices: indices.len(),
                                    }, span.clone()));
                                }
                                remaining_shape.remove(0);
                            }
                            IndexArg::Fancy(idx_expr) => {
                                // Array index (gather): replace first dim with index array dim.
                                let (si, idx_ty) = infer_inner(ctx, &apply_subst_env(&s, env), idx_expr)?;
                                s = compose(&si, &s);
                                let idx_ty_resolved = apply_subst(&s, &idx_ty);
                                // idx must be Int[k] for some k
                                match idx_ty_resolved {
                                    Type::Shaped(BaseType::Int, idx_shape) => {
                                        if remaining_shape.is_empty() {
                                            return Err((TypeError::IndexOutOfRank {
                                                rank: shape.len(),
                                                n_indices: indices.len(),
                                            }, span.clone()));
                                        }
                                        remaining_shape.remove(0);
                                        // Insert index shape dims at start of result
                                        for d in &idx_shape {
                                            result_shape.push(d.clone());
                                        }
                                    }
                                    other => {
                                        return Err((TypeError::UnificationFail(
                                            Type::Shaped(BaseType::Int, vec![Dim::Var("k".into())]),
                                            other,
                                        ), idx_expr.span.clone()));
                                    }
                                }
                            }
                            IndexArg::Slice(from, to) => {
                                // Slice: produce a dim
                                let result_dim = match (from, to) {
                                    (Some(f_expr), Some(t_expr)) => {
                                        // Infer both bounds as Int
                                        let (sf, ft) = infer_inner(ctx, &apply_subst_env(&s, env), f_expr)?;
                                        s = compose(&sf, &s);
                                        let su = unify(ctx, &apply_subst(&s, &ft), &Type::int())
                                            .map_err(|e| (e, f_expr.span.clone()))?;
                                        s = compose(&su, &s);

                                        let (st, tt) = infer_inner(ctx, &apply_subst_env(&s, env), t_expr)?;
                                        s = compose(&st, &s);
                                        let su = unify(ctx, &apply_subst(&s, &tt), &Type::int())
                                            .map_err(|e| (e, t_expr.span.clone()))?;
                                        s = compose(&su, &s);

                                        // Try to compute static dim if both are literals
                                        match (f_expr.inner.clone(), t_expr.inner.clone()) {
                                            (Expr::Lit(Lit::Int(a)), Expr::Lit(Lit::Int(b))) => {
                                                Dim::Nat((b - a) as u64)
                                            }
                                            _ => Dim::Var(ctx.fresh_dim_name()),
                                        }
                                    }
                                    _ => Dim::Var(ctx.fresh_dim_name()),
                                };
                                if !remaining_shape.is_empty() {
                                    remaining_shape.remove(0);
                                }
                                result_shape.push(result_dim);
                            }
                        }
                    }

                    // Remaining shape dims come after all processed indices
                    result_shape.extend(remaining_shape);
                    let result_ty = Type::Shaped(base, result_shape);
                    Ok((s, result_ty))
                }
                other => Err((TypeError::NotAnArray(other), span.clone())),
            }
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn normalize_ty(ty: Type) -> Type { normalize(ty) }

/// Shared core: run inference, apply final substitution to TypeMap, return (subst, type, type_map).
fn run_infer(env: &TypeEnv, expr: &Spanned<Expr>) -> Result<(Subst, Type, TypeMap), InferError> {
    let mut ctx = Ctx::new();
    let (s, ty) = infer_inner(&mut ctx, env, expr)
        .map_err(|(kind, span)| InferError { kind, span })?;
    for t in ctx.type_map.types.values_mut() {
        *t = apply_subst(&s, t);
    }
    Ok((s, ty, ctx.type_map))
}

pub fn infer(env: &TypeEnv, expr: &Spanned<Expr>) -> Result<Type, InferError> {
    let (s, ty, _) = run_infer(env, expr)?;
    Ok(normalize(apply_subst(&s, &ty)))
}

/// Like `infer` but also returns the full `TypeMap` mapping each AST node to its final type.
pub fn infer_with_map(env: &TypeEnv, expr: &Spanned<Expr>) -> Result<(Type, TypeMap), InferError> {
    let (s, ty, type_map) = run_infer(env, expr)?;
    Ok((normalize(apply_subst(&s, &ty)), type_map))
}

pub fn infer_scheme(env: &TypeEnv, expr: &Spanned<Expr>) -> Result<Scheme, InferError> {
    let (s, ty, _) = run_infer(env, expr)?;
    Ok(generalize(&apply_subst_env(&s, env), &apply_subst(&s, &ty)))
}

/// Like `infer_scheme` but also returns the `TypeMap` for eval-time lifting decisions.
pub fn infer_scheme_with_map(env: &TypeEnv, expr: &Spanned<Expr>) -> Result<(Scheme, TypeMap), InferError> {
    let (s, ty, type_map) = run_infer(env, expr)?;
    let scheme = generalize(&apply_subst_env(&s, env), &apply_subst(&s, &ty));
    Ok((scheme, type_map))
}

fn constrain_against_existing(
    ctx: &mut Ctx,
    env: &TypeEnv,
    name: &str,
    ty: &Type,
) -> Result<(Subst, Type), TypeError> {
    if let Some(existing) = env.get(name) {
        let existing_ty = ctx.instantiate(existing);
        let s = unify(ctx, ty, &existing_ty)?;
        let resolved = apply_subst(&s, ty);
        Ok((s, resolved))
    } else {
        Ok((Subst::new(), ty.clone()))
    }
}

pub fn enforce_binding(env: &TypeEnv, name: &str, inferred: Scheme) -> Result<Scheme, TypeError> {
    let mut ctx = Ctx::new();
    let inferred_ty = ctx.instantiate(&inferred);
    let (s, resolved) = constrain_against_existing(&mut ctx, env, name, &inferred_ty)?;
    Ok(generalize(&apply_subst_env(&s, env), &resolved))
}

pub fn apply_ann(env: &TypeEnv, name: &str, te: &TypeExpr) -> Result<Scheme, TypeError> {
    let mut ctx = Ctx::new();
    let ann_ty = type_expr_to_type(te, &mut 0usize);
    let (s, resolved) = constrain_against_existing(&mut ctx, env, name, &ann_ty)?;
    Ok(generalize(&apply_subst_env(&s, env), &resolved))
}

// ── Step 10: Standard type environment ───────────────────────────────────────

pub fn std_type_env() -> TypeEnv {
    let mut env = TypeEnv::new();

    let tv = |n: &str| Type::Var(n.into());
    let dv = |n: &str| Dim::Var(n.into());

    // String concat: Str Str -> Str (stays scalar; lifting handles arrays)
    env.insert("++".into(), Scheme::mono(
        Type::fun(vec![Type::str_(), Type::str_()], Type::str_())
    ));

    // Arithmetic: Int Int -> Int (lifting handles shaped args)
    env.insert("+".into(), Scheme::mono(
        Type::fun(vec![Type::int(), Type::int()], Type::int())
    ));
    env.insert("-".into(), Scheme::mono(
        Type::fun(vec![Type::int(), Type::int()], Type::int())
    ));
    env.insert("*".into(), Scheme::mono(
        Type::fun(vec![Type::int(), Type::int()], Type::int())
    ));

    // Float arithmetic (scalar; lifting handles shaped args)
    env.insert("+.".into(), Scheme::mono(
        Type::fun(vec![Type::f64_(), Type::f64_()], Type::f64_())
    ));
    env.insert("-.".into(), Scheme::mono(
        Type::fun(vec![Type::f64_(), Type::f64_()], Type::f64_())
    ));
    env.insert("*.".into(), Scheme::mono(
        Type::fun(vec![Type::f64_(), Type::f64_()], Type::f64_())
    ));
    env.insert("/.".into(), Scheme::mono(
        Type::fun(vec![Type::f64_(), Type::f64_()], Type::f64_())
    ));

    // == : ∀a. a -> a -> [False | True]
    let bool_union = Type::scalar(BaseType::Union(
        vec![("False".into(), Type::unit()), ("True".into(), Type::unit())], None
    ));
    env.insert("==".into(), Scheme {
        vars: vec!["a".into()],
        dim_vars: vec![],
        base_vars: vec![],
        ty: Type::fun(vec![tv("a"), tv("a")], bool_union),
    });

    // fix : ∀a. (a -> a) -> a
    env.insert("fix".into(), Scheme {
        vars: vec!["a".into()],
        dim_vars: vec![],
        base_vars: vec![],
        ty: Type::fun(vec![Type::fun(vec![tv("a")], tv("a"))], tv("a")),
    });

    // Task helpers.
    let task = |ok: Type, err: Type| Type::task(ok, err);

    // ok : ∀a e. a -> Task a e
    env.insert("ok".into(), Scheme {
        vars: vec!["a".into(), "e".into()],
        dim_vars: vec![],
        base_vars: vec![],
        ty: Type::fun(vec![tv("a")], task(tv("a"), tv("e"))),
    });

    // >>= : ∀a b e. Task a e -> (a -> Task b e) -> Task b e
    env.insert(">>=".into(), Scheme {
        vars: vec!["a".into(), "b".into(), "e".into()],
        dim_vars: vec![],
        base_vars: vec![],
        ty: Type::fun(
            vec![
                task(tv("a"), tv("e")),
                Type::fun(vec![tv("a")], task(tv("b"), tv("e"))),
            ],
            task(tv("b"), tv("e")),
        ),
    });

    // catch : ∀a e f. Task a e -> (e -> Task a f) -> Task a f
    env.insert("catch".into(), Scheme {
        vars: vec!["a".into(), "e".into(), "f".into()],
        dim_vars: vec![],
        base_vars: vec![],
        ty: Type::fun(
            vec![
                task(tv("a"), tv("e")),
                Type::fun(vec![tv("e")], task(tv("a"), tv("f"))),
            ],
            task(tv("a"), tv("f")),
        ),
    });

    // fail : ∀a e. e -> Task a e
    env.insert("fail".into(), Scheme {
        vars: vec!["a".into(), "e".into()],
        dim_vars: vec![],
        base_vars: vec![],
        ty: Type::fun(vec![tv("e")], task(tv("a"), tv("e"))),
    });

    // print : ∀r. Str -> Task {} [IoErr Str | r]
    env.insert("print".into(), Scheme {
        vars: vec!["r".into()],
        dim_vars: vec![],
        base_vars: vec![],
        ty: Type::fun(
            vec![Type::str_()],
            task(Type::unit(), Type::scalar(BaseType::Union(
                vec![("IoErr".into(), Type::str_())], Some("r".into())
            ))),
        ),
    });

    // read_line : ∀r. Task Str [IoErr Str | r]
    env.insert("read_line".into(), Scheme {
        vars: vec!["r".into()],
        dim_vars: vec![],
        base_vars: vec![],
        ty: task(Type::str_(), Type::scalar(BaseType::Union(
            vec![("IoErr".into(), Type::str_())], Some("r".into())
        ))),
    });

    // debug : ∀a. a -> a  (pass-through, prints to stderr)
    env.insert("debug".into(), Scheme {
        vars: vec!["a".into()],
        dim_vars: vec![],
        base_vars: vec![],
        ty: Type::fun(vec![tv("a")], tv("a")),
    });

    let bv = |n: &str| BaseType::Var(n.into());

    // len : ∀(elem: Base) d. elem[d] -> Int
    env.insert("len".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["d".into()],
        base_vars: vec!["elem".into()],
        ty: Type::fun(
            vec![Type::Shaped(bv("elem"), vec![dv("d")])],
            Type::int(),
        ),
    });

    // fold : ∀(a: Base) b n. (b -> a -> b) -> b -> a[n] -> b
    env.insert("fold".into(), Scheme {
        vars: vec!["b".into()],
        dim_vars: vec!["n".into()],
        base_vars: vec!["a".into()],
        ty: Type::fun(
            vec![
                Type::fun(vec![tv("b"), Type::Shaped(bv("a"), vec![])], tv("b")),
                tv("b"),
                Type::Shaped(bv("a"), vec![dv("n")]),
            ],
            tv("b"),
        ),
    });

    // concat : ∀(elem: Base) m n k. elem[m] -> elem[n] -> elem[k]
    env.insert("concat".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into(), "n".into(), "k".into()],
        base_vars: vec!["elem".into()],
        ty: Type::fun(
            vec![
                Type::Shaped(bv("elem"), vec![dv("m")]),
                Type::Shaped(bv("elem"), vec![dv("n")]),
            ],
            Type::Shaped(bv("elem"), vec![dv("k")]),
        ),
    });

    // fill : ∀(a: Base) k. Int -> a -> a[k]
    env.insert("fill".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["k".into()],
        base_vars: vec!["a".into()],
        ty: Type::fun(
            vec![Type::int(), Type::Shaped(bv("a"), vec![])],
            Type::Shaped(bv("a"), vec![dv("k")]),
        ),
    });

    // tabulate : ∀(a: Base) k. Int -> (Int -> a) -> a[k]
    env.insert("tabulate".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["k".into()],
        base_vars: vec!["a".into()],
        ty: Type::fun(
            vec![Type::int(), Type::fun(vec![Type::int()], Type::Shaped(bv("a"), vec![]))],
            Type::Shaped(bv("a"), vec![dv("k")]),
        ),
    });

    // transpose : ∀m n. F64[m, n] -> F64[n, m]
    env.insert("transpose".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into(), "n".into()],
        base_vars: vec![],
        ty: Type::fun(
            vec![Type::Shaped(BaseType::F64, vec![dv("m"), dv("n")])],
            Type::Shaped(BaseType::F64, vec![dv("n"), dv("m")]),
        ),
    });

    // flatten : ∀m n k. F64[m, n] -> F64[k]
    env.insert("flatten".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into(), "n".into(), "k".into()],
        base_vars: vec![],
        ty: Type::fun(
            vec![Type::Shaped(BaseType::F64, vec![dv("m"), dv("n")])],
            Type::Shaped(BaseType::F64, vec![dv("k")]),
        ),
    });

    // unsqueeze : ∀n. F64[n] -> F64[1, n]
    env.insert("unsqueeze".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["n".into()],
        base_vars: vec![],
        ty: Type::fun(
            vec![Type::Shaped(BaseType::F64, vec![dv("n")])],
            Type::Shaped(BaseType::F64, vec![Dim::Nat(1), dv("n")]),
        ),
    });

    // squeeze : ∀n. F64[1, n] -> F64[n]
    env.insert("squeeze".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["n".into()],
        base_vars: vec![],
        ty: Type::fun(
            vec![Type::Shaped(BaseType::F64, vec![Dim::Nat(1), dv("n")])],
            Type::Shaped(BaseType::F64, vec![dv("n")]),
        ),
    });

    // zeros : ∀m n. Int -> Int -> F64[m, n]
    env.insert("zeros".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into(), "n".into()],
        base_vars: vec![],
        ty: Type::fun(
            vec![Type::int(), Type::int()],
            Type::Shaped(BaseType::F64, vec![dv("m"), dv("n")]),
        ),
    });

    // ones : ∀m n. Int -> Int -> F64[m, n]
    env.insert("ones".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into(), "n".into()],
        base_vars: vec![],
        ty: Type::fun(
            vec![Type::int(), Type::int()],
            Type::Shaped(BaseType::F64, vec![dv("m"), dv("n")]),
        ),
    });

    // dot : ∀(elem: Base) n. elem[n] -> elem[n] -> elem
    env.insert("dot".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["n".into()],
        base_vars: vec!["elem".into()],
        ty: Type::fun(
            vec![
                Type::Shaped(BaseType::Var("elem".into()), vec![dv("n")]),
                Type::Shaped(BaseType::Var("elem".into()), vec![dv("n")]),
            ],
            Type::Shaped(BaseType::Var("elem".into()), vec![]),
        ),
    });

    // matmul : ∀m k n. F64[m, k] -> F64[k, n] -> F64[m, n]
    env.insert("matmul".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into(), "k".into(), "n".into()],
        base_vars: vec![],
        ty: Type::fun(
            vec![
                Type::Shaped(BaseType::F64, vec![dv("m"), dv("k")]),
                Type::Shaped(BaseType::F64, vec![dv("k"), dv("n")]),
            ],
            Type::Shaped(BaseType::F64, vec![dv("m"), dv("n")]),
        ),
    });

    // :: : ∀(elem: Base) m k. elem -> elem[m] -> elem[k]
    env.insert("::".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into(), "k".into()],
        base_vars: vec!["elem".into()],
        ty: Type::fun(
            vec![
                Type::Shaped(bv("elem"), vec![]),
                Type::Shaped(bv("elem"), vec![dv("m")]),
            ],
            Type::Shaped(bv("elem"), vec![dv("k")]),
        ),
    });

    // head : ∀(elem: Base) m. elem[m] -> [None | Some elem]
    let some_none_elem = |a: Type| Type::scalar(BaseType::Union(
        vec![("None".into(), Type::unit()), ("Some".into(), a)], None,
    ));
    env.insert("head".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into()],
        base_vars: vec!["elem".into()],
        ty: Type::fun(
            vec![Type::Shaped(bv("elem"), vec![dv("m")])],
            some_none_elem(Type::Shaped(bv("elem"), vec![])),
        ),
    });

    // tail : ∀(elem: Base) m k. elem[m] -> [None | Some elem[k]]
    env.insert("tail".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into(), "k".into()],
        base_vars: vec!["elem".into()],
        ty: Type::fun(
            vec![Type::Shaped(bv("elem"), vec![dv("m")])],
            some_none_elem(Type::Shaped(bv("elem"), vec![dv("k")])),
        ),
    });

    // map : ∀(a b: Base) m. (a -> b) -> a[m] -> b[m]
    env.insert("map".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into()],
        base_vars: vec!["a".into(), "b".into()],
        ty: Type::fun(
            vec![
                Type::fun(vec![Type::Shaped(bv("a"), vec![])], Type::Shaped(bv("b"), vec![])),
                Type::Shaped(bv("a"), vec![dv("m")]),
            ],
            Type::Shaped(bv("b"), vec![dv("m")]),
        ),
    });

    // list_of : ∀(a: Base) k. Int -> a -> a[k]
    env.insert("list_of".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["k".into()],
        base_vars: vec!["a".into()],
        ty: Type::fun(
            vec![Type::int(), Type::Shaped(bv("a"), vec![])],
            Type::Shaped(bv("a"), vec![dv("k")]),
        ),
    });

    // list_init : ∀(a: Base) k. Int -> (Int -> a) -> a[k]
    env.insert("list_init".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["k".into()],
        base_vars: vec!["a".into()],
        ty: Type::fun(
            vec![Type::int(), Type::fun(vec![Type::int()], Type::Shaped(bv("a"), vec![]))],
            Type::Shaped(bv("a"), vec![dv("k")]),
        ),
    });

    // <> : ∀(elem: Base) m n k. elem[m] -> elem[n] -> elem[k]
    env.insert("<>".into(), Scheme {
        vars: vec![],
        dim_vars: vec!["m".into(), "n".into(), "k".into()],
        base_vars: vec!["elem".into()],
        ty: Type::fun(
            vec![
                Type::Shaped(bv("elem"), vec![dv("m")]),
                Type::Shaped(bv("elem"), vec![dv("n")]),
            ],
            Type::Shaped(bv("elem"), vec![dv("k")]),
        ),
    });

    // Aliases (after all symbols are defined)
    for (alias, sym) in [("then", ">>="), ("cons", "::"), ("append", "<>")] {
        if let Some(scheme) = env.get(sym).cloned() {
            env.insert(alias.into(), scheme);
        }
    }

    env
}

// ── Normalization: rename t0,t1,… → a,b,… ───────────────────────────────────

fn normalize(ty: Type) -> Type {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut counter = 0usize;
    normalize_inner(&ty, &mut map, &mut counter)
}

fn normalize_inner(ty: &Type, map: &mut HashMap<String, String>, n: &mut usize) -> Type {
    match ty {
        Type::Var(v) => {
            let renamed = map.entry(v.clone()).or_insert_with(|| {
                let name = var_name(*n);
                *n += 1;
                name
            });
            Type::Var(renamed.clone())
        }
        Type::Shaped(base, shape) => {
            let new_base = normalize_base_inner(base, map, n);
            let new_shape = shape.iter().map(|d| normalize_dim_inner(d, map, n)).collect();
            Type::Shaped(new_base, new_shape)
        }
    }
}

fn normalize_dim_inner(d: &Dim, map: &mut HashMap<String, String>, n: &mut usize) -> Dim {
    match d {
        Dim::Nat(v) => Dim::Nat(*v),
        Dim::Var(v) => {
            let renamed = map.entry(format!("dim:{}", v)).or_insert_with(|| {
                let name = var_name(*n);
                *n += 1;
                name
            });
            Dim::Var(renamed.clone())
        }
    }
}

fn normalize_base_inner(base: &BaseType, map: &mut HashMap<String, String>, n: &mut usize) -> BaseType {
    match base {
        BaseType::Var(v) => {
            let renamed = map.entry(format!("base:{}", v)).or_insert_with(|| {
                let name = var_name(*n);
                *n += 1;
                name
            });
            BaseType::Var(renamed.clone())
        }
        BaseType::Int => BaseType::Int,
        BaseType::F64 => BaseType::F64,
        BaseType::Str => BaseType::Str,
        BaseType::Bool => BaseType::Bool,
        BaseType::Record(fields, row) => {
            let new_fields = fields.iter()
                .map(|(k, v)| (k.clone(), normalize_inner(v, map, n)))
                .collect();
            let new_row = row.as_ref().map(|r| {
                map.entry(r.clone()).or_insert_with(|| {
                    let name = var_name(*n); *n += 1; name
                }).clone()
            });
            BaseType::Record(new_fields, new_row)
        }
        BaseType::Union(tags, row) => {
            let mut new_tags: Vec<(String, Type)> = tags.iter()
                .map(|(k, v)| (k.clone(), normalize_inner(v, map, n)))
                .collect();
            new_tags.sort_by(|a, b| a.0.cmp(&b.0));
            let new_row = row.as_ref().map(|r| {
                map.entry(r.clone()).or_insert_with(|| {
                    let name = var_name(*n); *n += 1; name
                }).clone()
            });
            BaseType::Union(new_tags, new_row)
        }
        BaseType::Fun(params, ret) => {
            BaseType::Fun(
                params.iter().map(|p| normalize_inner(p, map, n)).collect(),
                Box::new(normalize_inner(ret, map, n)),
            )
        }
        BaseType::Task(ok, err) => {
            BaseType::Task(
                Box::new(normalize_inner(ok, map, n)),
                Box::new(normalize_inner(err, map, n)),
            )
        }
    }
}

fn var_name(n: usize) -> String {
    if n < 26 { char::from(b'a' + n as u8).to_string() }
    else { format!("t{}", n) }
}
