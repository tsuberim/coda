use std::collections::{HashMap, HashSet};

use crate::ast::{BlockItem, Expr, Lit};

// ── Compiler state ────────────────────────────────────────────────────────────

struct Compiler {
    counter: usize,
    string_consts: Vec<(String, String)>, // (global_name, content)
    top_fns: Vec<String>,
}

impl Compiler {
    fn new() -> Self {
        Compiler { counter: 0, string_consts: Vec::new(), top_fns: Vec::new() }
    }

    fn next(&mut self) -> usize {
        let n = self.counter;
        self.counter += 1;
        n
    }

    fn ssa(&mut self, prefix: &str) -> String {
        format!("%{}_{}", prefix, self.next())
    }

    fn label(&mut self, prefix: &str) -> String {
        format!("{}_{}", prefix, self.next())
    }

    /// Return global name for a string constant (deduplicated).
    fn string_const(&mut self, s: &str) -> String {
        for (name, content) in &self.string_consts {
            if content == s {
                return name.clone();
            }
        }
        let name = format!("@str_{}", self.string_consts.len());
        self.string_consts.push((name.clone(), s.to_string()));
        name
    }
}

// ── Function builder ─────────────────────────────────────────────────────────

struct FnBuilder {
    done: Vec<(String, Vec<String>)>, // finished blocks
    cur_label: String,
    cur_instrs: Vec<String>,
    owned: HashSet<String>,
}

impl FnBuilder {
    fn new() -> Self {
        FnBuilder {
            done: Vec::new(),
            cur_label: "entry".into(),
            cur_instrs: Vec::new(),
            owned: HashSet::new(),
        }
    }

    fn cur_label(&self) -> &str {
        &self.cur_label
    }

    fn emit(&mut self, s: impl Into<String>) {
        self.cur_instrs.push(s.into());
    }

    /// Seal the current block and start a new one with the given label.
    fn next_block(&mut self, label: impl Into<String>) {
        let old_instrs = std::mem::take(&mut self.cur_instrs);
        let old_label = std::mem::replace(&mut self.cur_label, label.into());
        self.done.push((old_label, old_instrs));
        self.owned = HashSet::new();
    }

    fn emit_br(&mut self, label: &str) {
        self.emit(format!("br label %{}", label));
    }

    fn emit_condbr(&mut self, cond: &str, t: &str, f: &str) {
        self.emit(format!("br i1 {}, label %{}, label %{}", cond, t, f));
    }

    fn emit_ret(&mut self, val: &str) {
        self.emit(format!("ret ptr {}", val));
    }

    fn own(&mut self, ssa: &str) {
        self.owned.insert(ssa.to_string());
    }

    fn is_owned(&self, ssa: &str) -> bool {
        self.owned.contains(ssa)
    }

    fn release_if_owned(&mut self, ssa: &str) {
        if self.owned.remove(ssa) {
            self.emit(format!("call void @coda_release(ptr {})", ssa));
        }
    }

    /// Ensure we own `export` (retain if borrowed), release all other owned values,
    /// then disown `export` (transferring ownership out).
    fn prepare_export(&mut self, export: &str) {
        if !self.owned.contains(export) {
            self.emit(format!("call void @coda_retain(ptr {})", export));
            self.owned.insert(export.to_string());
        }
        let to_release: Vec<String> = self.owned.iter()
            .filter(|v| v.as_str() != export)
            .cloned()
            .collect();
        for v in &to_release {
            self.emit(format!("call void @coda_release(ptr {})", v));
        }
        self.owned = HashSet::new();
        // export is transferred, not in owned anymore
    }

    fn to_ir(mut self) -> String {
        // Seal the last block.
        let last_instrs = std::mem::take(&mut self.cur_instrs);
        self.done.push((self.cur_label.clone(), last_instrs));

        let mut out = String::new();
        for (label, instrs) in &self.done {
            out.push_str(&format!("{}:\n", label));
            for i in instrs {
                out.push_str(&format!("  {}\n", i));
            }
        }
        out
    }
}

// ── Free variable analysis ────────────────────────────────────────────────────

