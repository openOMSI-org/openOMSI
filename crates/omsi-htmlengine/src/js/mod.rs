//! The JavaScript subset: values, tokens and syntax tree, the small conversions every
//! part shares, and the lexer, parser and interpreter in the sub-modules.

mod interp;
mod lexer;
mod parser;

pub(crate) use interp::*;
pub(crate) use lexer::*;
pub(crate) use parser::*;

use super::*;

pub(crate) type ObjRef = Arc<Mutex<HashMap<String, Val>>>;
pub(crate) type ArrRef = Arc<Mutex<Vec<Val>>>;
pub(crate) type Env = Arc<Mutex<Scope>>;

pub(crate) struct Scope {
    pub(crate) vars: HashMap<String, Val>,
    pub(crate) parent: Option<Env>,
}

pub(crate) struct FuncDef {
    pub(crate) params: Vec<String>,
    pub(crate) body: Vec<Stmt>,
}

pub(crate) struct Closure {
    pub(crate) def: Arc<FuncDef>,
    pub(crate) env: Env,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Nat {
    Round,
    Floor,
    Ceil,
    Abs,
    Min,
    Max,
    Sqrt,
    Pow,
    Trunc,
    Sin,
    Cos,
    ParseInt,
    ParseFloat,
    Str,
    Number,
    IsNaN,
    GetById,
    Query,
    SetVar,
    Trigger,
    GetVar,
    Log,
    SetTimeout,
    SetInterval,
    ClearTimer,
    ObjectKeys,
    CreateEl,
    SetRoute,
    SetLine,
    SetDestination,
    ClearLine,
    SetNextStop,
    GetDepartures,
}

#[derive(Clone)]
pub(crate) enum Val {
    Undef,
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Obj(ObjRef),
    Arr(ArrRef),
    Func(Arc<Closure>),
    Nat(Nat),
    Elem(usize),
    Style(usize),
    ClassList(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Num(f64),
    Str(String),
    Id(String),
    P(String),
    Eof,
}

pub(crate) enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    Undef,
    Ident(String),
    Member(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Bin(String, Box<Expr>, Box<Expr>),
    Un(String, Box<Expr>),
    Assign(String, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Func(Arc<FuncDef>),
    Obj(Vec<(String, Expr)>),
    Arr(Vec<Expr>),
}

pub(crate) enum Stmt {
    Expr(Expr),
    Var(Vec<(String, Option<Expr>)>),
    Func(String, Arc<FuncDef>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    While(Expr, Box<Stmt>),
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    Break,
    Continue,
}

pub(crate) fn fmt_num(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else if n.is_infinite() {
        if n > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        }
    } else if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

pub(crate) fn to_str(v: &Val) -> String {
    match v {
        Val::Undef => "undefined".into(),
        Val::Null => "null".into(),
        Val::Bool(b) => b.to_string(),
        Val::Num(n) => fmt_num(*n),
        Val::Str(s) => s.clone(),
        Val::Arr(a) => a
            .lock()
            .unwrap()
            .iter()
            .map(to_str)
            .collect::<Vec<_>>()
            .join(","),
        Val::Obj(_) | Val::Elem(_) | Val::Style(_) | Val::ClassList(_) => "[object Object]".into(),
        Val::Func(_) | Val::Nat(_) => "function".into(),
    }
}

pub(crate) fn to_num(v: &Val) -> f64 {
    match v {
        Val::Num(n) => *n,
        Val::Bool(b) => *b as u8 as f64,
        Val::Null => 0.0,
        Val::Str(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        _ => f64::NAN,
    }
}

pub(crate) fn truthy(v: &Val) -> bool {
    match v {
        Val::Undef | Val::Null => false,
        Val::Bool(b) => *b,
        Val::Num(n) => *n != 0.0 && !n.is_nan(),
        Val::Str(s) => !s.is_empty(),
        _ => true,
    }
}

pub(crate) fn strict_eq(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Undef, Val::Undef) | (Val::Null, Val::Null) => true,
        (Val::Bool(x), Val::Bool(y)) => x == y,
        (Val::Num(x), Val::Num(y)) => x == y,
        (Val::Str(x), Val::Str(y)) => x == y,
        (Val::Obj(x), Val::Obj(y)) => Arc::ptr_eq(x, y),
        (Val::Arr(x), Val::Arr(y)) => Arc::ptr_eq(x, y),
        (Val::Elem(x), Val::Elem(y)) => x == y,
        _ => false,
    }
}

pub(crate) fn loose_eq(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Undef | Val::Null, Val::Undef | Val::Null) => true,
        (Val::Num(_) | Val::Str(_) | Val::Bool(_), Val::Num(_) | Val::Str(_) | Val::Bool(_)) => {
            if let (Val::Str(x), Val::Str(y)) = (a, b) {
                x == y
            } else {
                to_num(a) == to_num(b)
            }
        }
        _ => strict_eq(a, b),
    }
}

pub(crate) fn kebab(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub(crate) fn obj_of(props: &[(&str, Val)]) -> ObjRef {
    Arc::new(Mutex::new(
        props
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    ))
}
