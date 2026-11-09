//! Simplified C block: a C-subset interpreter.
//!
//! Supported: declarations (double float int long short char bool unsigned signed static const),
//! one-dimensional arrays with initializer lists, all C operators (arithmetic, comparison, logical,
//! bitwise, shifts, assignment and compound assignment, ++/--, ?:, comma, casts), if/else, while,
//! do-while, for, switch/case/default, break, continue, return, blocks, `#define NAME value`,
//! math.h functions and M_PI / M_E.
//!
//! As documented for this block, every value is a double: integer division is evaluated in
//! floating point; use (int) or floor() to truncate. Variables declared in the "global" section, and
//! `static` variables in the step code, keep their values between steps; uninitialised variables are 0.
//! The step code sees x1..xn (inputs), y1..ym (outputs, start at 0 every step), t and delt.

use crate::clone_box;
use crate::expr::js_round;
use crate::sim::*;
use crate::util::{js_to_i32, js_to_u32};
use std::collections::HashMap;

/* ------------------------------------------------------------------ tokens */
#[derive(Clone, Debug, PartialEq)]
enum T {
    Num(f64),
    Id(String),
    Op(&'static str),
}

const OPS: &[&str] = &[
    "<<=", ">>=", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "&", "|", "^", "~", "!", "<", ">", "=", "?", ":", ";", ",", "(", ")", "[", "]", "{",
    "}",
];

fn strip_comments(src: &str) -> String {
    let c: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        if c[i] == '/' && i + 1 < c.len() && c[i + 1] == '*' {
            i += 2;
            while i + 1 < c.len() && !(c[i] == '*' && c[i + 1] == '/') {
                if c[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i += 2;
            out.push(' ');
            continue;
        }
        if c[i] == '/' && i + 1 < c.len() && c[i + 1] == '/' {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

fn lex(src: &str, macros: &mut HashMap<String, Vec<T>>) -> Result<Vec<T>, String> {
    let text = strip_comments(src);
    let mut raw = Vec::new();
    for line in text.split('\n') {
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix('#') {
            let rest = rest.trim_start();
            if let Some(d) = rest.strip_prefix("define") {
                let d = d.trim();
                let name: String = d.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                if name.is_empty() {
                    return Err("#define 格式錯誤".into());
                }
                let body = &d[name.len()..];
                if body.starts_with('(') {
                    return Err(format!("不支援帶參數的巨集 {}()", name));
                }
                let toks = lex_line(body)?;
                macros.insert(name, toks);
            }
            continue; // #include and other directives are ignored
        }
        raw.extend(lex_line(line)?);
    }
    // macro expansion (object-like, nested up to a fixed depth)
    let mut out = raw;
    for _ in 0..16 {
        let mut changed = false;
        let mut nx = Vec::with_capacity(out.len());
        for t in out {
            if let T::Id(n) = &t {
                if let Some(m) = macros.get(n) {
                    nx.extend(m.iter().cloned());
                    changed = true;
                    continue;
                }
            }
            nx.push(t);
        }
        out = nx;
        if !changed {
            break;
        }
    }
    Ok(out)
}

fn lex_line(s: &str) -> Result<Vec<T>, String> {
    let c: Vec<char> = s.chars().collect();
    let n = c.len();
    let mut i = 0;
    let mut out = Vec::new();
    while i < n {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < n && c[i + 1].is_ascii_digit()) {
            let st = i;
            let v;
            if ch == '0' && i + 1 < n && (c[i + 1] == 'x' || c[i + 1] == 'X') {
                i += 2;
                let hs = i;
                while i < n && c[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let h: String = c[hs..i].iter().collect();
                v = u64::from_str_radix(&h, 16).map_err(|_| "十六進位數字格式錯誤".to_string())? as f64;
            } else {
                while i < n && c[i].is_ascii_digit() {
                    i += 1;
                }
                let mut is_f = false;
                if i < n && c[i] == '.' {
                    is_f = true;
                    i += 1;
                    while i < n && c[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < n && (c[i] == 'e' || c[i] == 'E') {
                    let mut k = i + 1;
                    if k < n && (c[k] == '+' || c[k] == '-') {
                        k += 1;
                    }
                    if k < n && c[k].is_ascii_digit() {
                        is_f = true;
                        while k < n && c[k].is_ascii_digit() {
                            k += 1;
                        }
                        i = k;
                    }
                }
                let txt: String = c[st..i].iter().collect();
                if !is_f && txt.len() > 1 && txt.starts_with('0') && txt.chars().all(|d| d.is_digit(8)) {
                    v = u64::from_str_radix(&txt[1..], 8).unwrap_or(0) as f64;
                } else {
                    v = crate::expr::parse_js_float(&txt);
                }
            }
            while i < n && matches!(c[i], 'u' | 'U' | 'l' | 'L' | 'f' | 'F') {
                i += 1;
            }
            if i < n && (c[i].is_ascii_alphanumeric() || c[i] == '_') {
                return Err(format!("數字格式錯誤：{}", c[st..=i].iter().collect::<String>()));
            }
            out.push(T::Num(v));
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let st = i;
            while i < n && (c[i].is_ascii_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(T::Id(c[st..i].iter().collect()));
            continue;
        }
        if ch == '\'' {
            // character literal
            let (v, len) = if i + 1 < n && c[i + 1] == '\\' && i + 3 < n && c[i + 3] == '\'' {
                let e = match c[i + 2] {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '0' => '\0',
                    o => o,
                };
                (e as u32 as f64, 4)
            } else if i + 2 < n && c[i + 2] == '\'' {
                (c[i + 1] as u32 as f64, 3)
            } else {
                return Err("字元常數格式錯誤".into());
            };
            out.push(T::Num(v));
            i += len;
            continue;
        }
        if ch == '"' {
            return Err("不支援字串".into());
        }
        let mut found = false;
        for op in OPS {
            let l = op.chars().count();
            if i + l <= n && c[i..i + l].iter().collect::<String>() == *op {
                out.push(T::Op(op));
                i += l;
                found = true;
                break;
            }
        }
        if !found {
            return Err(format!("無法辨識的字元 \"{}\"", ch));
        }
    }
    Ok(out)
}

/* ------------------------------------------------------------------ syntax tree */
#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    G(usize),
    L(usize),
    P(usize),
}
#[derive(Clone, Copy, Debug)]
enum Mf {
    Sqrt, Sin, Cos, Tan, Asin, Acos, Atan, Atan2, Exp, Log, Log10, Log2, Pow, Floor, Ceil, Sinh, Cosh, Tanh, Fmin, Fmax, Round, Trunc, Fabs, Fmod, Hypot, Cbrt, IsNan, IsInf,
}
fn mf_of(n: &str) -> Option<(Mf, usize)> {
    use Mf::*;
    Some(match n {
        "sqrt" => (Sqrt, 1),
        "sin" => (Sin, 1),
        "cos" => (Cos, 1),
        "tan" => (Tan, 1),
        "asin" => (Asin, 1),
        "acos" => (Acos, 1),
        "atan" => (Atan, 1),
        "atan2" => (Atan2, 2),
        "exp" => (Exp, 1),
        "log" => (Log, 1),
        "log10" => (Log10, 1),
        "log2" => (Log2, 1),
        "pow" => (Pow, 2),
        "floor" => (Floor, 1),
        "ceil" => (Ceil, 1),
        "sinh" => (Sinh, 1),
        "cosh" => (Cosh, 1),
        "tanh" => (Tanh, 1),
        "fmin" => (Fmin, 2),
        "fmax" => (Fmax, 2),
        "round" => (Round, 1),
        "trunc" => (Trunc, 1),
        "fabs" | "abs" | "labs" => (Fabs, 1),
        "fmod" => (Fmod, 2),
        "hypot" => (Hypot, 2),
        "cbrt" => (Cbrt, 1),
        "isnan" => (IsNan, 1),
        "isinf" => (IsInf, 1),
        _ => return None,
    })
}
#[derive(Clone, Debug)]
enum Lv {
    V(Slot),
    I(Slot, Box<Ex>),
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Bo {
    Add, Sub, Mul, Div, Mod, Shl, Shr, Lt, Gt, Le, Ge, Eq, Ne, BAnd, BOr, BXor,
}
#[derive(Clone, Copy, Debug)]
enum Cast {
    Int,
    Bool,
    None,
}
#[derive(Clone, Debug)]
enum Ex {
    Num(f64),
    Var(Slot),
    Idx(Slot, Box<Ex>),
    Assign(Lv, Option<Bo>, Box<Ex>),
    Inc(Lv, f64, bool), // (target, delta, prefix)
    Neg(Box<Ex>),
    Not(Box<Ex>),
    BNot(Box<Ex>),
    Bin(Bo, Box<Ex>, Box<Ex>),
    And(Box<Ex>, Box<Ex>),
    Or(Box<Ex>, Box<Ex>),
    Tern(Box<Ex>, Box<Ex>, Box<Ex>),
    Call(Mf, Vec<Ex>),
    Comma(Box<Ex>, Box<Ex>),
    Cast(Cast, Box<Ex>),
}
#[derive(Clone, Debug)]
struct DeclItem {
    slot: Slot,
    arr: Option<Ex>,
    init: Option<Ex>,
    list: Option<Vec<Ex>>,
}
#[derive(Clone, Debug)]
enum St {
    Decl(Vec<DeclItem>),
    Ex(Ex),
    If(Ex, Box<St>, Option<Box<St>>),
    While(Ex, Box<St>),
    Do(Box<St>, Ex),
    For(Option<Box<St>>, Option<Ex>, Option<Ex>, Box<St>),
    Switch(Ex, Vec<(Option<Ex>, usize)>, Vec<St>),
    Break,
    Continue,
    Return,
    Block(Vec<St>),
    Empty,
}

/* ------------------------------------------------------------------ parser */
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Scalar,
    Array,
}
struct Ps<'a> {
    t: &'a [T],
    i: usize,
    scopes: Vec<HashMap<String, (Slot, Kind)>>,
    n_glob: usize,
    n_loc: usize,
    glob_mode: bool,
    statics: Vec<DeclItem>,
    /// current expression nesting (unary / parenthesis levels) and statement nesting
    depth: usize,
    sdepth: usize,
    /// operands parsed in the current statement (bounds left-deep operator chains)
    ops: usize,
}
/// limits that keep crafted C code from overflowing the stack (parse, evaluation and drop all recurse)
const MAX_DEPTH: usize = 64;
const MAX_OPERANDS: usize = 1000;
type PR<T> = Result<T, String>;
const TYPES: &[&str] = &["double", "float", "int", "long", "short", "char", "bool", "unsigned", "signed", "static", "const", "volatile", "register", "extern", "_Bool"];

impl<'a> Ps<'a> {
    fn peek(&self) -> Option<&T> {
        self.t.get(self.i)
    }
    fn is_op(&self, o: &str) -> bool {
        matches!(self.peek(), Some(T::Op(x)) if *x == o)
    }
    fn is_id(&self, s: &str) -> bool {
        matches!(self.peek(), Some(T::Id(x)) if x == s)
    }
    fn eat(&mut self, o: &str) -> bool {
        if self.is_op(o) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, o: &str) -> PR<()> {
        if self.eat(o) {
            Ok(())
        } else {
            Err(format!("缺少 \"{}\"（靠近 {}）", o, self.near()))
        }
    }
    fn near(&self) -> String {
        match self.peek() {
            None => "結尾".into(),
            Some(T::Num(v)) => crate::util::js_num_str(*v),
            Some(T::Id(s)) => s.clone(),
            Some(T::Op(o)) => o.to_string(),
        }
    }
    fn lookup(&self, n: &str) -> Option<(Slot, Kind)> {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(n) {
                return Some(*v);
            }
        }
        None
    }
    fn is_type_start(&self) -> bool {
        matches!(self.peek(), Some(T::Id(x)) if TYPES.contains(&x.as_str()))
    }
    fn block_items(&mut self, until_brace: bool) -> PR<Vec<St>> {
        let mut v = Vec::new();
        loop {
            if until_brace {
                if self.is_op("}") {
                    self.i += 1;
                    return Ok(v);
                }
                if self.peek().is_none() {
                    return Err("缺少 \"}\"".into());
                }
            } else if self.peek().is_none() {
                return Ok(v);
            }
            v.push(self.stmt()?);
        }
    }
    fn decl(&mut self) -> PR<St> {
        let mut is_static = false;
        let mut saw_type = false;
        while let Some(T::Id(x)) = self.peek() {
            if TYPES.contains(&x.as_str()) {
                if x == "static" {
                    is_static = true;
                }
                saw_type = true;
                self.i += 1;
            } else {
                break;
            }
        }
        if !saw_type {
            return Err("宣告格式錯誤".into());
        }
        let mut items = Vec::new();
        loop {
            let name = match self.peek() {
                Some(T::Id(n)) => n.clone(),
                _ => return Err(format!("宣告缺少變數名稱（靠近 {}）", self.near())),
            };
            self.i += 1;
            if self.is_op("(") {
                return Err(format!("不支援自訂函數 {}()", name));
            }
            let mut arr = None;
            if self.eat("[") {
                if self.is_op("]") {
                    arr = Some(Ex::Num(-1.0)); // size from initializer
                } else {
                    arr = Some(self.expr()?);
                }
                self.expect("]")?;
                if self.is_op("[") {
                    return Err("僅支援一維陣列".into());
                }
            }
            let mut init = None;
            let mut list = None;
            if self.eat("=") {
                if self.eat("{") {
                    let mut l = Vec::new();
                    if !self.is_op("}") {
                        { self.ops = 0; l.push(self.assign()?); }
                        while self.eat(",") {
                            if self.is_op("}") {
                                break;
                            }
                            { self.ops = 0; l.push(self.assign()?); }
                        }
                    }
                    self.expect("}")?;
                    list = Some(l);
                } else {
                    init = Some(self.assign()?);
                }
            }
            let kind = if arr.is_some() { Kind::Array } else { Kind::Scalar };
            if list.is_some() && arr.is_none() {
                return Err(format!("{} 不是陣列，不能用 {{...}} 初始化", name));
            }
            let persistent = self.glob_mode || is_static;
            let slot = if persistent {
                let s = Slot::G(self.n_glob);
                self.n_glob += 1;
                s
            } else {
                let s = Slot::L(self.n_loc);
                self.n_loc += 1;
                s
            };
            let item = DeclItem { slot, arr, init, list };
            self.scopes.last_mut().unwrap().insert(name, (slot, kind));
            if is_static && !self.glob_mode {
                self.statics.push(item);
            } else {
                items.push(item);
            }
            if self.eat(",") {
                continue;
            }
            break;
        }
        Ok(St::Decl(items))
    }
    fn stmt(&mut self) -> PR<St> {
        if self.sdepth >= MAX_DEPTH {
            return Err(format!("程式區塊巢狀過深（超過 {} 層）", MAX_DEPTH));
        }
        let saved = self.ops;
        self.ops = 0;
        self.sdepth += 1;
        let r = self.stmt_inner();
        self.sdepth -= 1;
        self.ops = saved;
        r
    }
    fn stmt_inner(&mut self) -> PR<St> {
        if self.is_type_start() {
            let d = self.decl()?;
            self.expect(";")?;
            return Ok(d);
        }
        if self.eat(";") {
            return Ok(St::Empty);
        }
        if self.eat("{") {
            self.scopes.push(HashMap::new());
            let b = self.block_items(true);
            self.scopes.pop();
            return Ok(St::Block(b?));
        }
        if let Some(T::Id(k)) = self.peek().cloned() {
            match k.as_str() {
                "if" => {
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    let a = self.scoped_stmt()?;
                    let b = if self.is_id("else") {
                        self.i += 1;
                        Some(Box::new(self.scoped_stmt()?))
                    } else {
                        None
                    };
                    return Ok(St::If(c, Box::new(a), b));
                }
                "while" => {
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    let b = self.scoped_stmt()?;
                    return Ok(St::While(c, Box::new(b)));
                }
                "do" => {
                    self.i += 1;
                    let b = self.scoped_stmt()?;
                    if !self.is_id("while") {
                        return Err("do 之後缺少 while".into());
                    }
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    self.expect(";")?;
                    return Ok(St::Do(Box::new(b), c));
                }
                "for" => {
                    self.i += 1;
                    self.expect("(")?;
                    self.scopes.push(HashMap::new());
                    let r = (|| -> PR<St> {
                        let init = if self.eat(";") {
                            None
                        } else if self.is_type_start() {
                            let d = self.decl()?;
                            self.expect(";")?;
                            Some(Box::new(d))
                        } else {
                            let e = self.expr()?;
                            self.expect(";")?;
                            Some(Box::new(St::Ex(e)))
                        };
                        let cond = if self.is_op(";") { None } else { Some(self.expr()?) };
                        self.expect(";")?;
                        let step = if self.is_op(")") { None } else { Some(self.expr()?) };
                        self.expect(")")?;
                        let body = self.scoped_stmt()?;
                        Ok(St::For(init, cond, step, Box::new(body)))
                    })();
                    self.scopes.pop();
                    return r;
                }
                "switch" => {
                    self.i += 1;
                    self.expect("(")?;
                    let v = self.expr()?;
                    self.expect(")")?;
                    self.expect("{")?;
                    self.scopes.push(HashMap::new());
                    let r = (|| -> PR<St> {
                        let mut labels = Vec::new();
                        let mut body = Vec::new();
                        loop {
                            if self.eat("}") {
                                break;
                            }
                            if self.peek().is_none() {
                                return Err("switch 缺少 \"}\"".into());
                            }
                            if self.is_id("case") {
                                self.i += 1;
                                let e = self.ternary()?;
                                self.expect(":")?;
                                labels.push((Some(e), body.len()));
                                continue;
                            }
                            if self.is_id("default") {
                                self.i += 1;
                                self.expect(":")?;
                                labels.push((None, body.len()));
                                continue;
                            }
                            body.push(self.stmt()?);
                        }
                        Ok(St::Switch(v, labels, body))
                    })();
                    self.scopes.pop();
                    return r;
                }
                "break" => {
                    self.i += 1;
                    self.expect(";")?;
                    return Ok(St::Break);
                }
                "continue" => {
                    self.i += 1;
                    self.expect(";")?;
                    return Ok(St::Continue);
                }
                "return" => {
                    self.i += 1;
                    if !self.is_op(";") {
                        self.expr()?;
                    }
                    self.expect(";")?;
                    return Ok(St::Return);
                }
                "else" => return Err("else 沒有對應的 if".into()),
                "case" | "default" => return Err(format!("{} 只能用在 switch 中", k)),
                _ => {}
            }
        }
        let e = self.expr()?;
        self.expect(";")?;
        Ok(St::Ex(e))
    }
    fn scoped_stmt(&mut self) -> PR<St> {
        self.scopes.push(HashMap::new());
        let r = self.stmt();
        self.scopes.pop();
        r
    }
    fn expr(&mut self) -> PR<Ex> {
        let mut a = self.assign()?;
        while self.eat(",") {
            let b = self.assign()?;
            a = Ex::Comma(Box::new(a), Box::new(b));
        }
        Ok(a)
    }
    fn to_lv(&self, e: Ex) -> PR<Lv> {
        match e {
            Ex::Var(s) => Ok(Lv::V(s)),
            Ex::Idx(s, i) => Ok(Lv::I(s, i)),
            _ => Err("指定運算的左邊必須是變數".into()),
        }
    }
    fn assign(&mut self) -> PR<Ex> {
        let lhs = self.ternary()?;
        let op = match self.peek() {
            Some(T::Op("=")) => Some(None),
            Some(T::Op("+=")) => Some(Some(Bo::Add)),
            Some(T::Op("-=")) => Some(Some(Bo::Sub)),
            Some(T::Op("*=")) => Some(Some(Bo::Mul)),
            Some(T::Op("/=")) => Some(Some(Bo::Div)),
            Some(T::Op("%=")) => Some(Some(Bo::Mod)),
            Some(T::Op("<<=")) => Some(Some(Bo::Shl)),
            Some(T::Op(">>=")) => Some(Some(Bo::Shr)),
            Some(T::Op("&=")) => Some(Some(Bo::BAnd)),
            Some(T::Op("|=")) => Some(Some(Bo::BOr)),
            Some(T::Op("^=")) => Some(Some(Bo::BXor)),
            _ => None,
        };
        if let Some(op) = op {
            self.i += 1;
            let lv = self.to_lv(lhs)?;
            if let Lv::V(Slot::P(_)) = lv {
                // inputs / time are plain values; assigning them is allowed (local copy) in C semantics
            }
            let rhs = self.assign()?;
            return Ok(Ex::Assign(lv, op, Box::new(rhs)));
        }
        Ok(lhs)
    }
    fn ternary(&mut self) -> PR<Ex> {
        let c = self.or()?;
        if self.eat("?") {
            let a = self.expr()?;
            self.expect(":")?;
            let b = self.ternary()?;
            return Ok(Ex::Tern(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }
    fn or(&mut self) -> PR<Ex> {
        let mut a = self.and()?;
        while self.eat("||") {
            let b = self.and()?;
            a = Ex::Or(Box::new(a), Box::new(b));
        }
        Ok(a)
    }
    fn and(&mut self) -> PR<Ex> {
        let mut a = self.bin_level(0)?;
        while self.eat("&&") {
            let b = self.bin_level(0)?;
            a = Ex::And(Box::new(a), Box::new(b));
        }
        Ok(a)
    }
    fn bin_level(&mut self, lvl: usize) -> PR<Ex> {
        const LV: &[&[(&str, Bo)]] = &[
            &[("|", Bo::BOr)],
            &[("^", Bo::BXor)],
            &[("&", Bo::BAnd)],
            &[("==", Bo::Eq), ("!=", Bo::Ne)],
            &[("<", Bo::Lt), (">", Bo::Gt), ("<=", Bo::Le), (">=", Bo::Ge)],
            &[("<<", Bo::Shl), (">>", Bo::Shr)],
            &[("+", Bo::Add), ("-", Bo::Sub)],
            &[("*", Bo::Mul), ("/", Bo::Div), ("%", Bo::Mod)],
        ];
        if lvl >= LV.len() {
            return self.unary();
        }
        let mut a = self.bin_level(lvl + 1)?;
        'outer: loop {
            for (o, b) in LV[lvl] {
                if self.is_op(o) {
                    self.i += 1;
                    let r = self.bin_level(lvl + 1)?;
                    a = Ex::Bin(*b, Box::new(a), Box::new(r));
                    continue 'outer;
                }
            }
            return Ok(a);
        }
    }
    fn unary(&mut self) -> PR<Ex> {
        self.ops += 1;
        if self.ops > MAX_OPERANDS {
            return Err(format!("運算式過長（單一敘述超過 {} 個運算元）", MAX_OPERANDS));
        }
        if self.depth >= MAX_DEPTH {
            return Err(format!("運算式巢狀過深（超過 {} 層）", MAX_DEPTH));
        }
        self.depth += 1;
        let r = self.unary_inner();
        self.depth -= 1;
        r
    }
    fn unary_inner(&mut self) -> PR<Ex> {
        if self.eat("-") {
            return Ok(Ex::Neg(Box::new(self.unary()?)));
        }
        if self.eat("+") {
            return self.unary();
        }
        if self.eat("!") {
            return Ok(Ex::Not(Box::new(self.unary()?)));
        }
        if self.eat("~") {
            return Ok(Ex::BNot(Box::new(self.unary()?)));
        }
        if self.is_op("++") || self.is_op("--") {
            let d = if self.is_op("++") { 1.0 } else { -1.0 };
            self.i += 1;
            let e = self.unary()?;
            return Ok(Ex::Inc(self.to_lv(e)?, d, true));
        }
        // cast: ( type ) unary
        if self.is_op("(") {
            if let Some(T::Id(x)) = self.t.get(self.i + 1) {
                if TYPES.contains(&x.as_str()) {
                    self.i += 1;
                    let mut cast = Cast::None;
                    while let Some(T::Id(x)) = self.peek() {
                        if !TYPES.contains(&x.as_str()) {
                            break;
                        }
                        match x.as_str() {
                            "int" | "long" | "short" | "unsigned" | "signed" | "char" => cast = Cast::Int,
                            "bool" | "_Bool" => cast = Cast::Bool,
                            _ => {}
                        }
                        self.i += 1;
                    }
                    self.expect(")")?;
                    let e = self.unary()?;
                    return Ok(Ex::Cast(cast, Box::new(e)));
                }
            }
        }
        self.postfix()
    }
    fn postfix(&mut self) -> PR<Ex> {
        let mut e = self.primary()?;
        loop {
            if self.is_op("++") || self.is_op("--") {
                let d = if self.is_op("++") { 1.0 } else { -1.0 };
                self.i += 1;
                e = Ex::Inc(self.to_lv(e)?, d, false);
                continue;
            }
            return Ok(e);
        }
    }
    fn primary(&mut self) -> PR<Ex> {
        let t = match self.peek() {
            Some(t) => t.clone(),
            None => return Err("運算式不完整".into()),
        };
        self.i += 1;
        match t {
            T::Num(v) => Ok(Ex::Num(v)),
            T::Op("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            T::Id(n) => {
                if self.is_op("(") {
                    self.i += 1;
                    let (f, ar) = mf_of(&n).ok_or_else(|| format!("不支援的函數 {}()", n))?;
                    let mut args = Vec::new();
                    if !self.is_op(")") {
                        args.push(self.assign()?);
                        while self.eat(",") {
                            args.push(self.assign()?);
                        }
                    }
                    self.expect(")")?;
                    if args.len() != ar {
                        return Err(format!("{}() 需要 {} 個參數", n, ar));
                    }
                    return Ok(Ex::Call(f, args));
                }
                match n.as_str() {
                    "M_PI" => return Ok(Ex::Num(std::f64::consts::PI)),
                    "M_E" => return Ok(Ex::Num(std::f64::consts::E)),
                    "TRUE" | "true" => return Ok(Ex::Num(1.0)),
                    "FALSE" | "false" => return Ok(Ex::Num(0.0)),
                    _ => {}
                }
                let (slot, kind) = self.lookup(&n).ok_or_else(|| format!("未宣告的變數「{}」（請先宣告，例如 double {} = 0;）", n, n))?;
                if self.eat("[") {
                    if kind != Kind::Array {
                        return Err(format!("{} 不是陣列", n));
                    }
                    let i = self.expr()?;
                    self.expect("]")?;
                    return Ok(Ex::Idx(slot, Box::new(i)));
                }
                if kind == Kind::Array {
                    return Err(format!("陣列 {} 需要索引，例如 {}[0]", n, n));
                }
                Ok(Ex::Var(slot))
            }
            T::Op(o) => Err(format!("語法錯誤，靠近 \"{}\"", o)),
        }
    }
}

/* ------------------------------------------------------------------ runtime */
#[derive(Clone, Debug)]
enum V {
    N(f64),
    A(Vec<f64>),
}
enum Flow {
    Norm,
    Brk,
    Cont,
    Ret,
}
struct Rt<'a> {
    g: &'a mut Vec<V>,
    l: &'a mut Vec<V>,
    p: &'a [f64],
    budget: u64,
}
fn tb(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}
fn bf(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}
impl<'a> Rt<'a> {
    fn get(&self, s: Slot) -> f64 {
        match s {
            Slot::G(i) => match &self.g[i] {
                V::N(v) => *v,
                V::A(_) => f64::NAN,
            },
            Slot::L(i) => match &self.l[i] {
                V::N(v) => *v,
                V::A(_) => f64::NAN,
            },
            Slot::P(i) => self.p[i],
        }
    }
    fn set(&mut self, s: Slot, v: f64) {
        match s {
            Slot::G(i) => self.g[i] = V::N(v),
            Slot::L(i) => self.l[i] = V::N(v),
            Slot::P(_) => {}
        }
    }
    fn arr(&mut self, s: Slot) -> Option<&mut Vec<f64>> {
        let v = match s {
            Slot::G(i) => &mut self.g[i],
            Slot::L(i) => &mut self.l[i],
            Slot::P(_) => return None,
        };
        match v {
            V::A(a) => Some(a),
            _ => None,
        }
    }
    fn idx_get(&mut self, s: Slot, i: f64) -> f64 {
        match self.arr(s) {
            Some(a) => {
                if i >= 0.0 && i.fract() == 0.0 && (i as usize) < a.len() {
                    a[i as usize]
                } else {
                    f64::NAN
                }
            }
            None => f64::NAN,
        }
    }
    fn idx_set(&mut self, s: Slot, i: f64, v: f64) {
        if let Some(a) = self.arr(s) {
            if i >= 0.0 && i.fract() == 0.0 && i < 1e7 {
                let k = i as usize;
                if k >= a.len() {
                    a.resize(k + 1, 0.0);
                }
                a[k] = v;
            }
        }
    }
    fn lv_get(&mut self, lv: &Lv) -> Result<(f64, f64), String> {
        Ok(match lv {
            Lv::V(s) => (self.get(*s), 0.0),
            Lv::I(s, ie) => {
                let i = self.ev(ie)?;
                (self.idx_get(*s, i), i)
            }
        })
    }
    fn lv_set(&mut self, lv: &Lv, idx: f64, v: f64) {
        match lv {
            Lv::V(s) => self.set(*s, v),
            Lv::I(s, _) => self.idx_set(*s, idx, v),
        }
    }
    fn bin(op: Bo, a: f64, b: f64) -> f64 {
        match op {
            Bo::Add => a + b,
            Bo::Sub => a - b,
            Bo::Mul => a * b,
            Bo::Div => a / b,
            Bo::Mod => a % b,
            Bo::Shl => js_to_i32(a).wrapping_shl(js_to_u32(b) & 31) as f64,
            Bo::Shr => js_to_i32(a).wrapping_shr(js_to_u32(b) & 31) as f64,
            Bo::Lt => bf(a < b),
            Bo::Gt => bf(a > b),
            Bo::Le => bf(a <= b),
            Bo::Ge => bf(a >= b),
            Bo::Eq => bf(a == b),
            Bo::Ne => bf(a != b),
            Bo::BAnd => (js_to_i32(a) & js_to_i32(b)) as f64,
            Bo::BOr => (js_to_i32(a) | js_to_i32(b)) as f64,
            Bo::BXor => (js_to_i32(a) ^ js_to_i32(b)) as f64,
        }
    }
    fn ev(&mut self, e: &Ex) -> Result<f64, String> {
        Ok(match e {
            Ex::Num(v) => *v,
            Ex::Var(s) => self.get(*s),
            Ex::Idx(s, i) => {
                let i = self.ev(i)?;
                self.idx_get(*s, i)
            }
            Ex::Assign(lv, op, r) => {
                let (old, idx) = if op.is_some() {
                    self.lv_get(lv)?
                } else {
                    match lv {
                        Lv::V(_) => (0.0, 0.0),
                        Lv::I(_, ie) => (0.0, self.ev(ie)?),
                    }
                };
                let rv = self.ev(r)?;
                let v = match op {
                    None => rv,
                    Some(o) => Self::bin(*o, old, rv),
                };
                self.lv_set(lv, idx, v);
                v
            }
            Ex::Inc(lv, d, pre) => {
                let (old, idx) = self.lv_get(lv)?;
                let nv = old + d;
                self.lv_set(lv, idx, nv);
                if *pre {
                    nv
                } else {
                    old
                }
            }
            Ex::Neg(a) => -self.ev(a)?,
            Ex::Not(a) => bf(!tb(self.ev(a)?)),
            Ex::BNot(a) => (!js_to_i32(self.ev(a)?)) as f64,
            Ex::Bin(o, a, b) => {
                let x = self.ev(a)?;
                let y = self.ev(b)?;
                Self::bin(*o, x, y)
            }
            Ex::And(a, b) => bf(tb(self.ev(a)?) && tb(self.ev(b)?)),
            Ex::Or(a, b) => bf(tb(self.ev(a)?) || tb(self.ev(b)?)),
            Ex::Tern(c, a, b) => {
                if tb(self.ev(c)?) {
                    self.ev(a)?
                } else {
                    self.ev(b)?
                }
            }
            Ex::Comma(a, b) => {
                self.ev(a)?;
                self.ev(b)?
            }
            Ex::Cast(c, a) => {
                let v = self.ev(a)?;
                match c {
                    Cast::Int => v.trunc(),
                    Cast::Bool => bf(tb(v)),
                    Cast::None => v,
                }
            }
            Ex::Call(f, args) => {
                let a = self.ev(&args[0])?;
                let b = if args.len() > 1 { self.ev(&args[1])? } else { 0.0 };
                use Mf::*;
                match f {
                    Sqrt => a.sqrt(),
                    Sin => a.sin(),
                    Cos => a.cos(),
                    Tan => a.tan(),
                    Asin => a.asin(),
                    Acos => a.acos(),
                    Atan => a.atan(),
                    Atan2 => a.atan2(b),
                    Exp => a.exp(),
                    Log => a.ln(),
                    Log10 => a.log10(),
                    Log2 => a.log2(),
                    Pow => crate::expr::js_pow(a, b),
                    Floor => a.floor(),
                    Ceil => a.ceil(),
                    Sinh => a.sinh(),
                    Cosh => a.cosh(),
                    Tanh => a.tanh(),
                    Fmin => crate::expr::js_min(&[a, b]),
                    Fmax => crate::expr::js_max(&[a, b]),
                    Round => {
                        // C round(): half away from zero
                        if a.is_finite() {
                            a.round()
                        } else {
                            js_round(a)
                        }
                    }
                    Trunc => a.trunc(),
                    Fabs => a.abs(),
                    Fmod => a % b,
                    Hypot => a.hypot(b),
                    Cbrt => a.cbrt(),
                    IsNan => bf(a.is_nan()),
                    IsInf => bf(a.is_infinite()),
                }
            }
        })
    }
    fn decl(&mut self, items: &[DeclItem]) -> Result<(), String> {
        for it in items {
            if let Some(len) = &it.arr {
                let mut n = self.ev(len)?;
                let list: Vec<f64> = match &it.list {
                    Some(l) => {
                        let mut v = Vec::with_capacity(l.len());
                        for e in l {
                            v.push(self.ev(e)?);
                        }
                        v
                    }
                    None => vec![],
                };
                if n < 0.0 {
                    n = list.len() as f64;
                }
                let n = if n.is_finite() { (n.trunc().max(0.0) as usize).min(10_000_000) } else { 0 };
                let mut a = vec![0.0; n];
                for (i, v) in list.into_iter().enumerate() {
                    if i < a.len() {
                        a[i] = v;
                    }
                }
                match it.slot {
                    Slot::G(i) => self.g[i] = V::A(a),
                    Slot::L(i) => self.l[i] = V::A(a),
                    Slot::P(_) => {}
                }
            } else {
                let v = match &it.init {
                    Some(e) => self.ev(e)?,
                    None => 0.0,
                };
                self.set(it.slot, v);
            }
        }
        Ok(())
    }
    fn tick(&mut self) -> Result<(), String> {
        if self.budget == 0 {
            return Err("執行逾時（迴圈次數過多，可能是無窮迴圈）".into());
        }
        self.budget -= 1;
        Ok(())
    }
    fn run(&mut self, s: &St) -> Result<Flow, String> {
        match s {
            St::Decl(items) => {
                self.decl(items)?;
                Ok(Flow::Norm)
            }
            St::Ex(e) => {
                self.ev(e)?;
                Ok(Flow::Norm)
            }
            St::If(c, a, b) => {
                if tb(self.ev(c)?) {
                    self.run(a)
                } else if let Some(b) = b {
                    self.run(b)
                } else {
                    Ok(Flow::Norm)
                }
            }
            St::While(c, body) => {
                while tb(self.ev(c)?) {
                    self.tick()?;
                    match self.run(body)? {
                        Flow::Brk => break,
                        Flow::Ret => return Ok(Flow::Ret),
                        _ => {}
                    }
                }
                Ok(Flow::Norm)
            }
            St::Do(body, c) => {
                loop {
                    self.tick()?;
                    match self.run(body)? {
                        Flow::Brk => break,
                        Flow::Ret => return Ok(Flow::Ret),
                        _ => {}
                    }
                    if !tb(self.ev(c)?) {
                        break;
                    }
                }
                Ok(Flow::Norm)
            }
            St::For(init, cond, step, body) => {
                if let Some(i) = init {
                    self.run(i)?;
                }
                loop {
                    if let Some(c) = cond {
                        if !tb(self.ev(c)?) {
                            break;
                        }
                    }
                    self.tick()?;
                    match self.run(body)? {
                        Flow::Brk => break,
                        Flow::Ret => return Ok(Flow::Ret),
                        _ => {}
                    }
                    if let Some(st) = step {
                        self.ev(st)?;
                    }
                }
                Ok(Flow::Norm)
            }
            St::Switch(v, labels, body) => {
                let x = self.ev(v)?;
                let mut start = None;
                for (l, pos) in labels {
                    if let Some(le) = l {
                        if self.ev(le)? == x {
                            start = Some(*pos);
                            break;
                        }
                    }
                }
                if start.is_none() {
                    start = labels.iter().find(|(l, _)| l.is_none()).map(|(_, p)| *p);
                }
                if let Some(st) = start {
                    for s in &body[st..] {
                        match self.run(s)? {
                            Flow::Brk => break,
                            Flow::Cont => return Ok(Flow::Cont),
                            Flow::Ret => return Ok(Flow::Ret),
                            Flow::Norm => {}
                        }
                    }
                }
                Ok(Flow::Norm)
            }
            St::Break => Ok(Flow::Brk),
            St::Continue => Ok(Flow::Cont),
            St::Return => Ok(Flow::Ret),
            St::Block(v) => {
                for s in v {
                    match self.run(s)? {
                        Flow::Norm => {}
                        f => return Ok(f),
                    }
                }
                Ok(Flow::Norm)
            }
            St::Empty => Ok(Flow::Norm),
        }
    }
}

/* ------------------------------------------------------------------ block */
#[derive(Clone)]
pub struct CBlock {
    ins: Vec<i32>,
    outs: Vec<i32>,
    glob: Vec<St>,
    statics: Vec<DeclItem>,
    code: Vec<St>,
    n_glob: usize,
    n_loc: usize,
    out_slots: Vec<usize>,
    g: Vec<V>,
    pend: Option<Vec<V>>,
    p: Vec<f64>,
    name: String,
    id: i64,
}
const BUDGET: u64 = 50_000_000;
impl CBlock {
    fn err(&self, what: &str, m: String) -> SimError {
        SimError::el(format!("{}：C 程式碼{}：{}", self.name, what, m), self.id)
    }
}
impl Inst for CBlock {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { c: true, ..Default::default() }
    }
    fn ins(&self) -> Vec<i32> {
        self.ins.clone()
    }
    fn outs(&self) -> Vec<i32> {
        self.outs.clone()
    }
    fn init(&mut self, _e: &mut E) -> R<()> {
        let mut g = vec![V::N(0.0); self.n_glob];
        let mut l = vec![V::N(0.0); self.n_loc];
        let p: Vec<f64> = vec![0.0; self.p.len()];
        {
            let mut rt = Rt { g: &mut g, l: &mut l, p: &p, budget: BUDGET };
            for s in &self.glob {
                rt.run(s).map_err(|m| self.err("初始化錯誤", m))?;
            }
            rt.decl(&self.statics).map_err(|m| self.err("初始化錯誤", m))?;
        }
        self.g = g;
        self.pend = None;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        self.p[0] = t;
        self.p[1] = h;
        for (j, &i) in self.ins.iter().enumerate() {
            self.p[2 + j] = e.cv[i as usize];
        }
        let mut g = self.g.clone();
        let mut l = vec![V::N(0.0); self.n_loc];
        {
            let mut rt = Rt { g: &mut g, l: &mut l, p: &self.p, budget: BUDGET };
            for s in &self.code {
                match rt.run(s) {
                    Ok(Flow::Ret) => break,
                    Ok(_) => {}
                    Err(m) => return Err(self.err("執行錯誤", m)),
                }
            }
        }
        for (j, &o) in self.outs.iter().enumerate() {
            let v = match &l[self.out_slots[j]] {
                V::N(v) => *v,
                V::A(_) => f64::NAN,
            };
            e.cv[o as usize] = v;
            e.cev[o as usize] = NAN;
        }
        self.pend = Some(g);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, _e: &mut E) {
        if let Some(g) = self.pend.take() {
            self.g = g;
        }
    }
}

pub fn make(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let nin = ctx.int("nin", 1.0)?;
    let nout = ctx.int("nout", 1.0)?;
    if !(0..=10).contains(&nin) || !(1..=10).contains(&nout) {
        return Err(ctx.err("輸入/輸出數量需在 0~10 / 1~10 之間"));
    }
    let (nin, nout) = (nin as usize, nout as usize);
    let mut ins = vec![];
    let mut outs = vec![];
    for i in 0..nin {
        ins.push(ctx.c(i)?);
    }
    for i in 0..nout {
        outs.push(ctx.c(nin + i)?);
    }
    let gsrc = ctx.raw("glob", "");
    let ssrc = ctx.raw("code", "");
    let mut macros = HashMap::new();
    let gt = lex(&gsrc, &mut macros).map_err(|m| ctx.err(format!("C 程式碼語法錯誤：{}", m)))?;
    let st = lex(&ssrc, &mut macros).map_err(|m| ctx.err(format!("C 程式碼語法錯誤：{}", m)))?;
    // global section
    let mut ps = Ps { t: &gt, i: 0, scopes: vec![HashMap::new()], n_glob: 0, n_loc: 0, glob_mode: true, statics: vec![], depth: 0, sdepth: 0, ops: 0 };
    let glob = ps.block_items(false).map_err(|m| ctx.err(format!("C 程式碼語法錯誤（全域變數）：{}", m)))?;
    let gscope = ps.scopes.pop().unwrap();
    let n_glob = ps.n_glob;
    // step code: params t, delt, x1..xn; locals y1..ym first
    let mut base = HashMap::new();
    base.insert("t".to_string(), (Slot::P(0), Kind::Scalar));
    base.insert("delt".to_string(), (Slot::P(1), Kind::Scalar));
    for i in 0..nin {
        base.insert(format!("x{}", i + 1), (Slot::P(2 + i), Kind::Scalar));
    }
    let mut out_slots = vec![];
    for i in 0..nout {
        base.insert(format!("y{}", i + 1), (Slot::L(i), Kind::Scalar));
        out_slots.push(i);
    }
    let mut ps2 = Ps { t: &st, i: 0, scopes: vec![base, gscope], n_glob, n_loc: nout, glob_mode: false, statics: vec![], depth: 0, sdepth: 0, ops: 0 };
    let code = ps2.block_items(false).map_err(|m| ctx.err(format!("C 程式碼語法錯誤：{}", m)))?;
    let statics = std::mem::take(&mut ps2.statics);
    Ok(vec![Box::new(CBlock {
        ins,
        outs,
        glob,
        statics,
        code,
        n_glob: ps2.n_glob,
        n_loc: ps2.n_loc,
        out_slots,
        g: vec![],
        pend: None,
        p: vec![0.0; 2 + nin],
        name: ctx.name.clone(),
        id: ctx.id,
    })])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run1(glob: &str, code: &str, x: f64) -> f64 {
        let mut m = HashMap::new();
        let gt = lex(glob, &mut m).unwrap();
        let st = lex(code, &mut m).unwrap();
        let mut ps = Ps { t: &gt, i: 0, scopes: vec![HashMap::new()], n_glob: 0, n_loc: 0, glob_mode: true, statics: vec![], depth: 0, sdepth: 0, ops: 0 };
        let g = ps.block_items(false).unwrap();
        let gs = ps.scopes.pop().unwrap();
        let mut base = HashMap::new();
        base.insert("t".to_string(), (Slot::P(0), Kind::Scalar));
        base.insert("delt".to_string(), (Slot::P(1), Kind::Scalar));
        base.insert("x1".to_string(), (Slot::P(2), Kind::Scalar));
        base.insert("y1".to_string(), (Slot::L(0), Kind::Scalar));
        let mut ps2 = Ps { t: &st, i: 0, scopes: vec![base, gs], n_glob: ps.n_glob, n_loc: 1, glob_mode: false, statics: vec![], depth: 0, sdepth: 0, ops: 0 };
        let c = ps2.block_items(false).unwrap();
        let mut gv = vec![V::N(0.0); ps2.n_glob];
        let mut lv = vec![V::N(0.0); ps2.n_loc];
        let p = vec![0.0, 1e-6, x];
        {
            let mut rt = Rt { g: &mut gv, l: &mut vec![], p: &p, budget: BUDGET };
            for s in &g {
                rt.run(s).unwrap();
            }
        }
        let mut rt = Rt { g: &mut gv, l: &mut lv, p: &p, budget: BUDGET };
        for s in &c {
            if let Flow::Ret = rt.run(s).unwrap() {
                break;
            }
        }
        match lv[0] {
            V::N(v) => v,
            _ => f64::NAN,
        }
    }
    #[test]
    fn basics() {
        assert_eq!(run1("static double acc = 0;", "acc = acc + x1*delt;\ny1 = 2*x1 + 100*acc;", 3.0), 6.0 + 100.0 * 3e-6);
        assert_eq!(run1("double a[4] = {1,2,3,4};", "int i; double s = 0; for (i = 0; i < 4; i++) s += a[i]; y1 = s;", 0.0), 10.0);
        assert_eq!(run1("#define K 5\n", "switch ((int)x1) { case 1: y1 = K; break; case 2: y1 = 7; break; default: y1 = -1; }", 1.0), 5.0);
        assert_eq!(run1("", "y1 = 5/2;", 0.0), 2.5);
        assert_eq!(run1("", "y1 = (int)(5/2);", 0.0), 2.0);
        assert_eq!(run1("", "y1 = (x1 > 1) ? fabs(-3.0) : 0;", 2.0), 3.0);
        assert_eq!(run1("", "int k = 0; while (1) { k++; if (k >= 10) break; } y1 = k;", 0.0), 10.0);
        assert_eq!(run1("", "y1 = 1 << 4 | 1;", 0.0), 17.0);
    }
    fn parse_err(code: &str) -> bool {
        let mut m = HashMap::new();
        let st = match lex(code, &mut m) {
            Ok(t) => t,
            Err(_) => return true,
        };
        let mut base = HashMap::new();
        base.insert("y1".to_string(), (Slot::L(0), Kind::Scalar));
        let mut ps = Ps { t: &st, i: 0, scopes: vec![base], n_glob: 0, n_loc: 1, glob_mode: false, statics: vec![], depth: 0, sdepth: 0, ops: 0 };
        ps.block_items(false).is_err()
    }
    /// crafted C code must give an error, never a stack overflow
    #[test]
    fn hostile_code_is_bounded() {
        assert!(parse_err(&format!("y1 = {}1{};", "(".repeat(100_000), ")".repeat(100_000))));
        assert!(parse_err(&format!("y1 = {}1;", "-".repeat(100_000))));
        assert!(parse_err(&format!("y1 = {};", vec!["1"; 200_000].join("+"))));
        assert!(parse_err(&format!("y1 = {};", vec!["1"; 200_000].join(","))));
        assert!(parse_err(&format!("{}y1 = 1;{}", "{".repeat(10_000), "}".repeat(10_000))));
        assert!(parse_err(&format!("{}y1 = 1;", "if (1) ".repeat(10_000))));
        // a long initializer table is fine: each item is its own expression
        let table = format!("double tab[3000] = {{{}}}; y1 = tab[2999];", vec!["2"; 3000].join(","));
        assert_eq!(run1("", &table, 0.0), 2.0);
        assert_eq!(run1("", &format!("y1 = {};", vec!["1"; 500].join("+")), 0.0), 500.0);
    }
}