fn free_vars(expr: &Expr, bound: &HashSet<String>) -> HashSet<String> {
    match expr {
        Expr::Var(n) => {
            if bound.contains(n) { HashSet::new() } else { [n.clone()].into_iter().collect() }
        }
        Expr::Lam(params, body) => {
            let mut b2 = bound.clone();
            for p in params { b2.insert(p.clone()); }
            free_vars(body, &b2)
        }
        Expr::App(f, args) => {
            let mut fvs = free_vars(f, bound);
            for a in args { fvs.extend(free_vars(a, bound)); }
            fvs
        }
        Expr::Block(items, body) => {
            let mut b2 = bound.clone();
            let mut fvs = HashSet::new();
            for item in items {
                match item {
                    BlockItem::Bind(name, e) => {
                        fvs.extend(free_vars(e, &b2));
                        b2.insert(name.clone());
                    }
                    BlockItem::Ann(_, _) => {}
                    BlockItem::MonadicBind(_, _) => unreachable!(),
                }
            }
            fvs.extend(free_vars(body, &b2));
            fvs
        }
        Expr::Record(fields) => fields.iter().flat_map(|(_, e)| free_vars(e, bound)).collect(),
        Expr::Field(e, _) => free_vars(e, bound),
        Expr::Tag(_, payload) => {
            payload.as_ref().map_or_else(HashSet::new, |e| free_vars(e, bound))
        }
        Expr::When(scrut, branches, otherwise) => {
            let mut fvs = free_vars(scrut, bound);
            for (_, binding, body) in branches {
                let mut b2 = bound.clone();
                if let Some(b) = binding { b2.insert(b.clone()); }
                fvs.extend(free_vars(body, &b2));
            }
            if let Some(body) = otherwise { fvs.extend(free_vars(body, bound)); }
            fvs
        }
        Expr::List(elems) => elems.iter().flat_map(|e| free_vars(e, bound)).collect(),
        Expr::Import(_) => HashSet::new(),
        Expr::Lit(_) => HashSet::new(),
    }
}

// ── Expression compiler ───────────────────────────────────────────────────────

type Env = HashMap<String, String>; // source name -> SSA name

fn compile_expr(
    c: &mut Compiler,
    fb: &mut FnBuilder,
    env: &Env,
    expr: &Expr,
) -> Result<String, String> {
    match expr {
        Expr::Lit(Lit::Int(n)) => {
            let r = c.ssa("int");
            fb.emit(format!("{} = call ptr @coda_mk_int(i64 {})", r, n));
            fb.own(&r);
            Ok(r)
        }

        Expr::Lit(Lit::Str(s)) => {
            let g = c.string_const(s);
            let r = c.ssa("str");
            fb.emit(format!("{} = call ptr @coda_mk_str(ptr {})", r, g));
            fb.own(&r);
            Ok(r)
        }

        Expr::Var(name) => env.get(name).cloned().ok_or_else(|| format!("unbound: {}", name)),

        Expr::Lam(params, body) => compile_lam(c, fb, env, params, body),

        Expr::App(f, args) => {
            let f_ssa = compile_expr(c, fb, env, f)?;
            let arg_ssas: Vec<String> =
                args.iter().map(|a| compile_expr(c, fb, env, a)).collect::<Result<_, _>>()?;
            emit_apply(c, fb, &f_ssa, &arg_ssas)
        }

        Expr::Block(items, body) => {
            let mut env2 = env.clone();
            for item in items {
                match item {
                    BlockItem::Bind(name, e) => {
                        let ssa = compile_expr(c, fb, &env2, e)?;
                        env2.insert(name.clone(), ssa);
                    }
                    BlockItem::Ann(_, _) => {}
                    BlockItem::MonadicBind(_, _) => unreachable!(),
                }
            }
            compile_expr(c, fb, &env2, body)
        }

        Expr::Record(fields) => {
            let mut key_gs = Vec::new();
            let mut val_ssas = Vec::new();
            for (k, e) in fields {
                key_gs.push(c.string_const(k));
                val_ssas.push(compile_expr(c, fb, env, e)?);
            }
            emit_record(c, fb, &key_gs, &val_ssas)
        }

        Expr::Field(rec_expr, field) => {
            let rec_ssa = compile_expr(c, fb, env, rec_expr)?;
            let field_g = c.string_const(field);
            let r = c.ssa("field");
            fb.emit(format!("{} = call ptr @coda_field_get(ptr {}, ptr {})", r, rec_ssa, field_g));
            // result is BORROWED from the record — do NOT own it
            Ok(r)
        }

        Expr::Tag(name, payload) => {
            let name_g = c.string_const(name);
            let payload_ssa = match payload {
                Some(e) => compile_expr(c, fb, env, e)?,
                None => {
                    let r = c.ssa("unit");
                    fb.emit(format!("{} = call ptr @coda_mk_unit()", r));
                    fb.own(&r);
                    r
                }
            };
            let r = c.ssa("tag");
            fb.emit(format!("{} = call ptr @coda_mk_tag(ptr {}, ptr {})", r, name_g, payload_ssa));
            fb.own(&r);
            // mk_tag retained payload; release our local ref
            fb.release_if_owned(&payload_ssa);
            Ok(r)
        }

        Expr::When(scrutinee, branches, otherwise) => {
            compile_when(c, fb, env, scrutinee, branches, otherwise)
        }

        Expr::List(elems) => {
            let ssas: Vec<String> =
                elems.iter().map(|e| compile_expr(c, fb, env, e)).collect::<Result<_, _>>()?;
            emit_list(c, fb, &ssas)
        }

        Expr::Import(_) => Err("imports not supported in compiled mode".into()),
    }
}

