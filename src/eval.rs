use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use crate::types::TypeMap;

use colored::Colorize;

use crate::ast::*;

// ── Value ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Record(Vec<(String, Value)>),
    /// Tag always carries a payload; unit payload = Record([]).
    Tag(String, Box<Value>),
    Closure {
        params: Vec<String>,
        body: Box<Spanned<Expr>>,
        env: Env,
    },
    Builtin(String, Rc<dyn Fn(Vec<Value>) -> Result<Value, EvalError>>),
    /// A suspended IO computation. Returns Ok(value) on success, Err(tag) on failure.
    Task(Rc<dyn Fn() -> Result<Value, Value>>),
    /// Generic array — used for non-numeric element types (Str, records, etc.).
    Array(Vec<Value>),
    /// Integer array (N-D, flat row-major).
    IntArray { data: Rc<Vec<i64>>, shape: Vec<usize> },
    /// Float array (N-D, flat row-major).
    FloatArray { data: Rc<Vec<f64>>, shape: Vec<usize> },
    /// Legacy tensor (kept for backward compat with old tensor builtins).
    Tensor { data: Rc<Vec<f64>>, shape: Vec<usize> },
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Record(a), Value::Record(b)) => a == b,
            (Value::Tag(t1, v1), Value::Tag(t2, v2)) => t1 == t2 && v1 == v2,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::IntArray { data: d1, shape: s1 }, Value::IntArray { data: d2, shape: s2 }) => {
                s1 == s2 && d1.as_ref() == d2.as_ref()
            }
            (Value::FloatArray { data: d1, shape: s1 }, Value::FloatArray { data: d2, shape: s2 }) => {
                s1 == s2 && d1.as_ref() == d2.as_ref()
            }
            (Value::Tensor { data: d1, shape: s1 }, Value::Tensor { data: d2, shape: s2 }) => {
                s1 == s2 && d1.as_ref() == d2.as_ref()
            }
            _ => false, // Closure, Builtin, Task not comparable
        }
    }
}

fn fmt_shape(shape: &[usize]) -> String {
    shape.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("x")
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(x) => write!(f, "{}", x),
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
            Value::Array(xs) => {
                let parts: Vec<_> = xs.iter().map(|v| format!("{:?}", v)).collect();
                write!(f, "[{}]", parts.join(", "))
            }
            Value::IntArray { data, shape } => {
                write!(f, "Int[{}]([", fmt_shape(shape))?;
                for (i, x) in data.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", x)?;
                }
                write!(f, "])")
            }
            Value::FloatArray { data, shape } => {
                write!(f, "F64[{}]([", fmt_shape(shape))?;
                for (i, x) in data.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", x)?;
                }
                write!(f, "])")
            }
            Value::Tensor { data, shape } => {
                write!(f, "Tensor({}, [", fmt_shape(shape))?;
                for (i, x) in data.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", x)?;
                }
                write!(f, "])")
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(x) => write!(f, "{}", x),
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
            Value::Array(xs) => {
                let parts: Vec<_> = xs.iter().map(|v| format!("{}", v)).collect();
                write!(f, "[{}]", parts.join(", "))
            }
            Value::IntArray { data, shape } => {
                // rank-1: display as [1, 2, 3]; higher rank: show shape
                if shape.len() == 1 {
                    let parts: Vec<_> = data.iter().map(|x| x.to_string()).collect();
                    write!(f, "[{}]", parts.join(", "))
                } else {
                    write!(f, "Int[{}]([", fmt_shape(shape))?;
                    for (i, x) in data.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{}", x)?;
                    }
                    write!(f, "])")
                }
            }
            Value::FloatArray { data, shape } => {
                if shape.len() == 1 {
                    let parts: Vec<_> = data.iter().map(|x| x.to_string()).collect();
                    write!(f, "[{}]", parts.join(", "))
                } else {
                    write!(f, "F64[{}]([", fmt_shape(shape))?;
                    for (i, x) in data.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{}", x)?;
                    }
                    write!(f, "])")
                }
            }
            Value::Tensor { data, shape } => {
                write!(f, "Tensor({}, [", fmt_shape(shape))?;
                for (i, x) in data.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", x)?;
                }
                write!(f, "])")
            }
        }
    }
}

