use std::{cell::RefCell, collections::HashMap};

use chumsky::Parser as _;

use crate::{
    eval::{eval, std_env, Value},
    parser::file_parser,
    types::{infer, std_type_env, Type},
};

#[derive(Clone)]
pub struct ModuleEntry {
    pub ty: Type,
    pub val: Value,
}

thread_local! {
    static CACHE: RefCell<HashMap<String, ModuleEntry>> = RefCell::new(HashMap::new());
}

/// Read, parse, type-check, and evaluate a module file exactly once.
/// Results are cached by canonical path — safe because the language is pure.
pub fn load_module(path: &str) -> Result<ModuleEntry, String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| format!("{}: {}", path, e))?;
    let key = canonical.to_string_lossy().into_owned();

    if let Some(entry) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Ok(entry);
    }

    let src = std::fs::read_to_string(&canonical)
        .map_err(|e| format!("{}: {}", path, e))?;
    let ast = file_parser()
        .parse(src.as_str())
        .map_err(|errs| format!("{}: parse error: {:?}", path, errs))?;
    let ty = infer(&std_type_env(), &ast)
        .map_err(|e| format!("{}: type error: {}", path, e))?;
    let val = eval(&ast, &std_env())
        .map_err(|e| format!("{}: eval error: {}", path, e))?;

    let entry = ModuleEntry { ty, val };
    CACHE.with(|c| c.borrow_mut().insert(key, entry.clone()));
    Ok(entry)
}