fn compile_lam(
    c: &mut Compiler,
    fb: &mut FnBuilder,
    env: &Env,
    params: &[String],
    body: &Expr,
) -> Result<String, String> {
    // Compute free variables: used in body but not bound by params, filtered to what's in env.
    let param_set: HashSet<String> = params.iter().cloned().collect();
    let fvs_all = free_vars(body, &param_set);
    let mut fvs: Vec<String> = fvs_all.into_iter().filter(|v| env.contains_key(v)).collect();
    fvs.sort(); // deterministic order

    let lam_name = format!("lam_{}", c.next());

    // Build the lambda function body.
    let mut lam_fb = FnBuilder::new();
    let mut lam_env: Env = HashMap::new();

    // Load each capture from %caps — borrowed, do NOT retain or own.
    for (i, fv) in fvs.iter().enumerate() {
        let p = format!("%cp_{}", i);
        let ssa = format!("%cap_{}", i);
        lam_fb.emit(format!("{} = getelementptr ptr, ptr %caps, i32 {}", p, i));
        lam_fb.emit(format!("{} = load ptr, ptr {}", ssa, p));
        lam_env.insert(fv.clone(), ssa);
    }

    // Load each param from %args — retain and own them.
    for (i, param) in params.iter().enumerate() {
        let p = format!("%ap_{}", i);
        let ssa = format!("%arg_{}", i);
        lam_fb.emit(format!("{} = getelementptr ptr, ptr %args, i32 {}", p, i));
        lam_fb.emit(format!("{} = load ptr, ptr {}", ssa, p));
        lam_fb.emit(format!("call void @coda_retain(ptr {})", ssa));
        lam_fb.own(&ssa);
        lam_env.insert(param.clone(), ssa);
    }

    let result = compile_expr(c, &mut lam_fb, &lam_env, body)?;
    lam_fb.prepare_export(&result);
    lam_fb.emit_ret(&result);

    let body_ir = lam_fb.to_ir();
    c.top_fns.push(format!(
        "define ptr @{}(ptr %caps, ptr %args, i32 %nargs) {{\n{}}}",
        lam_name, body_ir
    ));

    // Emit closure creation at the call site.
    let r = c.ssa("clos");
    if fvs.is_empty() {
        fb.emit(format!(
            "{} = call ptr @coda_mk_closure(ptr @{}, ptr null, i32 0)",
            r, lam_name
        ));
    } else {
        let n = fvs.len();
        let caps_arr = c.ssa("caps");
        fb.emit(format!("{} = alloca [{} x ptr]", caps_arr, n));
        for (i, fv) in fvs.iter().enumerate() {
            let gep = c.ssa("cgep");
            fb.emit(format!(
                "{} = getelementptr [{} x ptr], ptr {}, i32 0, i32 {}",
                gep, n, caps_arr, i
            ));
            fb.emit(format!("store ptr {}, ptr {}", env[fv], gep));
        }
        fb.emit(format!(
            "{} = call ptr @coda_mk_closure(ptr @{}, ptr {}, i32 {})",
            r, lam_name, caps_arr, n
        ));
        // Closure retains each cap; release our local ref for each cap that we own
        for fv in &fvs {
            fb.release_if_owned(&env[fv]);
        }
    }
    fb.own(&r);
    Ok(r)
}

