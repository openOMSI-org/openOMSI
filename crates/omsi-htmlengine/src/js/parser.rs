//! Tokens to a syntax tree.

use super::*;

pub(crate) struct Parser {
    pub(crate) t: Vec<Tok>,
    pub(crate) i: usize,
}

impl Parser {
    pub(crate) fn peek(&self) -> &Tok {
        self.t.get(self.i).unwrap_or(&Tok::Eof)
    }

    pub(crate) fn is_p(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::P(q) if q == p)
    }

    pub(crate) fn is_id(&self, n: &str) -> bool {
        matches!(self.peek(), Tok::Id(q) if q == n)
    }

    pub(crate) fn eat(&mut self, p: &str) -> bool {
        if self.is_p(p) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    pub(crate) fn expect(&mut self, p: &str) -> Result<(), String> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(format!("expected {p:?}, found {:?}", self.peek()))
        }
    }

    pub(crate) fn ident(&mut self) -> Result<String, String> {
        match self.peek().clone() {
            Tok::Id(n) => {
                self.i += 1;
                Ok(n)
            }
            t => Err(format!("expected a name, found {t:?}")),
        }
    }

    pub(crate) fn program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut out = Vec::new();
        while *self.peek() != Tok::Eof {
            out.push(self.stmt()?);
        }
        Ok(out)
    }

    pub(crate) fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect("{")?;
        let mut out = Vec::new();
        while !self.is_p("}") {
            if *self.peek() == Tok::Eof {
                return Err("unclosed block".into());
            }
            out.push(self.stmt()?);
        }
        self.expect("}")?;
        Ok(out)
    }

    pub(crate) fn stmt(&mut self) -> Result<Stmt, String> {
        if self.is_p("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.eat(";") {
            return Ok(Stmt::Block(Vec::new()));
        }
        if let Tok::Id(k) = self.peek().clone() {
            match k.as_str() {
                "var" | "let" | "const" => {
                    self.i += 1;
                    let mut decls = Vec::new();
                    loop {
                        let name = self.ident()?;
                        let init = if self.eat("=") {
                            Some(self.assign()?)
                        } else {
                            None
                        };
                        decls.push((name, init));
                        if !self.eat(",") {
                            break;
                        }
                    }
                    self.eat(";");
                    return Ok(Stmt::Var(decls));
                }
                "function" if matches!(self.t.get(self.i + 1), Some(Tok::Id(_))) => {
                    self.i += 1;
                    let name = self.ident()?;
                    let def = self.func_rest()?;
                    return Ok(Stmt::Func(name, def));
                }
                "if" => {
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.assign()?;
                    self.expect(")")?;
                    let a = Box::new(self.stmt()?);
                    let b = if self.is_id("else") {
                        self.i += 1;
                        Some(Box::new(self.stmt()?))
                    } else {
                        None
                    };
                    return Ok(Stmt::If(c, a, b));
                }
                "while" => {
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.assign()?;
                    self.expect(")")?;
                    return Ok(Stmt::While(c, Box::new(self.stmt()?)));
                }
                "for" => {
                    self.i += 1;
                    self.expect("(")?;
                    let init = if self.eat(";") {
                        None
                    } else {
                        Some(Box::new(self.stmt()?))
                    };
                    let cond = if self.is_p(";") {
                        None
                    } else {
                        Some(self.assign()?)
                    };
                    self.expect(";")?;
                    let upd = if self.is_p(")") {
                        None
                    } else {
                        Some(self.assign()?)
                    };
                    self.expect(")")?;
                    return Ok(Stmt::For(init, cond, upd, Box::new(self.stmt()?)));
                }
                "return" => {
                    self.i += 1;
                    let e = if self.is_p(";") || self.is_p("}") || *self.peek() == Tok::Eof {
                        None
                    } else {
                        Some(self.assign()?)
                    };
                    self.eat(";");
                    return Ok(Stmt::Return(e));
                }
                "break" => {
                    self.i += 1;
                    self.eat(";");
                    return Ok(Stmt::Break);
                }
                "continue" => {
                    self.i += 1;
                    self.eat(";");
                    return Ok(Stmt::Continue);
                }
                _ => {}
            }
        }
        let e = self.assign()?;
        self.eat(";");
        Ok(Stmt::Expr(e))
    }

    pub(crate) fn func_rest(&mut self) -> Result<Arc<FuncDef>, String> {
        self.expect("(")?;
        let mut params = Vec::new();
        while !self.is_p(")") {
            params.push(self.ident()?);
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        let body = self.block()?;
        Ok(Arc::new(FuncDef { params, body }))
    }

    pub(crate) fn is_arrow(&self) -> bool {
        match self.t.get(self.i) {
            Some(Tok::Id(n)) if !KEYWORDS.contains(&n.as_str()) => {
                matches!(self.t.get(self.i + 1), Some(Tok::P(p)) if p == "=>")
            }
            Some(Tok::P(p)) if p == "(" => {
                let mut depth = 0;
                let mut j = self.i;
                while j < self.t.len() {
                    match &self.t[j] {
                        Tok::P(q) if q == "(" => depth += 1,
                        Tok::P(q) if q == ")" => {
                            depth -= 1;
                            if depth == 0 {
                                return matches!(self.t.get(j + 1), Some(Tok::P(r)) if r == "=>");
                            }
                        }
                        Tok::Eof => return false,
                        _ => {}
                    }
                    j += 1;
                }
                false
            }
            _ => false,
        }
    }

    pub(crate) fn arrow(&mut self) -> Result<Expr, String> {
        let mut params = Vec::new();
        if self.eat("(") {
            while !self.is_p(")") {
                params.push(self.ident()?);
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(")")?;
        } else {
            params.push(self.ident()?);
        }
        self.expect("=>")?;
        let body = if self.is_p("{") {
            self.block()?
        } else {
            vec![Stmt::Return(Some(self.assign()?))]
        };
        Ok(Expr::Func(Arc::new(FuncDef { params, body })))
    }

    pub(crate) fn assign(&mut self) -> Result<Expr, String> {
        if self.is_arrow() {
            return self.arrow();
        }
        let lhs = self.cond()?;
        if let Tok::P(p) = self.peek().clone() {
            if matches!(p.as_str(), "=" | "+=" | "-=" | "*=" | "/=") {
                if !matches!(lhs, Expr::Ident(_) | Expr::Member(..)) {
                    return Err("invalid assignment target".into());
                }
                self.i += 1;
                let rhs = self.assign()?;
                return Ok(Expr::Assign(p, Box::new(lhs), Box::new(rhs)));
            }
        }
        Ok(lhs)
    }

    pub(crate) fn cond(&mut self) -> Result<Expr, String> {
        let c = self.bin(1)?;
        if self.eat("?") {
            let a = self.assign()?;
            self.expect(":")?;
            let b = self.assign()?;
            return Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    pub(crate) fn prec(&self) -> Option<(String, u8)> {
        let (s, p) = match self.peek() {
            Tok::P(p) => (
                p.clone(),
                match p.as_str() {
                    "||" => 1,
                    "&&" => 2,
                    "==" | "!=" | "===" | "!==" => 3,
                    "<" | ">" | "<=" | ">=" => 4,
                    "+" | "-" => 5,
                    "*" | "/" | "%" => 6,
                    _ => return None,
                },
            ),
            Tok::Id(n) if n == "in" => ("in".to_string(), 4),
            _ => return None,
        };
        Some((s, p))
    }

    pub(crate) fn bin(&mut self, min: u8) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        while let Some((op, p)) = self.prec() {
            if p < min {
                break;
            }
            self.i += 1;
            let rhs = self.bin(p + 1)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    pub(crate) fn unary(&mut self) -> Result<Expr, String> {
        if let Tok::P(p) = self.peek().clone() {
            if matches!(p.as_str(), "!" | "-" | "+") {
                self.i += 1;
                return Ok(Expr::Un(p, Box::new(self.unary()?)));
            }
        }
        if self.is_p("++") || self.is_p("--") {
            // prefix `++x` / `--x`: the same as `x += 1` / `x -= 1`, which yields the new value
            let op = if self.is_p("++") { "+=" } else { "-=" };
            self.i += 1;
            let target = self.unary()?;
            if !matches!(target, Expr::Ident(_) | Expr::Member(..)) {
                return Err("invalid increment target".into());
            }
            return Ok(Expr::Assign(
                op.into(),
                Box::new(target),
                Box::new(Expr::Num(1.0)),
            ));
        }
        if self.is_id("typeof") {
            self.i += 1;
            return Ok(Expr::Un("typeof".into(), Box::new(self.unary()?)));
        }
        self.postfix()
    }

    pub(crate) fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        loop {
            if self.eat(".") {
                let n = self.ident()?;
                e = Expr::Member(Box::new(e), Box::new(Expr::Str(n)));
            } else if self.eat("[") {
                let k = self.assign()?;
                self.expect("]")?;
                e = Expr::Member(Box::new(e), Box::new(k));
            } else if self.eat("(") {
                let mut args = Vec::new();
                while !self.is_p(")") {
                    args.push(self.assign()?);
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect(")")?;
                e = Expr::Call(Box::new(e), args);
            } else if self.is_p("++") || self.is_p("--") {
                let op = if self.is_p("++") { "+=" } else { "-=" };
                self.i += 1;
                e = Expr::Assign(op.into(), Box::new(e), Box::new(Expr::Num(1.0)));
            } else {
                break;
            }
        }
        Ok(e)
    }

    pub(crate) fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Tok::Num(n) => {
                self.i += 1;
                Ok(Expr::Num(n))
            }
            Tok::Str(s) => {
                self.i += 1;
                Ok(Expr::Str(s))
            }
            Tok::Id(n) => {
                self.i += 1;
                match n.as_str() {
                    "true" => Ok(Expr::Bool(true)),
                    "false" => Ok(Expr::Bool(false)),
                    "null" => Ok(Expr::Null),
                    "undefined" => Ok(Expr::Undef),
                    "function" => {
                        if matches!(self.peek(), Tok::Id(_)) {
                            self.i += 1;
                        }
                        Ok(Expr::Func(self.func_rest()?))
                    }
                    _ => Ok(Expr::Ident(n)),
                }
            }
            Tok::P(p) if p == "(" => {
                self.i += 1;
                let e = self.assign()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::P(p) if p == "[" => {
                self.i += 1;
                let mut items = Vec::new();
                while !self.is_p("]") {
                    items.push(self.assign()?);
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect("]")?;
                Ok(Expr::Arr(items))
            }
            Tok::P(p) if p == "{" => {
                self.i += 1;
                let mut props = Vec::new();
                while !self.is_p("}") {
                    let key = match self.peek().clone() {
                        Tok::Id(n) => n,
                        Tok::Str(s) => s,
                        Tok::Num(n) => fmt_num(n),
                        t => return Err(format!("bad object key {t:?}")),
                    };
                    self.i += 1;
                    self.expect(":")?;
                    props.push((key, self.assign()?));
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect("}")?;
                Ok(Expr::Obj(props))
            }
            t => Err(format!("unexpected {t:?}")),
        }
    }
}
