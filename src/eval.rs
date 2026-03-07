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
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Record(a), Value::Record(b)) => a == b,
            (Value::Tag(t1, v1), Value::Tag(t2, v2)) => t1 == t2 && v1 == v2,
            _ => false,
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

    env
}