fn compile_when(
    c: &mut Compiler,
    fb: &mut FnBuilder,
    env: &Env,
    scrutinee: &Expr,
    branches: &[(String, Option<String>, Box<Expr>)],
    otherwise: &Option<Box<Expr>>,
) -> Result<String, String> {
    let scrut = compile_expr(c, fb, env, scrutinee)?;
    let scrut_was_owned = fb.is_owned(&scrut);
    // Remove scrutinee from owned set — we'll release it manually at the merge block
    if scrut_was_owned {
        fb.owned.remove(&scrut);
    }

    let tn = c.ssa("tn");
    fb.emit(format!("{} = call ptr @coda_tag_name(ptr {})", tn, scrut));

    let merge = c.label("merge");

    // Pre-allocate all block labels.
    let branch_labels: Vec<String> = (0..branches.len()).map(|_| c.label("branch")).collect();
    let check_labels: Vec<String> =
        (0..branches.len().saturating_sub(1)).map(|_| c.label("check")).collect();
    let otherwise_label = otherwise.as_ref().map(|_| c.label("otherwise"));

    // When there's no `otherwise`, the non-matching path is unreachable (guaranteed by the
    // type system). We still need a valid target for LLVM's CFG, so we use an `unreachable`
    // block.
    let no_match_label = if otherwise_label.is_none() { Some(c.label("no_match")) } else { None };

    // Determine the "miss" target for the first branch.
    let first_miss = if branches.len() > 1 {
        check_labels[0].clone()
    } else {
        otherwise_label.clone()
            .or_else(|| no_match_label.clone())
            .unwrap_or_else(|| merge.clone())
    };

    // Emit first comparison in current block (before any branch).
    if !branches.is_empty() {
        let tag_g = c.string_const(&branches[0].0);
        let cmp = c.ssa("cmp");
        fb.emit(format!("{} = call i1 @coda_str_eq(ptr {}, ptr {})", cmp, tn, tag_g));
        fb.emit_condbr(&cmp, &branch_labels[0], &first_miss);
    } else if let Some(ref ol) = otherwise_label {
        fb.emit_br(ol);
    } else {
        fb.emit_br(&merge);
    }

    let mut phi_entries: Vec<(String, String)> = Vec::new();

    for i in 0..branches.len() {
        let (_, binding, body) = &branches[i];

        // Branch body block.
        fb.next_block(branch_labels[i].clone());
        let mut env2 = env.clone();
        if let Some(b) = binding {
            let p = c.ssa("payload");
            fb.emit(format!("{} = call ptr @coda_tag_payload(ptr {})", p, scrut));
            env2.insert(b.clone(), p);
        }
        let result = compile_expr(c, fb, &mut env2, body)?;
        let from = fb.cur_label().to_string();
        fb.prepare_export(&result);
        phi_entries.push((result, from));
        fb.emit_br(&merge);

        // Check block for next branch (if any).
        if i + 1 < branches.len() {
            fb.next_block(check_labels[i].clone());
            let tag_g = c.string_const(&branches[i + 1].0);
            let cmp = c.ssa("cmp");
            fb.emit(format!("{} = call i1 @coda_str_eq(ptr {}, ptr {})", cmp, tn, tag_g));
            let miss = if i + 2 < branches.len() {
                check_labels[i + 1].clone()
            } else {
                otherwise_label.clone()
                    .or_else(|| no_match_label.clone())
                    .unwrap_or_else(|| merge.clone())
            };
            fb.emit_condbr(&cmp, &branch_labels[i + 1], &miss);
        }
    }

    // Otherwise block.
    if let Some(body) = otherwise {
        let ol = otherwise_label.clone().unwrap();
        fb.next_block(ol);
        let result = compile_expr(c, fb, env, body)?;
        let from = fb.cur_label().to_string();
        fb.prepare_export(&result);
        phi_entries.push((result, from));
        fb.emit_br(&merge);
    }

    // No-match block (unreachable; satisfies LLVM CFG when there's no `otherwise`).
    if let Some(nm) = no_match_label {
        fb.next_block(nm);
        fb.emit("unreachable");
    }

    // Merge block with phi.
    fb.next_block(merge);
    let r = c.ssa("when");
    if phi_entries.is_empty() {
        fb.emit(format!("{} = call ptr @coda_mk_unit()", r));
        fb.own(&r);
    } else {
        let phi_args = phi_entries
            .iter()
            .map(|(v, l)| format!("[ {}, %{} ]", v, l))
            .collect::<Vec<_>>()
            .join(", ");
        fb.emit(format!("{} = phi ptr {}", r, phi_args));
        fb.own(&r);
    }

    // Release the scrutinee now that all branches are done
    if scrut_was_owned {
        fb.emit(format!("call void @coda_release(ptr {})", scrut));
    }

    Ok(r)
}

