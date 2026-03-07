use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use colored::Colorize;

use crate::ast::{BlockItem, Expr, Lit, TypeExpr};

// ── Type ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Var(String),
    Con(String, Vec<Type>),
    /// Record type with optional row variable. None=closed, Some(r)=open.
    Record(Vec<(String, Type)>, Option<String>),
    /// Union (tagged) type with optional row variable. None=closed, Some(r)=open.
    Union(Vec<(String, Type)>, Option<String>),
}

impl Type {
    pub fn int() -> Self { Type::Con("Int".into(), vec![]) }
    pub fn str_() -> Self { Type::Con("Str".into(), vec![]) }
    /// Empty record — unit type.
    pub fn unit() -> Self { Type::Record(vec![], None) }
    /// Empty union — never/bottom type.
    pub fn never() -> Self { Type::Union(vec![], None) }
    /// N-ary function type: `params -> ret`. Requires at least one param.
    pub fn fun(mut params: Vec<Type>, ret: Type) -> Self {
        assert!(!params.is_empty());
        params.push(ret);
        Type::Con("->".into(), params)
    }

    fn is_fun(&self) -> bool { matches!(self, Type::Con(n, args) if n == "->" && args.len() >= 2) }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Var(n) => write!(f, "{}", n),
            Type::Con(n, args) if n == "->" && args.len() >= 2 => {
                let (params, ret) = args.split_at(args.len() - 1);
                let params_str = params.iter().map(|p| {
                    if p.is_fun() { format!("({})", p) } else { p.to_string() }
                }).collect::<Vec<_>>().join(" ");
                write!(f, "{} -> {}", params_str, ret[0])
            }
            Type::Con(n, args) if args.is_empty() => write!(f, "{}", n),
            Type::Con(n, args) => {
                write!(f, "{}({})", n, args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", "))
            }
            Type::Record(fields, row) => {
                let pairs: Vec<_> = fields.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
                match row {
                    None => write!(f, "{{{}}}", pairs.join(", ")),
                    Some(_) if pairs.is_empty() => write!(f, "{{*}}"),
                    Some(_) => write!(f, "{{{} | *}}", pairs.join(", ")),
                }
            }
            Type::Union(tags, row) => {
                let tag_strs: Vec<_> = tags.iter().map(|(tag, ty)| {
                    if ty == &Type::unit() { tag.clone() } else { format!("{} {}", tag, ty) }
                }).collect();
                match row {
                    None if tag_strs.is_empty() => write!(f, "[]"),
                    None => write!(f, "[{}]", tag_strs.join(", ")),
                    Some(_) if tag_strs.is_empty() => write!(f, "[*]"),
                    Some(_) => write!(f, "[{} | *]", tag_strs.join(", ")),
                }
            }
        }
    }
}