impl Value {
    /// Colored representation for REPL output.
    pub fn pretty(&self) -> String {
        match self {
            Value::Int(n) => n.to_string().yellow().to_string(),
            Value::Float(x) => x.to_string().yellow().to_string(),
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
            Value::Array(xs) => {
                let parts: Vec<_> = xs.iter().map(|v| v.pretty()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::IntArray { data, shape } => {
                format!("Int[{}]([{}])", fmt_shape(shape),
                    data.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
                    .cyan().to_string()
            }
            Value::FloatArray { data, shape } => {
                format!("F64[{}]([{}])", fmt_shape(shape),
                    data.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
                    .cyan().to_string()
            }
            Value::Tensor { data, shape } => {
                format!("Tensor({}, [{}])", fmt_shape(shape),
                    data.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
                    .cyan().to_string()
            }
        }
    }
}

// ── Env ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Env {
    frame: Rc<EnvFrame>,
    pub type_map: Option<Rc<TypeMap>>,
}

#[derive(Debug)]
struct EnvFrame {
    bindings: RefCell<HashMap<String, Value>>,
    parent: Option<Env>,
}

impl Env {
    pub fn new() -> Self {
        Env {
            frame: Rc::new(EnvFrame {
                bindings: RefCell::new(HashMap::new()),
                parent: None,
            }),
            type_map: None,
        }
    }

    pub fn with_type_map(mut self, tm: TypeMap) -> Self {
        self.type_map = Some(Rc::new(tm));
        self
    }

    pub fn extend(&self) -> Self {
        Env {
            frame: Rc::new(EnvFrame {
                bindings: RefCell::new(HashMap::new()),
                parent: Some(self.clone()),
            }),
            type_map: self.type_map.clone(),
        }
    }

    pub fn set(&self, name: impl Into<String>, value: Value) {
        self.frame.bindings.borrow_mut().insert(name.into(), value);
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(val) = self.frame.bindings.borrow().get(name) {
            return Some(val.clone());
        }
        self.frame.parent.as_ref()?.get(name)
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

pub fn eval(expr: &Spanned<Expr>, env: &Env) -> Result<Value, EvalError> {
    let Node { id, inner: expr, .. } = expr;
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
            // Use type_map to decide if this App node should be lifted.
            let is_lifted = env.type_map.as_ref()
                .map_or(false, |tm| tm.lifted_apps.contains(id));
            if is_lifted {
                let has_array = arg_vals.iter().any(|a| matches!(a, Value::IntArray { .. } | Value::FloatArray { .. }));
                if has_array {
                    let out_shape = broadcast_all_shapes(&arg_vals)?;
                    return lift_apply(&f_val, arg_vals, &out_shape);
                }
            }
            plain_apply(f_val, arg_vals)
        }

        Expr::Record(fields) => {
            let evaled = fields.iter()
                .map(|(k, e)| eval(e, env).map(|v| (k.clone(), v)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Record(evaled))
        }

        Expr::Field(expr, name) => {
            match eval(expr.as_ref(), env)? {
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
            match eval(scrutinee.as_ref(), env)? {
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
            // Convert to typed array if all elements are the same numeric type.
            if vals.is_empty() {
                return Ok(Value::Array(vec![]));
            }
            match &vals[0] {
                Value::Int(_) => {
                    let ints: Option<Vec<i64>> = vals.iter().map(|v| match v {
                        Value::Int(n) => Some(*n),
                        _ => None,
                    }).collect();
                    if let Some(data) = ints {
                        let n = data.len();
                        return Ok(Value::IntArray { data: Rc::new(data), shape: vec![n] });
                    }
                }
                Value::Float(_) => {
                    let floats: Option<Vec<f64>> = vals.iter().map(|v| match v {
                        Value::Float(x) => Some(*x),
                        _ => None,
                    }).collect();
                    if let Some(data) = floats {
                        let n = data.len();
                        return Ok(Value::FloatArray { data: Rc::new(data), shape: vec![n] });
                    }
                }
                Value::IntArray { shape: inner_shape, .. } => {
                    let inner_shape = inner_shape.clone();
                    if vals.iter().all(|v| matches!(v, Value::IntArray { shape, .. } if *shape == inner_shape)) {
                        let mut data: Vec<i64> = Vec::new();
                        for v in &vals { if let Value::IntArray { data: d, .. } = v { data.extend(d.iter().copied()); } }
                        let mut shape = vec![vals.len()];
                        shape.extend(&inner_shape);
                        return Ok(Value::IntArray { data: Rc::new(data), shape });
                    }
                }
                Value::FloatArray { shape: inner_shape, .. } => {
                    let inner_shape = inner_shape.clone();
                    if vals.iter().all(|v| matches!(v, Value::FloatArray { shape, .. } if *shape == inner_shape)) {
                        let mut data: Vec<f64> = Vec::new();
                        for v in &vals { if let Value::FloatArray { data: d, .. } = v { data.extend(d.iter().copied()); } }
                        let mut shape = vec![vals.len()];
                        shape.extend(&inner_shape);
                        return Ok(Value::FloatArray { data: Rc::new(data), shape });
                    }
                }
                _ => {}
            }
            Ok(Value::Array(vals))
        }

        Expr::Import(path) => {
            crate::module::load_module(path)
                .map(|e| e.val)
                .map_err(EvalError::ModuleError)
        }

        Expr::Index(arr_expr, indices) => {
            use crate::ast::IndexArg;
            let arr = eval(arr_expr, env)?;
            let mut current = arr;
            for idx in indices {
                current = match idx {
                    IndexArg::Scalar(idx_expr) => {
                        let idx_val = eval(idx_expr, env)?;
                        let i = match idx_val {
                            Value::Int(n) => n as usize,
                            other => return Err(EvalError::TypeMismatch {
                                expected: "Int",
                                got: format!("{:?}", other),
                            }),
                        };
                        match current {
                            Value::IntArray { ref data, ref shape } => {
                                if shape.len() == 1 {
                                    // Rank-1: return scalar
                                    Value::Int(data[i] as i64)
                                } else {
                                    // Higher rank: return a slice
                                    let row_size: usize = shape[1..].iter().product();
                                    let start = i * row_size;
                                    let end = start + row_size;
                                    let new_data = data[start..end].to_vec();
                                    let new_shape = shape[1..].to_vec();
                                    Value::IntArray { data: Rc::new(new_data), shape: new_shape }
                                }
                            }
                            Value::FloatArray { ref data, ref shape } => {
                                if shape.len() == 1 {
                                    Value::Float(data[i])
                                } else {
                                    let row_size: usize = shape[1..].iter().product();
                                    let start = i * row_size;
                                    let end = start + row_size;
                                    let new_data = data[start..end].to_vec();
                                    let new_shape = shape[1..].to_vec();
                                    Value::FloatArray { data: Rc::new(new_data), shape: new_shape }
                                }
                            }
                            Value::Array(xs) => xs.into_iter().nth(i).ok_or_else(|| {
                                EvalError::TypeMismatch { expected: "in-bounds index", got: format!("{}", i) }
                            })?,
                            other => return Err(EvalError::TypeMismatch {
                                expected: "array",
                                got: format!("{:?}", other),
                            }),
                        }
                    }
                    IndexArg::Fancy(idx_expr) => {
                        let idx_val = eval(idx_expr, env)?;
                        // Gather operation
                        match (current, idx_val) {
                            (Value::IntArray { data, shape: _ }, Value::IntArray { data: idx_data, shape: idx_shape }) => {
                                let gathered: Vec<i64> = idx_data.iter().map(|&i| data[i as usize]).collect();
                                Value::IntArray { data: Rc::new(gathered), shape: idx_shape }
                            }
                            (Value::FloatArray { data, shape: _ }, Value::IntArray { data: idx_data, shape: idx_shape }) => {
                                let gathered: Vec<f64> = idx_data.iter().map(|&i| data[i as usize]).collect();
                                Value::FloatArray { data: Rc::new(gathered), shape: idx_shape }
                            }
                            (other, _) => return Err(EvalError::TypeMismatch {
                                expected: "numeric array",
                                got: format!("{:?}", other),
                            }),
                        }
                    }
                    IndexArg::Slice(from_expr, to_expr) => {
                        match current {
                            Value::IntArray { data, shape } => {
                                let from = match from_expr {
                                    Some(e) => match eval(e, env)? {
                                        Value::Int(n) => n as usize,
                                        _ => return Err(EvalError::TypeMismatch { expected: "Int", got: "non-int".into() }),
                                    },
                                    None => 0,
                                };
                                let to = match to_expr {
                                    Some(e) => match eval(e, env)? {
                                        Value::Int(n) => n as usize,
                                        _ => return Err(EvalError::TypeMismatch { expected: "Int", got: "non-int".into() }),
                                    },
                                    None => shape[0],
                                };
                                if shape.len() == 1 {
                                    let sliced = data[from..to].to_vec();
                                    Value::IntArray { data: Rc::new(sliced), shape: vec![to - from] }
                                } else {
                                    let row_size: usize = shape[1..].iter().product();
                                    let sliced = data[from * row_size..to * row_size].to_vec();
                                    let mut new_shape = vec![to - from];
                                    new_shape.extend_from_slice(&shape[1..]);
                                    Value::IntArray { data: Rc::new(sliced), shape: new_shape }
                                }
                            }
                            Value::FloatArray { data, shape } => {
                                let from = match from_expr {
                                    Some(e) => match eval(e, env)? { Value::Int(n) => n as usize, _ => 0 },
                                    None => 0,
                                };
                                let to = match to_expr {
                                    Some(e) => match eval(e, env)? { Value::Int(n) => n as usize, _ => shape[0] },
                                    None => shape[0],
                                };
                                if shape.len() == 1 {
                                    let sliced = data[from..to].to_vec();
                                    Value::FloatArray { data: Rc::new(sliced), shape: vec![to - from] }
                                } else {
                                    let row_size: usize = shape[1..].iter().product();
                                    let sliced = data[from * row_size..to * row_size].to_vec();
                                    let mut new_shape = vec![to - from];
                                    new_shape.extend_from_slice(&shape[1..]);
                                    Value::FloatArray { data: Rc::new(sliced), shape: new_shape }
                                }
                            }
                            other => return Err(EvalError::TypeMismatch {
                                expected: "numeric array",
                                got: format!("{:?}", other),
                            }),
                        }
                    }
                };
            }
            Ok(current)
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
            eval(body.as_ref(), &cur)
        }
    }
}

/// Return the broadcast shape of two shapes (shorter must be a prefix of longer).
/// Scalars (empty shape) broadcast to anything.
fn broadcast_shape_pair(s1: &[usize], s2: &[usize]) -> Result<Vec<usize>, EvalError> {
    let (shorter, longer) = if s1.len() <= s2.len() { (s1, s2) } else { (s2, s1) };
    for (a, b) in shorter.iter().zip(longer.iter()) {
        if a != b {
            return Err(EvalError::TypeMismatch {
                expected: "compatible shapes for broadcasting",
                got: format!("cannot broadcast shapes {:?} and {:?}", s1, s2),
            });
        }
    }
    Ok(longer.to_vec())
}

/// Compute the broadcast output shape across all args (scalars ignored).
fn broadcast_all_shapes(args: &[Value]) -> Result<Vec<usize>, EvalError> {
    let mut result: Vec<usize> = vec![];
    for arg in args {
        let shape: &[usize] = match arg {
            Value::IntArray { shape, .. } | Value::FloatArray { shape, .. } => shape,
            _ => &[],
        };
        result = broadcast_shape_pair(&result, shape)?;
    }
    Ok(result)
}

/// Extract the i-th "row" (removing the outermost dim).
/// For rank-1 arrays, returns a scalar. For rank-0 (scalar), returns val unchanged (broadcast).
fn extract_slice(val: &Value, i: usize) -> Value {
    match val {
        Value::IntArray { data, shape } if !shape.is_empty() => {
            if shape.len() == 1 {
                Value::Int(data[i])
            } else {
                let sub_size: usize = shape[1..].iter().product();
                let start = i * sub_size;
                let sub = data[start..start + sub_size].to_vec();
                Value::IntArray { data: Rc::new(sub), shape: shape[1..].to_vec() }
            }
        }
        Value::FloatArray { data, shape } if !shape.is_empty() => {
            if shape.len() == 1 {
                Value::Float(data[i])
            } else {
                let sub_size: usize = shape[1..].iter().product();
                let start = i * sub_size;
                let sub = data[start..start + sub_size].to_vec();
                Value::FloatArray { data: Rc::new(sub), shape: shape[1..].to_vec() }
            }
        }
        // Scalar or non-array: broadcasts (returned unchanged)
        other => other.clone(),
    }
}

/// Collect a Vec of homogeneous scalar/array results into a single shaped array.
fn collect_into_array(results: Vec<Value>, out_shape: &[usize]) -> Result<Value, EvalError> {
    if results.is_empty() {
        // Empty outer dim — infer element type as Int by default.
        return Ok(Value::IntArray { data: Rc::new(vec![]), shape: out_shape.to_vec() });
    }
    match &results[0] {
        Value::Int(_) => {
            let data: Result<Vec<i64>, _> = results.iter().map(|v| match v {
                Value::Int(n) => Ok(*n),
                other => Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
            }).collect();
            Ok(Value::IntArray { data: Rc::new(data?), shape: out_shape.to_vec() })
        }
        Value::Float(_) => {
            let data: Result<Vec<f64>, _> = results.iter().map(|v| match v {
                Value::Float(x) => Ok(*x),
                Value::Int(n) => Ok(*n as f64),
                other => Err(EvalError::TypeMismatch { expected: "Float", got: format!("{:?}", other) }),
            }).collect();
            Ok(Value::FloatArray { data: Rc::new(data?), shape: out_shape.to_vec() })
        }
        Value::IntArray { .. } => {
            let mut flat: Vec<i64> = vec![];
            for r in &results {
                match r {
                    Value::IntArray { data, .. } => flat.extend(data.iter().copied()),
                    other => return Err(EvalError::TypeMismatch { expected: "IntArray", got: format!("{:?}", other) }),
                }
            }
            Ok(Value::IntArray { data: Rc::new(flat), shape: out_shape.to_vec() })
        }
        Value::FloatArray { .. } => {
            let mut flat: Vec<f64> = vec![];
            for r in &results {
                match r {
                    Value::FloatArray { data, .. } => flat.extend(data.iter().copied()),
                    other => return Err(EvalError::TypeMismatch { expected: "FloatArray", got: format!("{:?}", other) }),
                }
            }
            Ok(Value::FloatArray { data: Rc::new(flat), shape: out_shape.to_vec() })
        }
        other => Err(EvalError::TypeMismatch {
            expected: "scalar or array result from lifted function",
            got: format!("{:?}", other),
        }),
    }
}

/// Recursively lift `f` over shaped args, producing a shaped result.
fn lift_apply(f: &Value, args: Vec<Value>, out_shape: &[usize]) -> Result<Value, EvalError> {
    if out_shape.is_empty() {
        return plain_apply(f.clone(), args);
    }
    let outer = out_shape[0];
    let rest_shape = &out_shape[1..];
    let mut results: Vec<Value> = Vec::with_capacity(outer);
    for i in 0..outer {
        let sliced: Vec<Value> = args.iter().map(|a| extract_slice(a, i)).collect();
        results.push(lift_apply(f, sliced, rest_shape)?);
    }
    collect_into_array(results, out_shape)
}

/// Direct application without lifting (current logic).
fn plain_apply(f: Value, args: Vec<Value>) -> Result<Value, EvalError> {
    match f {
        Value::Closure { params, body, env } => {
            if args.len() != params.len() {
                return Err(EvalError::ArityMismatch { expected: params.len(), got: args.len() });
            }
            let frame = env.extend();
            for (param, arg) in params.iter().zip(args) {
                frame.set(param, arg);
            }
            eval(&*body, &frame)
        }
        Value::Builtin(_, f) => f(args),
        other => Err(EvalError::TypeMismatch {
            expected: "function",
            got: format!("{:?}", other),
        }),
    }
}

/// Scalar builtins that should be lifted element-wise over shaped arrays.
const LIFTABLE_BUILTINS: &[&str] = &["+", "-", "*", "==", "+.", "-.", "*.", "/."];

fn apply(f: Value, args: Vec<Value>) -> Result<Value, EvalError> {
    // Lift element-wise over IntArray/FloatArray for closures and scalar builtins.
    let should_lift = match &f {
        Value::Closure { .. } => true,
        Value::Builtin(name, _) => LIFTABLE_BUILTINS.contains(&name.as_str()),
        _ => false,
    };
    if should_lift {
        let has_array = args.iter().any(|a| matches!(a, Value::IntArray { .. } | Value::FloatArray { .. }));
        if has_array {
            let out_shape = broadcast_all_shapes(&args)?;
            return lift_apply(&f, args, &out_shape);
        }
    }
    plain_apply(f, args)
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

// ── Fixed-point combinator ────────────────────────────────────────────────────

/// Build a value that behaves as `fix(f)`: when called with args, evaluates `f(fix(f))(args)`.
/// Wrapping in a closure avoids eager infinite recursion in a strict language.
fn fix_value(f: Rc<Value>) -> Value {
    let f2 = f.clone();
    Value::Builtin("<fix>".into(), Rc::new(move |args| {
        let recursive = fix_value(f2.clone());
        let stepped = apply((*f2).clone(), vec![recursive])?;
        apply(stepped, args)
    }))
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
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (&args[0], &args[1]) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (a, b) => Err(EvalError::TypeMismatch { expected: "Int Int", got: format!("{:?} and {:?}", a, b) }),
        }
    })));

    // Integer subtraction.
    env.set("-", Value::Builtin("-".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (&args[0], &args[1]) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (a, b) => Err(EvalError::TypeMismatch { expected: "Int Int", got: format!("{:?} and {:?}", a, b) }),
        }
    })));