fn emit_apply(
    c: &mut Compiler,
    fb: &mut FnBuilder,
    f_ssa: &str,
    arg_ssas: &[String],
) -> Result<String, String> {
    let r = c.ssa("app");
    let n = arg_ssas.len();
    if n == 0 {
        fb.emit(format!("{} = call ptr @coda_apply(ptr {}, ptr null, i32 0)", r, f_ssa));
    } else {
        let arr = c.ssa("argarr");
        fb.emit(format!("{} = alloca [{} x ptr]", arr, n));
        for (i, arg) in arg_ssas.iter().enumerate() {
            let gep = c.ssa("agep");
            fb.emit(format!(
                "{} = getelementptr [{} x ptr], ptr {}, i32 0, i32 {}",
                gep, n, arr, i
            ));
            fb.emit(format!("store ptr {}, ptr {}", arg, gep));
        }
        fb.emit(format!("{} = call ptr @coda_apply(ptr {}, ptr {}, i32 {})", r, f_ssa, arr, n));
    }
    fb.own(&r);
    Ok(r)
}

fn emit_record(
    c: &mut Compiler,
    fb: &mut FnBuilder,
    key_gs: &[String],
    val_ssas: &[String],
) -> Result<String, String> {
    let r = c.ssa("rec");
    let n = key_gs.len();
    if n == 0 {
        fb.emit(format!("{} = call ptr @coda_mk_unit()", r));
        fb.own(&r);
        return Ok(r);
    }
    let karr = c.ssa("karr");
    let varr = c.ssa("varr");
    fb.emit(format!("{} = alloca [{} x ptr]", karr, n));
    fb.emit(format!("{} = alloca [{} x ptr]", varr, n));
    for (i, (kg, vs)) in key_gs.iter().zip(val_ssas).enumerate() {
        let kgep = c.ssa("kgep");
        let vgep = c.ssa("vgep");
        fb.emit(format!(
            "{} = getelementptr [{} x ptr], ptr {}, i32 0, i32 {}",
            kgep, n, karr, i
        ));
        fb.emit(format!("store ptr {}, ptr {}", kg, kgep));
        fb.emit(format!(
            "{} = getelementptr [{} x ptr], ptr {}, i32 0, i32 {}",
            vgep, n, varr, i
        ));
        fb.emit(format!("store ptr {}, ptr {}", vs, vgep));
    }
    fb.emit(format!(
        "{} = call ptr @coda_mk_record(ptr {}, ptr {}, i32 {})",
        r, karr, varr, n
    ));
    fb.own(&r);
    // mk_record retains each val; release our local refs
    let val_ssas_owned: Vec<String> = val_ssas.to_vec();
    for vs in &val_ssas_owned {
        fb.release_if_owned(vs);
    }
    Ok(r)
}

fn emit_list(
    c: &mut Compiler,
    fb: &mut FnBuilder,
    elem_ssas: &[String],
) -> Result<String, String> {
    let r = c.ssa("list");
    let n = elem_ssas.len();
    if n == 0 {
        fb.emit(format!("{} = call ptr @coda_mk_list(ptr null, i32 0)", r));
        fb.own(&r);
        return Ok(r);
    }
    let arr = c.ssa("larr");
    fb.emit(format!("{} = alloca [{} x ptr]", arr, n));
    for (i, es) in elem_ssas.iter().enumerate() {
        let gep = c.ssa("lgep");
        fb.emit(format!(
            "{} = getelementptr [{} x ptr], ptr {}, i32 0, i32 {}",
            gep, n, arr, i
        ));
        fb.emit(format!("store ptr {}, ptr {}", es, gep));
    }
    fb.emit(format!("{} = call ptr @coda_mk_list(ptr {}, i32 {})", r, arr, n));
    fb.own(&r);
    // mk_list retains each elem; release our local refs
    let elem_ssas_owned: Vec<String> = elem_ssas.to_vec();
    for es in &elem_ssas_owned {
        fb.release_if_owned(es);
    }
    Ok(r)
}

