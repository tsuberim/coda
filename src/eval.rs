use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use colored::Colorize;

use crate::ast::*;

// ── Value ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum Value {
    Int(i64),
    Str(String),
    Record(Vec<(String, Value)>),
    /// Tag always carries a payload; unit payload = Record([]).
    Tag(String, Box<Value>),
    Closure {
        params: Vec<String>,
        body: Box<Expr>,
        env: Env,
    },
    Builtin(String, Rc<dyn Fn(Vec<Value>) -> Result<Value, EvalError>>),
    /// A suspended IO computation. Returns Ok(value) on success, Err(tag) on failure.
    Task(Rc<dyn Fn() -> Result<Value, Value>>),
    List(Vec<Value>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Record(a), Value::Record(b)) => a == b,
            (Value::Tag(t1, v1), Value::Tag(t2, v2)) => t1 == t2 && v1 == v2,
            (Value::List(a), Value::List(b)) => a == b,
            _ => false, // Closure, Builtin, Task not comparable
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Str(s) => write!(f, "{:?}", s),
            Value::Record(fields) => {
                let pairs: Vec<_> = fields.iter().map(|(k, v)| format!("{}: {:?}", k, v)).collect();
                write!(f, "{{{}}}", pairs.join(", "))
            }
            Value::Tag(tag, payload) => match payload.as_ref() {
                Value::Record(fields) if fields.is_empty() => write!(f, "{}", tag),
                p => write!(f, "{} {:?}", tag, p),
            },
            Value::Closure { params, .. } => write!(f, "<fn/{}>", params.len()),
            Value::Builtin(name, _) => write!(f, "<builtin:{}>", name),
            Value::Task(_) => write!(f, "<task>"),
            Value::List(xs) => {
                let parts: Vec<_> = xs.iter().map(|v| format!("{:?}", v)).collect();
                write!(f, "[{}]", parts.join(", "))
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Str(s) => write!(f, "{}", s),
            Value::Record(fields) => {
                let pairs: Vec<_> = fields.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
                write!(f, "{{{}}}", pairs.join(", "))
            }
            Value::Tag(tag, payload) => match payload.as_ref() {
                Value::Record(fields) if fields.is_empty() => write!(f, "{}", tag),
                p => write!(f, "{} {}", tag, p),
            },
            Value::Closure { params, .. } => write!(f, "<fn/{}>", params.len()),
            Value::Builtin(name, _) => write!(f, "<builtin:{}>", name),
            Value::Task(_) => write!(f, "<task>"),
            Value::List(xs) => {
                let parts: Vec<_> = xs.iter().map(|v| format!("{}", v)).collect();
                write!(f, "[{}]", parts.join(", "))
            }
        }
    }
}

impl Value {
    /// Colored representation for REPL output.
    pub fn pretty(&self) -> String {
        match self {
            Value::Int(n) => n.to_string().yellow().to_string(),
            Value::Str(s) => format!("{:?}", s).green().to_string(),
            Value::Record(fields) => {
                let pairs: Vec<_> = fields.iter()
                    .map(|(k, v)| format!("{}: {}", k.bright_white(), v.pretty()))
                    .collect();
                format!("{{{}}}", pairs.join(", "))
            }
            Value::Tag(tag, payload) => match payload.as_ref() {
                Value::Record(fields) if fields.is_empty() => tag.bright_yellow().to_string(),
                p => format!("{} {}", tag.bright_yellow(), p.pretty()),
            },
            Value::Closure { params, .. } => format!("<fn/{}>", params.len()).cyan().to_string(),
            Value::Builtin(name, _) => format!("<builtin:{}>", name).cyan().to_string(),
            Value::Task(_) => "<task>".cyan().to_string(),
            Value::List(xs) => {
                let parts: Vec<_> = xs.iter().map(|v| v.pretty()).collect();
                format!("[{}]", parts.join(", "))
            }
        }
    }
}

// ── Env ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Env(Rc<EnvFrame>);

#[derive(Debug)]
struct EnvFrame {
    bindings: RefCell<HashMap<String, Value>>,
    parent: Option<Env>,
}