impl Type {
    /// Colored display for REPL.
    pub fn pretty(&self) -> String {
        match self {
            Type::Var(n) => n.italic().to_string(),
            Type::Con(n, args) if n == "->" && args.len() >= 2 => {
                let (params, ret) = args.split_at(args.len() - 1);
                let arrow = "->".dimmed().to_string();
                let params_str = params.iter().map(|p| {
                    if p.is_fun() { format!("({})", p.pretty()) } else { p.pretty() }
                }).collect::<Vec<_>>().join(" ");
                format!("{} {} {}", params_str, arrow, ret[0].pretty())
            }
            Type::Con(n, args) if args.is_empty() => n.bright_blue().bold().to_string(),
            Type::Con(n, args) => {
                let args_str = args.iter().map(|a| a.pretty()).collect::<Vec<_>>().join(", ");
                format!("{}({})", n.bright_blue().bold(), args_str)
            }
            Type::Record(fields, row) => {
                let pairs: Vec<_> = fields.iter()
                    .map(|(k, v)| format!("{}: {}", k.bright_white(), v.pretty()))
                    .collect();
                match row {
                    None => format!("{{{}}}", pairs.join(", ")),
                    Some(_) if pairs.is_empty() => "{*}".to_string(),
                    Some(_) => format!("{{{} | *}}", pairs.join(", ")),
                }
            }
            Type::Union(tags, row) => {
                let tag_strs: Vec<_> = tags.iter().map(|(tag, ty)| {
                    if ty == &Type::unit() {
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
        }
    }
}

// ── Scheme ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Scheme {
    pub vars: Vec<String>,
    pub ty: Type,
}

impl Scheme {
    pub fn mono(ty: Type) -> Self {
        Scheme { vars: vec![], ty }
    }
}

// ── Substitution ──────────────────────────────────────────────────────────────

pub type Subst = HashMap<String, Type>;

fn apply_subst(s: &Subst, ty: &Type) -> Type {
    match ty {
        Type::Var(n) => s.get(n).cloned().unwrap_or_else(|| ty.clone()),
        Type::Con(n, args) => Type::Con(n.clone(), args.iter().map(|a| apply_subst(s, a)).collect()),
        Type::Record(fields, row) => {
            let new_fields: Vec<(String, Type)> = fields.iter()
                .map(|(k, v)| (k.clone(), apply_subst(s, v)))
                .collect();
            match row {
                None => Type::Record(new_fields, None),
                Some(r) => match s.get(r) {
                    None => Type::Record(new_fields, Some(r.clone())),
                    Some(bound) => match apply_subst(s, bound) {
                        Type::Record(extra, extra_row) => {
                            let mut merged = new_fields;
                            for (k, v) in extra {
                                if !merged.iter().any(|(mk, _)| mk == &k) {
                                    merged.push((k, v));
                                }
                            }
                            merged.sort_by(|a, b| a.0.cmp(&b.0));
                            Type::Record(merged, extra_row)
                        }
                        Type::Var(v) => Type::Record(new_fields, Some(v)),
                        _ => Type::Record(new_fields, Some(r.clone())),
                    },
                },
            }
        }
        Type::Union(tags, row) => {
            let new_tags: Vec<(String, Type)> = tags.iter()
                .map(|(k, v)| (k.clone(), apply_subst(s, v)))
                .collect();
            match row {
                None => Type::Union(new_tags, None),
                Some(r) => match s.get(r) {
                    None => Type::Union(new_tags, Some(r.clone())),
                    Some(bound) => match apply_subst(s, bound) {
                        Type::Union(extra, extra_row) => {
                            let mut merged = new_tags;
                            for (k, v) in extra {
                                if !merged.iter().any(|(mk, _)| mk == &k) {
                                    merged.push((k, v));
                                }
                            }
                            merged.sort_by(|a, b| a.0.cmp(&b.0));
                            Type::Union(merged, extra_row)
                        }
                        Type::Var(v) => Type::Union(new_tags, Some(v)),
                        _ => Type::Union(new_tags, Some(r.clone())),
                    },
                },
            }
        }
    }
}

fn apply_subst_scheme(s: &Subst, scheme: &Scheme) -> Scheme {
    let mut s = s.clone();
    for v in &scheme.vars { s.remove(v); }
    Scheme { vars: scheme.vars.clone(), ty: apply_subst(&s, &scheme.ty) }
}

fn apply_subst_env(s: &Subst, env: &TypeEnv) -> TypeEnv {
    env.iter().map(|(k, v)| (k.clone(), apply_subst_scheme(s, v))).collect()
}

/// s1 ∘ s2: apply s1 to the range of s2, then union.
fn compose(s1: &Subst, s2: &Subst) -> Subst {
    let mut out: Subst = s2.iter().map(|(k, v)| (k.clone(), apply_subst(s1, v))).collect();
    out.extend(s1.clone());
    out
}

// ── Free type variables ───────────────────────────────────────────────────────

fn ftv(ty: &Type) -> HashSet<String> {
    match ty {
        Type::Var(n) => HashSet::from([n.clone()]),
        Type::Con(_, args) => args.iter().flat_map(ftv).collect(),
        Type::Record(fields, row) => {
            let mut vars: HashSet<String> = fields.iter().flat_map(|(_, v)| ftv(v)).collect();
            if let Some(r) = row { vars.insert(r.clone()); }
            vars
        }
        Type::Union(tags, row) => {
            let mut vars: HashSet<String> = tags.iter().flat_map(|(_, v)| ftv(v)).collect();
            if let Some(r) = row { vars.insert(r.clone()); }
            vars
        }
    }
}

fn ftv_scheme(s: &Scheme) -> HashSet<String> {
    ftv(&s.ty).into_iter().filter(|v| !s.vars.contains(v)).collect()
}

fn ftv_env(env: &TypeEnv) -> HashSet<String> {
    env.values().flat_map(ftv_scheme).collect()
}

// ── Type environment ──────────────────────────────────────────────────────────

pub type TypeEnv = HashMap<String, Scheme>;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TypeError {
    UnificationFail(Type, Type),
    InfiniteType(String, Type),
    UnboundVar(String),
    NotARecord(Type),
    NoSuchField(String, Type),
    NotAUnion(Type),
    /// `otherwise` branch is dead: scrutinee is a closed union with all tags covered.
    DeadOtherwise,
    ModuleError(String),
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::UnificationFail(a, b) => write!(f, "cannot unify {} with {}", a, b),
            TypeError::InfiniteType(v, t) => write!(f, "infinite type: {} ~ {}", v, t),
            TypeError::UnboundVar(n) => write!(f, "unbound variable: {}", n),
            TypeError::NotARecord(t) => write!(f, "expected record, got {}", t),
            TypeError::NoSuchField(name, t) => write!(f, "no field `{}` in {}", name, t),
            TypeError::NotAUnion(t) => write!(f, "expected union, got {}", t),
            TypeError::DeadOtherwise => write!(f, "dead `otherwise`: scrutinee is a closed union"),
            TypeError::ModuleError(msg) => write!(f, "module error: {}", msg),
        }
    }
}

// ── Unification ───────────────────────────────────────────────────────────────

fn unify(ctx: &mut Ctx, t1: &Type, t2: &Type) -> Result<Subst, TypeError> {
    match (t1, t2) {
        (Type::Var(a), Type::Var(b)) if a == b => Ok(Subst::new()),
        (Type::Var(a), t) | (t, Type::Var(a)) => bind(a, t),
        (Type::Con(n1, args1), Type::Con(n2, args2)) if n1 == n2 && args1.len() == args2.len() => {
            args1.iter().zip(args2).try_fold(Subst::new(), |s, (a, b)| {
                let s2 = unify(ctx, &apply_subst(&s, a), &apply_subst(&s, b))?;
                Ok(compose(&s2, &s))
            })
        }
        (Type::Record(f1, row1), Type::Record(f2, row2)) => {
            unify_rows(ctx, f1, row1, f2, row2, |fields, row| Type::Record(fields, row), t1, t2)
        }
        (Type::Union(t1_tags, row1), Type::Union(t2_tags, row2)) => {
            unify_rows(ctx, t1_tags, row1, t2_tags, row2, |tags, row| Type::Union(tags, row), t1, t2)
        }
        _ => Err(TypeError::UnificationFail(t1.clone(), t2.clone())),
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
            // Both sides have unique elements — bind each row to the other's unique
            // elements plus a *shared* fresh row, avoiding circular bindings.
            let shared = ctx.fresh_name();
            match row2 {
                None => return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone())),
                Some(r2) => {
                    let su = bind(r2, &make(only1.clone(), Some(shared.clone())))?;
                    s = compose(&su, &s);
                }
            }
            match row1 {
                None => return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone())),
                Some(r1) => {
                    let su = bind(r1, &make(only2.clone(), Some(shared)))?;
                    s = compose(&su, &s);
                }
            }
        }
        (true, false) => match row2 {
            None => return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone())),
            Some(r2) => {
                let su = bind(r2, &make(only1.clone(), row1.clone()))?;
                s = compose(&su, &s);
            }
        },
        (false, true) => match row1 {
            None => return Err(TypeError::UnificationFail(orig1.clone(), orig2.clone())),
            Some(r1) => {
                let rest = apply_subst(&s, &make(only2.clone(), row2.clone()));
                let su = bind(r1, &rest)?;
                s = compose(&su, &s);
            }
        },
        (false, false) => {
            let su = match (row1, row2) {
                (Some(r1), Some(r2)) => unify(ctx, &Type::Var(r1.clone()), &Type::Var(r2.clone()))?,
                (Some(r1), None) => bind(r1, &make(vec![], None))?,
                (None, Some(r2)) => bind(r2, &make(vec![], None))?,
                (None, None) => Subst::new(),
            };
            s = compose(&su, &s);
        }
    }

    Ok(s)
}