// ── Runtime declarations and builtin wrappers (static IR text) ────────────────

const RUNTIME_DECLS: &str = r#"; Coda runtime declarations
declare ptr @coda_mk_int(i64)
declare ptr @coda_mk_str(ptr)
declare ptr @coda_mk_closure(ptr, ptr, i32)
declare ptr @coda_mk_tag(ptr, ptr)
declare ptr @coda_mk_unit()
declare ptr @coda_mk_record(ptr, ptr, i32)
declare ptr @coda_mk_list(ptr, i32)
declare ptr @coda_apply(ptr, ptr, i32)
declare ptr @coda_field_get(ptr, ptr)
declare ptr @coda_tag_name(ptr)
declare ptr @coda_tag_payload(ptr)
declare i1  @coda_str_eq(ptr, ptr)
declare ptr @coda_add(ptr, ptr)
declare ptr @coda_sub(ptr, ptr)
declare ptr @coda_mul(ptr, ptr)
declare ptr @coda_str_concat(ptr, ptr)
declare ptr @coda_eq(ptr, ptr)
declare ptr @coda_fix(ptr)
declare ptr @coda_cons(ptr, ptr)
declare ptr @coda_append(ptr, ptr)
declare ptr @coda_head(ptr)
declare ptr @coda_tail(ptr)
declare ptr @coda_len(ptr)
declare ptr @coda_map(ptr, ptr)
declare ptr @coda_fold(ptr, ptr, ptr)
declare ptr @coda_list_of(ptr, ptr)
declare ptr @coda_list_init(ptr, ptr)
declare void @coda_retain(ptr)
declare void @coda_release(ptr)
"#;

const BUILTIN_WRAPPERS: &str = r#"; Builtin wrapper functions (closure-callable wrappers around C runtime fns)

define ptr @builtin_str_concat(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %a = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %b = load ptr, ptr %p1
  %r = call ptr @coda_str_concat(ptr %a, ptr %b)
  ret ptr %r
}

define ptr @builtin_add(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %a = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %b = load ptr, ptr %p1
  %r = call ptr @coda_add(ptr %a, ptr %b)
  ret ptr %r
}

define ptr @builtin_sub(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %a = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %b = load ptr, ptr %p1
  %r = call ptr @coda_sub(ptr %a, ptr %b)
  ret ptr %r
}

define ptr @builtin_mul(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %a = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %b = load ptr, ptr %p1
  %r = call ptr @coda_mul(ptr %a, ptr %b)
  ret ptr %r
}

define ptr @builtin_eq(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %a = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %b = load ptr, ptr %p1
  %r = call ptr @coda_eq(ptr %a, ptr %b)
  ret ptr %r
}

define ptr @builtin_fix(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %f = load ptr, ptr %p0
  %r = call ptr @coda_fix(ptr %f)
  ret ptr %r
}

define ptr @builtin_cons(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %x = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %xs = load ptr, ptr %p1
  %r = call ptr @coda_cons(ptr %x, ptr %xs)
  ret ptr %r
}

define ptr @builtin_append(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %xs = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %ys = load ptr, ptr %p1
  %r = call ptr @coda_append(ptr %xs, ptr %ys)
  ret ptr %r
}

define ptr @builtin_head(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %xs = load ptr, ptr %p0
  %r = call ptr @coda_head(ptr %xs)
  ret ptr %r
}

define ptr @builtin_tail(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %xs = load ptr, ptr %p0
  %r = call ptr @coda_tail(ptr %xs)
  ret ptr %r
}

define ptr @builtin_len(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %xs = load ptr, ptr %p0
  %r = call ptr @coda_len(ptr %xs)
  ret ptr %r
}

define ptr @builtin_map(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %f = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %xs = load ptr, ptr %p1
  %r = call ptr @coda_map(ptr %f, ptr %xs)
  ret ptr %r
}