impl Env {
    pub fn new() -> Self {
        Env(Rc::new(EnvFrame {
            bindings: RefCell::new(HashMap::new()),
            parent: None,
        }))
    }

    pub fn extend(&self) -> Self {
        Env(Rc::new(EnvFrame {
            bindings: RefCell::new(HashMap::new()),
            parent: Some(self.clone()),
        }))
    }

    pub fn set(&self, name: impl Into<String>, value: Value) {
        self.0.bindings.borrow_mut().insert(name.into(), value);
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(val) = self.0.bindings.borrow().get(name) {
            return Some(val.clone());
        }
        self.0.parent.as_ref()?.get(name)
    }
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum EvalError {
    UnboundVar(String),
    TypeMismatch { expected: &'static str, got: String },
    ArityMismatch { expected: usize, got: usize },
    NoSuchField(String),
    NoMatchingBranch(String),
    ModuleError(String),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UnboundVar(n) => write!(f, "unbound variable: {}", n),
            EvalError::TypeMismatch { expected, got } => {
                write!(f, "type error: expected {}, got {}", expected, got)
            }
            EvalError::ArityMismatch { expected, got } => {
                write!(f, "arity error: expected {} args, got {}", expected, got)
            }
            EvalError::NoSuchField(name) => write!(f, "no such field: {}", name),
            EvalError::NoMatchingBranch(tag) => write!(f, "no matching branch for tag: {}", tag),
            EvalError::ModuleError(msg) => write!(f, "module error: {}", msg),
        }
    }
}

// ── Eval ──────────────────────────────────────────────────────────────────────

pub fn eval(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Lit(lit) => Ok(match lit {
            Lit::Int(n) => Value::Int(*n),
            Lit::Str(s) => Value::Str(s.clone()),
        }),

        Expr::Var(name) => env
            .get(name)
            .ok_or_else(|| EvalError::UnboundVar(name.clone())),

        Expr::Lam(params, body) => Ok(Value::Closure {
            params: params.clone(),
            body: body.clone(),
            env: env.clone(),
        }),

        Expr::App(f, args) => {
            let f_val = eval(f, env)?;
            let arg_vals = args
                .iter()
                .map(|a| eval(a, env))
                .collect::<Result<Vec<_>, _>>()?;
            apply(f_val, arg_vals)
        }

        Expr::Record(fields) => {
            let evaled = fields.iter()
                .map(|(k, e)| eval(e, env).map(|v| (k.clone(), v)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Record(evaled))
        }

        Expr::Field(expr, name) => {
            match eval(expr, env)? {
                Value::Record(fields) => fields.into_iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v)
                    .ok_or_else(|| EvalError::NoSuchField(name.clone())),
                other => Err(EvalError::TypeMismatch {
                    expected: "record",
                    got: format!("{:?}", other),
                }),
            }
        }

        Expr::Tag(name, payload) => {
            let payload_val = match payload {
                Some(e) => eval(e, env)?,
                None => Value::Record(vec![]),  // unit
            };
            Ok(Value::Tag(name.clone(), Box::new(payload_val)))
        }

        Expr::When(scrutinee, branches, otherwise) => {
            match eval(scrutinee, env)? {
                Value::Tag(tag, payload) => {
                    for (branch_tag, binding, body) in branches {
                        if &tag == branch_tag {
                            let frame = env.extend();
                            if let Some(b) = binding {
                                frame.set(b, *payload);
                            }
                            return eval(body, &frame);
                        }
                    }
                    if let Some(otherwise_body) = otherwise {
                        return eval(otherwise_body, env);
                    }
                    Err(EvalError::NoMatchingBranch(tag))
                }
                other => Err(EvalError::TypeMismatch {
                    expected: "tag",
                    got: format!("{:?}", other),
                }),
            }
        }

        Expr::List(elems) => {
            let vals = elems.iter()
                .map(|e| eval(e, env))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(vals))
        }

        Expr::Import(path) => {
            crate::module::load_module(path)
                .map(|e| e.val)
                .map_err(EvalError::ModuleError)
        }

        Expr::Block(items, body) => {
            // Annotations are skipped — accessing an annotated-but-unbound name
            // blows up at runtime with UnboundVar.
            let mut cur = env.clone();
            for item in items {
                match item {
                    crate::ast::BlockItem::Bind(name, expr) => {
                        let val = eval(expr, &cur)?;
                        let next = cur.extend();
                        next.set(name, val);
                        cur = next;
                    }
                    crate::ast::BlockItem::Ann(_, _) => {}
                    crate::ast::BlockItem::MonadicBind(_, _) => unreachable!("desugared at parse time"),
                }
            }
            eval(body, &cur)
        }
    }
}