fn bind(var: &str, ty: &Type) -> Result<Subst, TypeError> {
    if let Type::Var(v) = ty {
        if v == var { return Ok(Subst::new()); }
    }
    if ftv(ty).contains(var) {
        return Err(TypeError::InfiniteType(var.into(), ty.clone()));
    }
    Ok(Subst::from([(var.into(), ty.clone())]))
}

// ── Inference context (fresh variable supply) ─────────────────────────────────

struct Ctx { counter: usize }

impl Ctx {
    fn new() -> Self { Ctx { counter: 0 } }

    fn fresh_name(&mut self) -> String {
        let n = self.counter;
        self.counter += 1;
        format!("t{}", n)
    }

    fn fresh(&mut self) -> Type {
        Type::Var(self.fresh_name())
    }

    fn instantiate(&mut self, scheme: &Scheme) -> Type {
        let subs: Subst = scheme.vars.iter()
            .map(|v| (v.clone(), self.fresh()))
            .collect();
        apply_subst(&subs, &scheme.ty)
    }
}

/// Convert a surface `TypeExpr` to an internal `Type`.
/// `counter` generates unique row variable names for anonymous open rows.
pub fn type_expr_to_type(te: &TypeExpr, counter: &mut usize) -> Type {
    match te {
        TypeExpr::Var(v) => Type::Var(v.clone()),
        TypeExpr::Con(n) => Type::Con(n.clone(), vec![]),
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
            Type::Record(fs, r)
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
            Type::Union(ts, r)
        }
    }
}