define ptr @builtin_fold(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %f = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %init = load ptr, ptr %p1
  %p2 = getelementptr ptr, ptr %args, i32 2
  %xs = load ptr, ptr %p2
  %r = call ptr @coda_fold(ptr %f, ptr %init, ptr %xs)
  ret ptr %r
}

define ptr @builtin_list_of(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %n = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %v = load ptr, ptr %p1
  %r = call ptr @coda_list_of(ptr %n, ptr %v)
  ret ptr %r
}

define ptr @builtin_list_init(ptr %caps, ptr %args, i32 %nargs) {
entry:
  %p0 = getelementptr ptr, ptr %args, i32 0
  %n = load ptr, ptr %p0
  %p1 = getelementptr ptr, ptr %args, i32 1
  %f = load ptr, ptr %p1
  %r = call ptr @coda_list_init(ptr %n, ptr %f)
  ret ptr %r
}
"#;

// ── String constant emission ──────────────────────────────────────────────────

fn llvm_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\22"),
            b'\n' => out.push_str("\\0A"),
            b'\r' => out.push_str("\\0D"),
            b'\t' => out.push_str("\\09"),
            b if b >= 0x20 && b < 0x7f => out.push(b as char),
            b => out.push_str(&format!("\\{:02X}", b)),
        }
    }
    out.push_str("\\00");
    out
}

// ── Builtin names to wrapper function names ───────────────────────────────────

fn builtin_fn(name: &str) -> Option<&'static str> {
    match name {
        "++"     => Some("builtin_str_concat"),
        "+"      => Some("builtin_add"),
        "-"      => Some("builtin_sub"),
        "*"      => Some("builtin_mul"),
        "=="     => Some("builtin_eq"),
        "fix"    => Some("builtin_fix"),
        "::"     => Some("builtin_cons"),
        "cons"   => Some("builtin_cons"),
        "<>"     => Some("builtin_append"),
        "append" => Some("builtin_append"),
        "head"   => Some("builtin_head"),
        "tail"   => Some("builtin_tail"),
        "len"    => Some("builtin_len"),
        "map"    => Some("builtin_map"),
        "fold"   => Some("builtin_fold"),
        "list_of"   => Some("builtin_list_of"),
        "list_init" => Some("builtin_list_init"),
        _ => None,
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Compile a Coda AST expression to LLVM IR text.
/// Returns the complete `.ll` file content.
pub fn compile(expr: &Expr) -> Result<String, String> {
    let mut c = Compiler::new();
    let mut fb = FnBuilder::new();
    let mut env: Env = HashMap::new();

    // Set up standard env: create closure wrappers for all builtins.
    let builtins = [
        "++", "+", "-", "*", "==", "fix",
        "::", "cons", "<>", "append",
        "head", "tail", "len", "map", "fold",
        "list_of", "list_init",
        "then", // alias for >>=
    ];
    for name in &builtins {
        if let Some(fn_name) = builtin_fn(name) {
            let ssa = c.ssa("b");
            fb.emit(format!(
                "{} = call ptr @coda_mk_closure(ptr @{}, ptr null, i32 0)",
                ssa, fn_name
            ));
            fb.own(&ssa);
            env.insert(name.to_string(), ssa);
        }
    }

    // Compile the program body.
    let result = compile_expr(&mut c, &mut fb, &env, expr)?;
    fb.prepare_export(&result);
    fb.emit_ret(&result);

    let main_body = fb.to_ir();

    // Assemble the final IR.
    let mut out = String::new();
    out.push_str(RUNTIME_DECLS);
    out.push('\n');
    out.push_str(BUILTIN_WRAPPERS);
    out.push('\n');

    // String constants.
    for (name, content) in &c.string_consts {
        let escaped = llvm_escape(content);
        let len = content.len() + 1; // +1 for null terminator
        out.push_str(&format!(
            "{} = private constant [{} x i8] c\"{}\"\n",
            name, len, escaped
        ));
    }
    if !c.string_consts.is_empty() {
        out.push('\n');
    }

    // Lambda functions.
    for fn_ir in &c.top_fns {
        out.push_str(fn_ir);
        out.push_str("\n\n");
    }

    // Main function.
    out.push_str("define ptr @coda_main() {\n");
    out.push_str(&main_body);
    out.push_str("}\n");

    Ok(out)
}