    // Integer multiplication.
    env.set("*", Value::Builtin("*".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (&args[0], &args[1]) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (a, b) => Err(EvalError::TypeMismatch { expected: "Int Int", got: format!("{:?} and {:?}", a, b) }),
        }
    })));

    // Structural equality — returns True or False tag.
    env.set("==", Value::Builtin("==".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let unit = Box::new(Value::Record(vec![]));
        Ok(if args[0] == args[1] { Value::Tag("True".into(), unit) } else { Value::Tag("False".into(), unit) })
    })));

    // fix(f) — fixed-point combinator for recursion: fix(f) = f(fix(f))
    env.set("fix", Value::Builtin("fix".into(), Rc::new(|args| {
        if args.len() != 1 {
            return Err(EvalError::ArityMismatch { expected: 1, got: args.len() });
        }
        Ok(fix_value(Rc::new(args[0].clone())))
    })));

    // ok(v) — wrap a value in a successful Task.
    env.set("ok", Value::Builtin("ok".into(), Rc::new(|args| {
        if args.len() != 1 {
            return Err(EvalError::ArityMismatch { expected: 1, got: args.len() });
        }
        let v = args[0].clone();
        Ok(Value::Task(Rc::new(move || Ok(v.clone()))))
    })));

    // >>=(task, f) — sequence two Tasks; error type accumulates via row unification.
    env.set(">>=", Value::Builtin(">>=".into(), Rc::new(|args| {
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

    // catch(task, handler) — recover from a failed Task.
    env.set("catch", Value::Builtin("catch".into(), Rc::new(|args| {
        if args.len() != 2 {
            return Err(EvalError::ArityMismatch { expected: 2, got: args.len() });
        }
        let task = args[0].clone();
        let handler = args[1].clone();
        Ok(Value::Task(Rc::new(move || {
            match run_task(&task) {
                Ok(v) => Ok(v),
                Err(e) => {
                    let recovery = apply(handler.clone(), vec![e])
                        .map_err(|e| Value::Tag("EvalError".into(), Box::new(Value::Str(e.to_string()))))?;
                    run_task(&recovery)
                }
            }
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

    // ::(x, xs) — prepend x to xs.
    env.set("::", Value::Builtin("::".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let x = args[0].clone();
        match (x, args[1].clone()) {
            (Value::Int(v), Value::IntArray { data, shape: _ }) => {
                let mut new_data = vec![v];
                new_data.extend(data.iter().copied());
                let n = new_data.len();
                Ok(Value::IntArray { data: Rc::new(new_data), shape: vec![n] })
            }
            (Value::Float(v), Value::FloatArray { data, shape: _ }) => {
                let mut new_data = vec![v];
                new_data.extend(data.iter().copied());
                let n = new_data.len();
                Ok(Value::FloatArray { data: Rc::new(new_data), shape: vec![n] })
            }
            (x, Value::Array(mut xs)) => { xs.insert(0, x); Ok(Value::Array(xs)) }
            (_, other) => Err(EvalError::TypeMismatch { expected: "array or List", got: format!("{:?}", other) }),
        }
    })));

    // head(xs) — first element as Some, or None if empty.
    env.set("head", Value::Builtin("head".into(), Rc::new(|args| {
        if args.len() != 1 { return Err(EvalError::ArityMismatch { expected: 1, got: args.len() }); }
        let none = Value::Tag("None".into(), Box::new(Value::Record(vec![])));
        match &args[0] {
            Value::Array(xs) => Ok(if xs.is_empty() { none } else {
                Value::Tag("Some".into(), Box::new(xs[0].clone()))
            }),
            Value::IntArray { data, .. } => Ok(if data.is_empty() { none } else {
                Value::Tag("Some".into(), Box::new(Value::Int(data[0])))
            }),
            Value::FloatArray { data, .. } => Ok(if data.is_empty() { none } else {
                Value::Tag("Some".into(), Box::new(Value::Float(data[0])))
            }),
            other => Err(EvalError::TypeMismatch { expected: "array", got: format!("{:?}", other) }),
        }
    })));

    // tail(xs) — rest of list as Some, or None if empty.
    env.set("tail", Value::Builtin("tail".into(), Rc::new(|args| {
        if args.len() != 1 { return Err(EvalError::ArityMismatch { expected: 1, got: args.len() }); }
        let none = Value::Tag("None".into(), Box::new(Value::Record(vec![])));
        match &args[0] {
            Value::Array(xs) => Ok(if xs.is_empty() { none } else {
                Value::Tag("Some".into(), Box::new(Value::Array(xs[1..].to_vec())))
            }),
            Value::IntArray { data, .. } => Ok(if data.is_empty() { none } else {
                let rest = data[1..].to_vec();
                let n = rest.len();
                Value::Tag("Some".into(), Box::new(Value::IntArray { data: Rc::new(rest), shape: vec![n] }))
            }),
            Value::FloatArray { data, .. } => Ok(if data.is_empty() { none } else {
                let rest = data[1..].to_vec();
                let n = rest.len();
                Value::Tag("Some".into(), Box::new(Value::FloatArray { data: Rc::new(rest), shape: vec![n] }))
            }),
            other => Err(EvalError::TypeMismatch { expected: "array", got: format!("{:?}", other) }),
        }
    })));

    // len(xs) — first-dimension size.
    env.set("len", Value::Builtin("len".into(), Rc::new(|args| {
        if args.len() != 1 { return Err(EvalError::ArityMismatch { expected: 1, got: args.len() }); }
        match &args[0] {
            Value::Array(xs) => Ok(Value::Int(xs.len() as i64)),
            Value::IntArray { shape, .. } => Ok(Value::Int(shape[0] as i64)),
            Value::FloatArray { shape, .. } => Ok(Value::Int(shape[0] as i64)),
            Value::Tensor { shape, .. } => Ok(Value::Int(shape[0] as i64)),
            other => Err(EvalError::TypeMismatch { expected: "array", got: format!("{:?}", other) }),
        }
    })));

    // map(f, xs) — apply f to each element.
    env.set("map", Value::Builtin("map".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let f = args[0].clone();
        match args[1].clone() {
            Value::Array(xs) => {
                let result: Result<Vec<Value>, EvalError> = xs.into_iter()
                    .map(|x| apply(f.clone(), vec![x]))
                    .collect();
                Ok(Value::Array(result?))
            }
            Value::IntArray { data, shape: _ } => {
                let result: Result<Vec<Value>, EvalError> = data.iter()
                    .map(|&x| apply(f.clone(), vec![Value::Int(x)]))
                    .collect();
                let vals = result?;
                // If all results are Int, produce IntArray
                let ints: Option<Vec<i64>> = vals.iter().map(|v| match v { Value::Int(n) => Some(*n), _ => None }).collect();
                if let Some(data) = ints {
                    let n = data.len();
                    return Ok(Value::IntArray { data: Rc::new(data), shape: vec![n] });
                }
                Ok(Value::Array(vals))
            }
            Value::FloatArray { data, shape: _ } => {
                let result: Result<Vec<Value>, EvalError> = data.iter()
                    .map(|&x| apply(f.clone(), vec![Value::Float(x)]))
                    .collect();
                let vals = result?;
                let floats: Option<Vec<f64>> = vals.iter().map(|v| match v { Value::Float(x) => Some(*x), _ => None }).collect();
                if let Some(data) = floats {
                    let n = data.len();
                    return Ok(Value::FloatArray { data: Rc::new(data), shape: vec![n] });
                }
                Ok(Value::Array(vals))
            }
            other => Err(EvalError::TypeMismatch { expected: "array", got: format!("{:?}", other) }),
        }
    })));

    // fold(f, init, xs) — left fold.
    env.set("fold", Value::Builtin("fold".into(), Rc::new(|args| {
        if args.len() != 3 { return Err(EvalError::ArityMismatch { expected: 3, got: args.len() }); }
        let f = args[0].clone();
        let init = args[1].clone();
        match args[2].clone() {
            Value::Array(xs) => xs.into_iter().try_fold(init, |acc, x| apply(f.clone(), vec![acc, x])),
            Value::IntArray { data, .. } => {
                data.iter().try_fold(init, |acc, &x| apply(f.clone(), vec![acc, Value::Int(x)]))
            }
            Value::FloatArray { data, .. } => {
                data.iter().try_fold(init, |acc, &x| apply(f.clone(), vec![acc, Value::Float(x)]))
            }
            other => Err(EvalError::TypeMismatch { expected: "array", got: format!("{:?}", other) }),
        }
    })));

    // list_of(n, v) — create a list of length n filled with v.
    env.set("list_of", Value::Builtin("list_of".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let n = match &args[0] {
            Value::Int(n) => *n,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        Ok(Value::Array(vec![args[1].clone(); n as usize]))
    })));

    // list_init(n, f) — create a list of length n by calling f(i) for each index i.
    env.set("list_init", Value::Builtin("list_init".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let n = match &args[0] {
            Value::Int(n) => *n,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        let f = args[1].clone();
        let items: Result<Vec<Value>, EvalError> = (0..n)
            .map(|i| apply(f.clone(), vec![Value::Int(i)]))
            .collect();
        Ok(Value::Array(items?))
    })));

    // <>(xs, ys) — concatenate two lists/arrays.
    env.set("<>", Value::Builtin("<>".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (args[0].clone(), args[1].clone()) {
            // Empty list is the neutral element — treat as empty array of the other's type.
            (Value::Array(xs), Value::Array(ys)) => {
                let mut out = xs; out.extend(ys); Ok(Value::Array(out))
            }
            (Value::Array(xs), other) if xs.is_empty() => Ok(other),
            (other, Value::Array(ys)) if ys.is_empty() => Ok(other),
            (Value::IntArray { data: d1, .. }, Value::IntArray { data: d2, .. }) => {
                let mut new_data: Vec<i64> = d1.iter().copied().collect();
                new_data.extend(d2.iter().copied());
                let n = new_data.len();
                Ok(Value::IntArray { data: Rc::new(new_data), shape: vec![n] })
            }
            (Value::FloatArray { data: d1, .. }, Value::FloatArray { data: d2, .. }) => {
                let mut new_data: Vec<f64> = d1.iter().copied().collect();
                new_data.extend(d2.iter().copied());
                let n = new_data.len();
                Ok(Value::FloatArray { data: Rc::new(new_data), shape: vec![n] })
            }
            (other, _) => Err(EvalError::TypeMismatch { expected: "array", got: format!("{:?}", other) }),
        }
    })));

    // Aliases: natural names for the symbolic builtins.
    env.set("then",   env.get(">>=").unwrap());
    env.set("cons",   env.get("::").unwrap());
    env.set("append", env.get("<>").unwrap());

    // ── Tensor builtins ──────────────────────────────────────────────────────

    // zeros(rows, cols) — create a zero tensor.
    env.set("zeros", Value::Builtin("zeros".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let rows = match &args[0] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        let cols = match &args[1] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        Ok(Value::Tensor { data: Rc::new(vec![0.0; rows * cols]), shape: vec![rows, cols] })
    })));

    // ones(rows, cols) — create a ones tensor.
    env.set("ones", Value::Builtin("ones".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let rows = match &args[0] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        let cols = match &args[1] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        Ok(Value::Tensor { data: Rc::new(vec![1.0; rows * cols]), shape: vec![rows, cols] })
    })));

    // tensor_get(t, i, j) — get element at row i, col j.
    env.set("tensor_get", Value::Builtin("tensor_get".into(), Rc::new(|args| {
        if args.len() != 3 { return Err(EvalError::ArityMismatch { expected: 3, got: args.len() }); }
        let (data, shape) = match &args[0] {
            Value::Tensor { data, shape } => (data.clone(), shape.clone()),
            other => return Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        };
        let i = match &args[1] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        let j = match &args[2] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        Ok(Value::Float(data[i * shape[1] + j]))
    })));

    // tensor_set(t, i, j, v) — return a new tensor with element (i,j) set to v.
    env.set("tensor_set", Value::Builtin("tensor_set".into(), Rc::new(|args| {
        if args.len() != 4 { return Err(EvalError::ArityMismatch { expected: 4, got: args.len() }); }
        let (data, shape) = match &args[0] {
            Value::Tensor { data, shape } => (data.clone(), shape.clone()),
            other => return Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        };
        let i = match &args[1] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        let j = match &args[2] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        let v = match &args[3] {
            Value::Float(f) => *f,
            other => return Err(EvalError::TypeMismatch { expected: "Float", got: format!("{:?}", other) }),
        };
        let cols = shape[1];
        let mut new_data = (*data).clone();
        new_data[i * cols + j] = v;
        Ok(Value::Tensor { data: Rc::new(new_data), shape })
    })));

    // dot(a, b) — dot product of two rank-1 arrays.
    env.set("dot", Value::Builtin("dot".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (&args[0], &args[1]) {
            (Value::IntArray { data: da, shape: sa }, Value::IntArray { data: db, shape: sb }) => {
                if sa != sb {
                    return Err(EvalError::TypeMismatch {
                        expected: "matching shapes",
                        got: format!("{:?} vs {:?}", sa, sb),
                    });
                }
                Ok(Value::Int(da.iter().zip(db.iter()).map(|(a, b)| a * b).sum()))
            }
            (Value::FloatArray { data: da, shape: sa }, Value::FloatArray { data: db, shape: sb }) => {
                if sa != sb {
                    return Err(EvalError::TypeMismatch {
                        expected: "matching shapes",
                        got: format!("{:?} vs {:?}", sa, sb),
                    });
                }
                Ok(Value::Float(da.iter().zip(db.iter()).map(|(a, b)| a * b).sum()))
            }
            (a, b) => Err(EvalError::TypeMismatch {
                expected: "IntArray or FloatArray",
                got: format!("{:?} and {:?}", a, b),
            }),
        }
    })));

    // matmul(a, b) — naive O(m*k*n) matrix multiply.
    env.set("matmul", Value::Builtin("matmul".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let (da, sa) = match &args[0] {
            Value::Tensor { data, shape } => (data.clone(), shape.clone()),
            other => return Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        };
        let (db, sb) = match &args[1] {
            Value::Tensor { data, shape } => (data.clone(), shape.clone()),
            other => return Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        };
        let (m, k, n) = (sa[0], sa[1], sb[1]);
        if sb[0] != k {
            return Err(EvalError::TypeMismatch {
                expected: "compatible matrix dimensions",
                got: format!("{}x{} @ {}x{}", m, k, sb[0], n),
            });
        }
        let mut out = vec![0.0f64; m * n];
        for i in 0..m {
            for kk in 0..k {
                for j in 0..n {
                    out[i * n + j] += da[i * k + kk] * db[kk * n + j];
                }
            }
        }
        Ok(Value::Tensor { data: Rc::new(out), shape: vec![m, n] })
    })));

    // add_tensor(a, b) — elementwise addition.
    env.set("add_tensor", Value::Builtin("add_tensor".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let (da, sa) = match &args[0] {
            Value::Tensor { data, shape } => (data.clone(), shape.clone()),
            other => return Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        };
        let (db, sb) = match &args[1] {
            Value::Tensor { data, shape } => (data.clone(), shape.clone()),
            other => return Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        };
        if sa != sb {
            return Err(EvalError::TypeMismatch { expected: "matching tensor shapes", got: format!("{:?} vs {:?}", sa, sb) });
        }
        let out: Vec<f64> = da.iter().zip(db.iter()).map(|(a, b)| a + b).collect();
        Ok(Value::Tensor { data: Rc::new(out), shape: sa })
    })));

    // scale_tensor(s, t) — scalar * tensor.
    env.set("scale_tensor", Value::Builtin("scale_tensor".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        let s = match &args[0] {
            Value::Float(f) => *f,
            other => return Err(EvalError::TypeMismatch { expected: "Float", got: format!("{:?}", other) }),
        };
        let (data, shape) = match &args[1] {
            Value::Tensor { data, shape } => (data.clone(), shape.clone()),
            other => return Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        };
        let out: Vec<f64> = data.iter().map(|x| s * x).collect();
        Ok(Value::Tensor { data: Rc::new(out), shape })
    })));

    // reshape(new_rows, new_cols, t) — reshape tensor; validates total size.
    env.set("reshape", Value::Builtin("reshape".into(), Rc::new(|args| {
        if args.len() != 3 { return Err(EvalError::ArityMismatch { expected: 3, got: args.len() }); }
        let new_rows = match &args[0] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        let new_cols = match &args[1] {
            Value::Int(n) => *n as usize,
            other => return Err(EvalError::TypeMismatch { expected: "Int", got: format!("{:?}", other) }),
        };
        let (data, shape) = match &args[2] {
            Value::Tensor { data, shape } => (data.clone(), shape.clone()),
            other => return Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        };
        let old_size = shape[0] * shape[1];
        let new_size = new_rows * new_cols;
        if old_size != new_size {
            return Err(EvalError::TypeMismatch {
                expected: "same total size",
                got: format!("{} != {}", old_size, new_size),
            });
        }
        Ok(Value::Tensor { data, shape: vec![new_rows, new_cols] })
    })));

    // tensor_rows(t) — number of rows.
    env.set("tensor_rows", Value::Builtin("tensor_rows".into(), Rc::new(|args| {
        if args.len() != 1 { return Err(EvalError::ArityMismatch { expected: 1, got: args.len() }); }
        match &args[0] {
            Value::Tensor { shape, .. } => Ok(Value::Int(shape[0] as i64)),
            other => Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        }
    })));

    // tensor_cols(t) — number of columns.
    env.set("tensor_cols", Value::Builtin("tensor_cols".into(), Rc::new(|args| {
        if args.len() != 1 { return Err(EvalError::ArityMismatch { expected: 1, got: args.len() }); }
        match &args[0] {
            Value::Tensor { shape, .. } => Ok(Value::Int(shape[1] as i64)),
            other => Err(EvalError::TypeMismatch { expected: "Tensor", got: format!("{:?}", other) }),
        }
    })));

    // Float arithmetic operators.
    env.set("+.", Value::Builtin("+.".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (&args[0], &args[1]) {
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (a, b) => Err(EvalError::TypeMismatch { expected: "Float Float", got: format!("{:?} and {:?}", a, b) }),
        }
    })));

    env.set("-.", Value::Builtin("-.".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (&args[0], &args[1]) {
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (a, b) => Err(EvalError::TypeMismatch { expected: "Float Float", got: format!("{:?} and {:?}", a, b) }),
        }
    })));

    env.set("*.", Value::Builtin("*.".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (&args[0], &args[1]) {
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (a, b) => Err(EvalError::TypeMismatch { expected: "Float Float", got: format!("{:?} and {:?}", a, b) }),
        }
    })));

    env.set("/.", Value::Builtin("/.".into(), Rc::new(|args| {
        if args.len() != 2 { return Err(EvalError::ArityMismatch { expected: 2, got: args.len() }); }
        match (&args[0], &args[1]) {
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (a, b) => Err(EvalError::TypeMismatch { expected: "Float Float", got: format!("{:?} and {:?}", a, b) }),
        }
    })));

    env
}