fn generalize(env: &TypeEnv, ty: &Type) -> Scheme {
    let vars: Vec<String> = ftv(ty).difference(&ftv_env(env)).cloned().collect();
    Scheme { vars, ty: ty.clone() }
}

// ── Algorithm W ───────────────────────────────────────────────────────────────

fn infer_inner(ctx: &mut Ctx, env: &TypeEnv, expr: &Expr) -> Result<(Subst, Type), TypeError> {
    match expr {
        Expr::Lit(Lit::Int(_)) => Ok((Subst::new(), Type::int())),
        Expr::Lit(Lit::Str(_)) => Ok((Subst::new(), Type::str_())),

        Expr::Var(name) => {
            let scheme = env.get(name)
                .ok_or_else(|| TypeError::UnboundVar(name.clone()))?;
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

            let expected = if arg_tys.is_empty() {
                apply_subst(&s, &ret)
            } else {
                let arg_tys_subst: Vec<Type> = arg_tys.iter().map(|at| apply_subst(&s, at)).collect();
                Type::fun(arg_tys_subst, apply_subst(&s, &ret))
            };

            let su = unify(ctx, &apply_subst(&s, &f_ty), &expected)?;
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
            Ok((s, Type::Record(typed_fields, None)))
        }

        Expr::Field(expr, name) => {
            let (s, expr_ty) = infer_inner(ctx, env, expr)?;
            let ty = apply_subst(&s, &expr_ty);
            match ty {
                Type::Var(v) => {
                    let field_ty = ctx.fresh();
                    let row = ctx.fresh_name();
                    let record_ty = Type::Record(vec![(name.clone(), field_ty.clone())], Some(row));
                    let su = bind(&v, &record_ty)?;
                    let s_final = compose(&su, &s);
                    Ok((s_final, field_ty))
                }
                Type::Record(fields, row) => {
                    match fields.iter().find(|(k, _)| k == name) {
                        Some((_, field_ty)) => Ok((s, field_ty.clone())),
                        None => match row {
                            Some(r) => {
                                let field_ty = ctx.fresh();
                                let new_row = ctx.fresh_name();
                                let extension = Type::Record(
                                    vec![(name.clone(), field_ty.clone())],
                                    Some(new_row),
                                );
                                let su = bind(&r, &extension)?;
                                let s_final = compose(&su, &s);
                                Ok((s_final, field_ty))
                            }
                            None => Err(TypeError::NoSuchField(
                                name.clone(),
                                Type::Record(fields, None),
                            )),
                        },
                    }
                }
                other => Err(TypeError::NotARecord(other)),
            }
        }

        Expr::Tag(name, payload) => {
            // Tags with no payload desugar to unit payload.
            let (s, payload_ty) = match payload {
                Some(e) => infer_inner(ctx, env, e)?,
                None => (Subst::new(), Type::unit()),
            };
            // Tag construction yields an open union — the row var allows the value
            // to be used where more tags are expected (e.g. `f(Some 5)` where
            // f expects `[Some Int, None | *]`).
            let row = ctx.fresh_name();
            Ok((s, Type::Union(vec![(name.clone(), payload_ty)], Some(row))))
        }

        Expr::When(scrutinee, branches, otherwise) => {
            let (s0, scrut_ty) = infer_inner(ctx, env, scrutinee)?;
            let mut s = s0;
            let ret = ctx.fresh();

            // Assign a fresh payload type to each branch tag.
            let payload_tys: Vec<Type> = branches.iter().map(|_| ctx.fresh()).collect();
            let branch_tags: Vec<(String, Type)> = branches.iter()
                .zip(&payload_tys)
                .map(|((tag, _, _), pty)| (tag.clone(), pty.clone()))
                .collect();

            // Build the union type for the scrutinee.
            // With `otherwise`: open (has row var). Without: closed.
            let scrut_union = if otherwise.is_some() {
                Type::Union(branch_tags, Some(ctx.fresh_name()))
            } else {
                Type::Union(branch_tags, None)
            };

            // Unify scrutinee with expected union shape.
            let scrut_current = apply_subst(&s, &scrut_ty);
            let su = unify(ctx, &scrut_current, &scrut_union)?;
            s = compose(&su, &s);

            // Dead `otherwise` check: after unification, if `otherwise` was present
            // but the union is now closed (row var resolved to empty), it's dead.
            if otherwise.is_some() {
                let scrut_final = apply_subst(&s, &scrut_ty);
                if let Type::Union(_, None) = scrut_final {
                    return Err(TypeError::DeadOtherwise);
                }
            }

            // Infer each branch body.
            for ((_, binding, body), payload_ty) in branches.iter().zip(&payload_tys) {
                let mut env2 = apply_subst_env(&s, env);
                if let Some(b) = binding {
                    env2.insert(b.clone(), Scheme::mono(apply_subst(&s, payload_ty)));
                } else {
                    // No binding → payload is unit (tag written without payload).
                    let su = unify(ctx, &apply_subst(&s, payload_ty), &Type::unit())?;
                    s = compose(&su, &s);
                }
                let (sb, body_ty) = infer_inner(ctx, &env2, body)?;
                s = compose(&sb, &s);
                let su = unify(ctx, &apply_subst(&s, &body_ty), &apply_subst(&s, &ret))?;
                s = compose(&su, &s);
            }

            // Infer `otherwise` body.
            if let Some(otherwise_body) = otherwise {
                let env2 = apply_subst_env(&s, env);
                let (sb, body_ty) = infer_inner(ctx, &env2, otherwise_body)?;
                s = compose(&sb, &s);
                let su = unify(ctx, &apply_subst(&s, &body_ty), &apply_subst(&s, &ret))?;
                s = compose(&su, &s);
            }

            Ok((s.clone(), apply_subst(&s, &ret)))
        }

        Expr::Import(path) => {
            let ty = crate::module::load_module(path)
                .map_err(TypeError::ModuleError)?.ty;
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
                        // If this name was previously annotated, enforce the annotation.
                        let s2 = if let Some(existing) = env3.get(name) {
                            let existing_ty = ctx.instantiate(existing);
                            compose(&unify(ctx, &apply_subst(&s1, &ty), &existing_ty)?, &s1)
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
                            let s1 = unify(ctx, &existing_ty, &ann_ty)?;
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
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Normalize a type's variable names (t0,t1,… → a,b,…). Public for REPL display.
pub fn normalize_ty(ty: Type) -> Type { normalize(ty) }

pub fn infer(env: &TypeEnv, expr: &Expr) -> Result<Type, TypeError> {
    let mut ctx = Ctx::new();
    let (s, ty) = infer_inner(&mut ctx, env, expr)?;
    Ok(normalize(apply_subst(&s, &ty)))
}

/// Infer and return the generalised scheme — used by the REPL.
pub fn infer_scheme(env: &TypeEnv, expr: &Expr) -> Result<Scheme, TypeError> {
    let mut ctx = Ctx::new();
    let (s, ty) = infer_inner(&mut ctx, env, expr)?;
    let ty = apply_subst(&s, &ty);
    let env2 = apply_subst_env(&s, env);
    Ok(generalize(&env2, &ty))
}

/// Unify `ty` against any existing scheme for `name` in `env`.
/// Returns `(subst, resolved_ty)` — `ty` after the unification substitution is applied.
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

/// Check a new binding against any existing annotation for `name`.
/// Returns the (possibly more constrained) scheme, or a type error if incompatible.
pub fn enforce_binding(env: &TypeEnv, name: &str, inferred: Scheme) -> Result<Scheme, TypeError> {
    let mut ctx = Ctx::new();
    let inferred_ty = ctx.instantiate(&inferred);
    let (s, resolved) = constrain_against_existing(&mut ctx, env, name, &inferred_ty)?;
    Ok(generalize(&apply_subst_env(&s, env), &resolved))
}

/// Apply a type annotation for `name` against the current env.
/// If `name` already has a type, unify the annotation with it (error if incompatible).
/// Returns the resulting scheme to insert into the env.
pub fn apply_ann(env: &TypeEnv, name: &str, te: &TypeExpr) -> Result<Scheme, TypeError> {
    let mut ctx = Ctx::new();
    let ann_ty = type_expr_to_type(te, &mut 0usize);
    let (s, resolved) = constrain_against_existing(&mut ctx, env, name, &ann_ty)?;
    Ok(generalize(&apply_subst_env(&s, env), &resolved))
}

/// Standard type environment matching std_env() in eval.
pub fn std_type_env() -> TypeEnv {
    let mut env = TypeEnv::new();
    env.insert("++".into(), Scheme::mono(
        Type::fun(vec![Type::str_(), Type::str_()], Type::str_())
    ));
    env.insert("+".into(), Scheme::mono(
        Type::fun(vec![Type::int(), Type::int()], Type::int())
    ));

    // Task helpers.
    let task = |ok: Type, err: Type| Type::Con("Task".into(), vec![ok, err]);
    let tv = |n: &str| Type::Var(n.into());

    // ok : ∀a e. a -> Task a e
    env.insert("ok".into(), Scheme {
        vars: vec!["a".into(), "e".into()],
        ty: Type::fun(vec![tv("a")], task(tv("a"), tv("e"))),
    });

    // then : ∀a b e. Task a e -> (a -> Task b e) -> Task b e
    env.insert("then".into(), Scheme {
        vars: vec!["a".into(), "b".into(), "e".into()],
        ty: Type::fun(
            vec![
                task(tv("a"), tv("e")),
                Type::fun(vec![tv("a")], task(tv("b"), tv("e"))),
            ],
            task(tv("b"), tv("e")),
        ),
    });

    // fail : ∀a e. e -> Task a e
    env.insert("fail".into(), Scheme {
        vars: vec!["a".into(), "e".into()],
        ty: Type::fun(vec![tv("e")], task(tv("a"), tv("e"))),
    });

    // print : ∀r. Str -> Task {} [IoErr Str | r]
    env.insert("print".into(), Scheme {
        vars: vec!["r".into()],
        ty: Type::fun(
            vec![Type::str_()],
            task(Type::unit(), Type::Union(vec![("IoErr".into(), Type::str_())], Some("r".into()))),
        ),
    });

    // read_line : ∀r. Task Str [IoErr Str | r]
    env.insert("read_line".into(), Scheme {
        vars: vec!["r".into()],
        ty: task(Type::str_(), Type::Union(vec![("IoErr".into(), Type::str_())], Some("r".into()))),
    });

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
        Type::Con(name, args) => Type::Con(
            name.clone(),
            args.iter().map(|a| normalize_inner(a, map, n)).collect(),
        ),
        Type::Record(fields, row) => {
            let new_fields = fields.iter()
                .map(|(k, v)| (k.clone(), normalize_inner(v, map, n)))
                .collect();
            let new_row = row.as_ref().map(|r| {
                map.entry(r.clone()).or_insert_with(|| {
                    let name = var_name(*n); *n += 1; name
                }).clone()
            });
            Type::Record(new_fields, new_row)
        }
        Type::Union(tags, row) => {
            let mut new_tags: Vec<(String, Type)> = tags.iter()
                .map(|(k, v)| (k.clone(), normalize_inner(v, map, n)))
                .collect();
            new_tags.sort_by(|a, b| a.0.cmp(&b.0));
            let new_row = row.as_ref().map(|r| {
                map.entry(r.clone()).or_insert_with(|| {
                    let name = var_name(*n); *n += 1; name
                }).clone()
            });
            Type::Union(new_tags, new_row)
        }
    }
}

fn var_name(n: usize) -> String {
    if n < 26 { char::from(b'a' + n as u8).to_string() }
    else { format!("t{}", n) }
}