fn apply(f: Value, args: Vec<Value>) -> Result<Value, EvalError> {
    match f {
        Value::Closure { params, body, env } => {
            if args.len() != params.len() {
                return Err(EvalError::ArityMismatch {
                    expected: params.len(),
                    got: args.len(),
                });
            }
            let frame = env.extend();
            for (param, arg) in params.iter().zip(args) {
                frame.set(param, arg);
            }
            eval(&body, &frame)
        }
        Value::Builtin(_, f) => f(args),
        other => Err(EvalError::TypeMismatch {
            expected: "function",
            got: format!("{:?}", other),
        }),
    }
}

// ── Task execution ────────────────────────────────────────────────────────────

/// Run a Task value to completion. Returns Ok(value) or Err(error_tag).
pub fn run_task(v: &Value) -> Result<Value, Value> {
    match v {
        Value::Task(f) => f(),
        other => Err(Value::Tag(
            "RuntimeError".into(),
            Box::new(Value::Str(format!("expected task, got {}", other))),
        )),
    }
}

// ── Standard environment ──────────────────────────────────────────────────────

pub fn std_env() -> Env {
    let env = Env::new();

    // String concat — variadic, strings only.
    env.set("++", Value::Builtin("++".into(), Rc::new(|args| {
        let mut out = String::new();
        for v in args {
            match v {
                Value::Str(s) => out.push_str(&s),
                other => return Err(EvalError::TypeMismatch {
                    expected: "string",
                    got: format!("{:?}", other),
                }),
            }
        }
        Ok(Value::Str(out))
    })));

    // Integer addition.
    env.set("+", Value::Builtin("+".into(), Rc::new(|args| {
        if args.len() != 2 {
            return Err(EvalError::ArityMismatch { expected: 2, got: args.len() });
        }
        match (&args[0], &args[1]) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (a, b) => Err(EvalError::TypeMismatch {
                expected: "Int Int",
                got: format!("{:?} and {:?}", a, b),
            }),
        }
    })));

    // ok(v) — wrap a value in a successful Task.
    env.set("ok", Value::Builtin("ok".into(), Rc::new(|args| {
        if args.len() != 1 {
            return Err(EvalError::ArityMismatch { expected: 1, got: args.len() });
        }
        let v = args[0].clone();
        Ok(Value::Task(Rc::new(move || Ok(v.clone()))))
    })));

    // then(task, f) — sequence two Tasks; error type accumulates via row unification.
    env.set("then", Value::Builtin("then".into(), Rc::new(|args| {
        if args.len() != 2 {
            return Err(EvalError::ArityMismatch { expected: 2, got: args.len() });
        }
        let task = args[0].clone();
        let f = args[1].clone();
        Ok(Value::Task(Rc::new(move || {
            let v = run_task(&task)?;
            let next = apply(f.clone(), vec![v])
                .map_err(|e| Value::Tag("EvalError".into(), Box::new(Value::Str(e.to_string()))))?;
            run_task(&next)
        })))
    })));

    // fail(tag) — create a failed Task with the given error value.
    env.set("fail", Value::Builtin("fail".into(), Rc::new(|args| {
        if args.len() != 1 {
            return Err(EvalError::ArityMismatch { expected: 1, got: args.len() });
        }
        let e = args[0].clone();
        Ok(Value::Task(Rc::new(move || Err(e.clone()))))
    })));

    // print(str) — print a line to stdout.
    env.set("print", Value::Builtin("print".into(), Rc::new(|args| {
        if args.len() != 1 {
            return Err(EvalError::ArityMismatch { expected: 1, got: args.len() });
        }
        let s = match &args[0] {
            Value::Str(s) => s.clone(),
            other => return Err(EvalError::TypeMismatch {
                expected: "Str",
                got: format!("{:?}", other),
            }),
        };
        Ok(Value::Task(Rc::new(move || {
            println!("{}", s);
            Ok(Value::Record(vec![]))
        })))
    })));

    // read_line — read a line from stdin.
    env.set("read_line", Value::Task(Rc::new(|| {
        use std::io::BufRead;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)
            .map_err(|e| Value::Tag("IoErr".into(), Box::new(Value::Str(e.to_string()))))?;
        Ok(Value::Str(line.trim_end_matches('\n').to_string()))
    })));

    // ── List builtins ────────────────────────────────────────────────────────

    // cons(x, xs) — prepend x to xs.
    env.set("cons", Value::Builtin("cons".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let x = args[0].clone();
        match args[1].clone() {
            Value::List(mut xs) => { xs.insert(0, x); Ok(Value::List(xs)) }
            other => Err(EvalError::TypeMismatch { expected: "List", got: format!("{:?}", other) }),
        }
    })));

    // head(xs) — first element as Some, or None if empty.
    env.set("head", Value::Builtin("head".into(), Rc::new(|args| {
        if args.len() != 1 { return Err(EvalError::ArityMismatch { expected: 1, got: args.len() }); }
        match &args[0] {
            Value::List(xs) => Ok(if xs.is_empty() {
                Value::Tag("None".into(), Box::new(Value::Record(vec![])))
            } else {
                Value::Tag("Some".into(), Box::new(xs[0].clone()))
            }),
            other => Err(EvalError::TypeMismatch { expected: "List", got: format!("{:?}", other) }),
        }
    })));

    // tail(xs) — rest of list as Some, or None if empty.
    env.set("tail", Value::Builtin("tail".into(), Rc::new(|args| {
        if args.len() != 1 { return Err(EvalError::ArityMismatch { expected: 1, got: args.len() }); }
        match &args[0] {
            Value::List(xs) => Ok(if xs.is_empty() {
                Value::Tag("None".into(), Box::new(Value::Record(vec![])))
            } else {
                Value::Tag("Some".into(), Box::new(Value::List(xs[1..].to_vec())))
            }),
            other => Err(EvalError::TypeMismatch { expected: "List", got: format!("{:?}", other) }),
        }
    })));

    // len(xs) — number of elements.
    env.set("len", Value::Builtin("len".into(), Rc::new(|args| {
        if args.len() != 1 { return Err(EvalError::ArityMismatch { expected: 1, got: args.len() }); }
        match &args[0] {
            Value::List(xs) => Ok(Value::Int(xs.len() as i64)),
            other => Err(EvalError::TypeMismatch { expected: "List", got: format!("{:?}", other) }),
        }
    })));

    // map(f, xs) — apply f to each element.
    env.set("map", Value::Builtin("map".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let f = args[0].clone();
        match args[1].clone() {
            Value::List(xs) => {
                let result: Result<Vec<Value>, EvalError> = xs.into_iter()
                    .map(|x| apply(f.clone(), vec![x]))
                    .collect();
                Ok(Value::List(result?))
            }
            other => Err(EvalError::TypeMismatch { expected: "List", got: format!("{:?}", other) }),
        }
    })));

    // fold(f, init, xs) — left fold.
    env.set("fold", Value::Builtin("fold".into(), Rc::new(|args| {
        if args.len() != 3 { return Err(EvalError::ArityMismatch { expected: 3, got: args.len() }); }
        let f = args[0].clone();
        let init = args[1].clone();
        match args[2].clone() {
            Value::List(xs) => xs.into_iter().try_fold(init, |acc, x| apply(f.clone(), vec![acc, x])),
            other => Err(EvalError::TypeMismatch { expected: "List", got: format!("{:?}", other) }),
        }
    })));

    // append(xs, ys) — concatenate two lists.
    env.set("append", Value::Builtin("append".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (args[0].clone(), args[1].clone()) {
            (Value::List(mut xs), Value::List(ys)) => { xs.extend(ys); Ok(Value::List(xs)) }
            (other, _) => Err(EvalError::TypeMismatch { expected: "List", got: format!("{:?}", other) }),
        }
    })));

    env
}
