use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use crate::ast::*;

// ── Value ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
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
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::Str(s) => write!(f, "{:?}", s),
            Value::Closure { params, .. } => write!(f, "<fn/{}>", params.len()),
            Value::Builtin(name, _) => write!(f, "<builtin:{}>", name),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::Str(s) => write!(f, "{}", s),
            Value::Closure { params, .. } => write!(f, "<fn/{}>", params.len()),
            Value::Builtin(name, _) => write!(f, "<builtin:{}>", name),
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
        }
    }
}

// ── Eval ──────────────────────────────────────────────────────────────────────

pub fn eval(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Lit(lit) => Ok(match lit {
            Lit::Int(n) => Value::Int(*n),
            Lit::Float(f) => Value::Float(*f),
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

        Expr::Block(bindings, body) => {
            // Each binding is evaluated in the env of all *previous* bindings —
            // no self-reference, no mutual recursion (Y-combinator for that).
            let mut cur = env.clone();
            for (name, expr) in bindings {
                let val = eval(expr, &cur)?;
                let next = cur.extend();
                next.set(name, val);
                cur = next;
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

    // String concat — variadic, stringifies any primitive.
    env.set("++", Value::Builtin("++".into(), Rc::new(|args| {
        let mut out = String::new();
        for v in args {
            match v {
                Value::Str(s) => out.push_str(&s),
                Value::Int(n) => out.push_str(&n.to_string()),
                Value::Float(f) => out.push_str(&f.to_string()),
                other => return Err(EvalError::TypeMismatch {
                    expected: "stringifiable value",
                    got: format!("{:?}", other),
                }),
            }
        }
        Ok(Value::Str(out))
    })));

    // Numeric addition.
    env.set("+", Value::Builtin("+".into(), Rc::new(|args| {
        if args.len() != 2 {
            return Err(EvalError::ArityMismatch { expected: 2, got: args.len() });
        }
        match (&args[0], &args[1]) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (a, b) => Err(EvalError::TypeMismatch {
                expected: "two numbers of the same type",
                got: format!("{:?} and {:?}", a, b),
            }),
        }
    })));

    env
}
