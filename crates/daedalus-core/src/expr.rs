//! Expression language used by every numeric parameter, parameter files,
//! math-function sources/blocks and nonlinear elements.
//!
//! Faithful port of the original tokenizer / parser:
//!   * numbers with SI suffixes (G M k K m u µ μ n p f, `meg`), trailing unit
//!     letters are ignored (`10mH`, `2.2uF`);
//!   * operators  ?:  ||  &&  < > <= >= == !=  + -  * /  unary - + !  ^ **;
//!   * functions sqrt sin cos tan asin acos atan atan2 exp log ln log10 abs sign
//!     floor ceil round int trunc min max pow sinh cosh tanh mod fmod sat step;
//!   * names are case-insensitive; `pi` is predefined.
//! Numeric semantics follow JavaScript (all values are f64, comparisons and
//! logical operators yield 1/0, NaN is false).

use std::collections::HashMap;
use std::fmt;

/// Symbol table: lower-case name -> value.
pub type Syms = HashMap<String, f64>;

#[derive(Debug, Clone, PartialEq)]
pub struct ExprError(pub String);
impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ExprError {}
fn err<T>(m: impl Into<String>) -> Result<T, ExprError> {
    Err(ExprError(m.into()))
}

#[derive(Clone, Debug)]
enum Tok {
    Num { v: f64, _sp: bool, _spa: bool },
    Id { v: String },
    Op { v: &'static str, sp: bool, spa: bool },
}

fn js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn suffix(c: char) -> Option<f64> {
    Some(match c {
        'G' => 1e9,
        'M' => 1e6,
        'k' | 'K' => 1e3,
        'm' => 1e-3,
        'u' | 'µ' | 'μ' => 1e-6,
        'n' => 1e-9,
        'p' => 1e-12,
        'f' => 1e-15,
        _ => return None,
    })
}

/// Match `^(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?` at s[i..]; returns (value, length).
fn match_number(s: &[char], i: usize) -> Option<(f64, usize)> {
    let n = s.len();
    let dig = |k: usize| k < n && s[k].is_ascii_digit();
    let mut j = i;
    if dig(j) {
        while dig(j) {
            j += 1;
        }
        if j < n && s[j] == '.' {
            j += 1;
            while dig(j) {
                j += 1;
            }
        }
    } else if j < n && s[j] == '.' && dig(j + 1) {
        j += 1;
        while dig(j) {
            j += 1;
        }
    } else {
        return None;
    }
    if j < n && (s[j] == 'e' || s[j] == 'E') {
        let mut k = j + 1;
        if k < n && (s[k] == '+' || s[k] == '-') {
            k += 1;
        }
        if dig(k) {
            while dig(k) {
                k += 1;
            }
            j = k;
        }
    }
    let txt: String = s[i..j].iter().collect();
    Some((parse_js_float(&txt), j - i))
}

/// parseFloat of a string already known to be a valid decimal literal.
pub(crate) fn parse_js_float(t: &str) -> f64 {
    let mut s = t.to_string();
    if s.starts_with('.') {
        s.insert(0, '0');
    }
    if let Some(p) = s.find(['e', 'E']) {
        if p > 0 && s.as_bytes()[p - 1] == b'.' {
            s.insert(p, '0');
        }
    } else if s.ends_with('.') {
        s.push('0');
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

fn tokenize(src: &str) -> Result<Vec<Tok>, ExprError> {
    let s: Vec<char> = src.chars().collect();
    let n = s.len();
    let mut toks = Vec::new();
    let mut i = 0;
    let sp_at = |k: usize| k < n && js_space(s[k]);
    while i < n {
        let c = s[i];
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
            i += 1;
            continue;
        }
        let sp = i > 0 && js_space(s[i - 1]);
        if let Some((mut v, len)) = match_number(&s, i) {
            i += len;
            let meg = i + 3 <= n && s[i..i + 3].iter().collect::<String>().to_lowercase() == "meg";
            if meg {
                v *= 1e6;
                i += 3;
            } else if i < n {
                if let Some(m) = suffix(s[i]) {
                    v *= m;
                    i += 1;
                }
            }
            while i < n && (s[i].is_ascii_alphabetic() || s[i] == 'Ω') {
                i += 1;
            }
            toks.push(Tok::Num { v, _sp: sp, _spa: sp_at(i) });
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let st = i;
            while i < n && (s[i].is_ascii_alphanumeric() || s[i] == '_') {
                i += 1;
            }
            toks.push(Tok::Id { v: s[st..i].iter().collect() });
            continue;
        }
        if i + 1 < n {
            let two: String = s[i..i + 2].iter().collect();
            let op2 = match two.as_str() {
                "<=" => Some("<="),
                ">=" => Some(">="),
                "==" => Some("=="),
                "!=" => Some("!="),
                "&&" => Some("&&"),
                "||" => Some("||"),
                "**" => Some("**"),
                _ => None,
            };
            if let Some(o) = op2 {
                toks.push(Tok::Op { v: o, sp, spa: sp_at(i + 2) });
                i += 2;
                continue;
            }
        }
        let op1 = match c {
            '+' => Some("+"),
            '-' => Some("-"),
            '*' => Some("*"),
            '/' => Some("/"),
            '^' => Some("^"),
            '(' => Some("("),
            ')' => Some(")"),
            ',' => Some(","),
            '<' => Some("<"),
            '>' => Some(">"),
            '!' => Some("!"),
            '?' => Some("?"),
            ':' => Some(":"),
            _ => None,
        };
        if let Some(o) = op1 {
            toks.push(Tok::Op { v: o, sp, spa: sp_at(i + 1) });
            i += 1;
            continue;
        }
        return err(format!("無法辨識的字元 \"{}\"", c));
    }
    Ok(toks)
}

/// Built-in functions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Func {
    Sqrt, Sin, Cos, Tan, Asin, Acos, Atan, Atan2, Exp, Log, Log10, Abs, Sign, Floor, Ceil,
    Round, Trunc, Min, Max, Pow, Sinh, Cosh, Tanh, Mod, Sat, Step,
}
fn func_of(name: &str) -> Option<Func> {
    use Func::*;
    Some(match name {
        "sqrt" => Sqrt,
        "sin" => Sin,
        "cos" => Cos,
        "tan" => Tan,
        "asin" => Asin,
        "acos" => Acos,
        "atan" => Atan,
        "atan2" => Atan2,
        "exp" => Exp,
        "log" | "ln" => Log,
        "log10" => Log10,
        "abs" => Abs,
        "sign" => Sign,
        "floor" => Floor,
        "ceil" => Ceil,
        "round" => Round,
        "int" | "trunc" => Trunc,
        "min" => Min,
        "max" => Max,
        "pow" => Pow,
        "sinh" => Sinh,
        "cosh" => Cosh,
        "tanh" => Tanh,
        "mod" | "fmod" => Mod,
        "sat" => Sat,
        "step" => Step,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CmpOp { Lt, Gt, Le, Ge, Eq, Ne }

/// Expression tree.
#[derive(Clone, Debug)]
pub enum Node {
    Num(f64),
    Arg(usize),
    Neg(Box<Node>),
    Not(Box<Node>),
    Add(Box<Node>, Box<Node>),
    Sub(Box<Node>, Box<Node>),
    Mul(Box<Node>, Box<Node>),
    Div(Box<Node>, Box<Node>),
    Pow(Box<Node>, Box<Node>),
    Cmp(CmpOp, Box<Node>, Box<Node>),
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
    Tern(Box<Node>, Box<Node>, Box<Node>),
    Call(Func, Vec<Node>),
}

/// Name resolution result.
enum Res {
    Val(f64),
    Arg(usize),
}

/// limits that keep a crafted expression from overflowing the stack (parse, evaluation and drop all recurse)
const MAX_DEPTH: usize = 64;
const MAX_OPERANDS: usize = 1000;

struct Parser<'a, R: Fn(&str) -> Option<Res>> {
    t: &'a [Tok],
    i: usize,
    res: R,
    list: bool,
    /// current nesting of unary / parenthesis levels
    depth: usize,
    /// operands parsed in the current expression (bounds the length of left-deep operator chains)
    ops: usize,
}

impl<'a, R: Fn(&str) -> Option<Res>> Parser<'a, R> {
    fn is_op(&self, v: &str) -> bool {
        matches!(self.t.get(self.i), Some(Tok::Op { v: o, .. }) if *o == v)
    }
    fn expect(&mut self, v: &str) -> Result<(), ExprError> {
        if !self.is_op(v) {
            return err(format!("缺少 \"{}\"", v));
        }
        self.i += 1;
        Ok(())
    }
    fn expr(&mut self) -> Result<Node, ExprError> {
        self.ternary()
    }
    fn ternary(&mut self) -> Result<Node, ExprError> {
        let c = self.or()?;
        if self.is_op("?") {
            self.i += 1;
            let a = self.ternary()?;
            self.expect(":")?;
            let b = self.ternary()?;
            return Ok(Node::Tern(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }
    fn or(&mut self) -> Result<Node, ExprError> {
        let mut a = self.and()?;
        while self.is_op("||") {
            self.i += 1;
            let b = self.and()?;
            a = Node::Or(Box::new(a), Box::new(b));
        }
        Ok(a)
    }
    fn and(&mut self) -> Result<Node, ExprError> {
        let mut a = self.cmp()?;
        while self.is_op("&&") {
            self.i += 1;
            let b = self.cmp()?;
            a = Node::And(Box::new(a), Box::new(b));
        }
        Ok(a)
    }
    fn cmp(&mut self) -> Result<Node, ExprError> {
        let mut a = self.add()?;
        loop {
            let op = match self.t.get(self.i) {
                Some(Tok::Op { v: "<", .. }) => CmpOp::Lt,
                Some(Tok::Op { v: ">", .. }) => CmpOp::Gt,
                Some(Tok::Op { v: "<=", .. }) => CmpOp::Le,
                Some(Tok::Op { v: ">=", .. }) => CmpOp::Ge,
                Some(Tok::Op { v: "==", .. }) => CmpOp::Eq,
                Some(Tok::Op { v: "!=", .. }) => CmpOp::Ne,
                _ => return Ok(a),
            };
            self.i += 1;
            let b = self.add()?;
            a = Node::Cmp(op, Box::new(a), Box::new(b));
        }
    }
    fn add(&mut self) -> Result<Node, ExprError> {
        let mut a = self.mul()?;
        loop {
            match self.t.get(self.i) {
                Some(Tok::Op { v, sp, spa }) if *v == "+" || *v == "-" => {
                    // "1 -2" in a list => two items
                    if self.list && *sp && !*spa {
                        return Ok(a);
                    }
                    let plus = *v == "+";
                    self.i += 1;
                    let b = self.mul()?;
                    a = if plus { Node::Add(Box::new(a), Box::new(b)) } else { Node::Sub(Box::new(a), Box::new(b)) };
                }
                _ => return Ok(a),
            }
        }
    }
    fn mul(&mut self) -> Result<Node, ExprError> {
        let mut a = self.unary()?;
        loop {
            match self.t.get(self.i) {
                Some(Tok::Op { v, .. }) if *v == "*" || *v == "/" => {
                    let m = *v == "*";
                    self.i += 1;
                    let b = self.unary()?;
                    a = if m { Node::Mul(Box::new(a), Box::new(b)) } else { Node::Div(Box::new(a), Box::new(b)) };
                }
                _ => return Ok(a),
            }
        }
    }
    fn unary(&mut self) -> Result<Node, ExprError> {
        self.ops += 1;
        if self.ops > MAX_OPERANDS {
            return err(format!("運算式過長（超過 {} 個運算元）", MAX_OPERANDS));
        }
        if self.depth >= MAX_DEPTH {
            return err(format!("運算式巢狀過深（超過 {} 層）", MAX_DEPTH));
        }
        self.depth += 1;
        let r = self.unary_inner();
        self.depth -= 1;
        r
    }
    fn unary_inner(&mut self) -> Result<Node, ExprError> {
        match self.t.get(self.i) {
            Some(Tok::Op { v: "-", .. }) => {
                self.i += 1;
                Ok(Node::Neg(Box::new(self.unary()?)))
            }
            Some(Tok::Op { v: "+", .. }) => {
                self.i += 1;
                self.unary()
            }
            Some(Tok::Op { v: "!", .. }) => {
                self.i += 1;
                Ok(Node::Not(Box::new(self.unary()?)))
            }
            _ => self.pow(),
        }
    }
    fn pow(&mut self) -> Result<Node, ExprError> {
        let a = self.primary()?;
        if self.is_op("^") || self.is_op("**") {
            self.i += 1;
            let b = self.unary()?;
            return Ok(Node::Pow(Box::new(a), Box::new(b)));
        }
        Ok(a)
    }
    fn primary(&mut self) -> Result<Node, ExprError> {
        let k = match self.t.get(self.i) {
            Some(k) => k.clone(),
            None => return err("運算式不完整"),
        };
        self.i += 1;
        match k {
            Tok::Num { v, .. } => Ok(Node::Num(v)),
            Tok::Op { v: "(", .. } => {
                let a = self.expr()?;
                self.expect(")")?;
                Ok(a)
            }
            Tok::Id { v } => {
                let name = v.to_lowercase();
                if self.is_op("(") {
                    self.i += 1;
                    let f = match func_of(&name) {
                        Some(f) => f,
                        None => return err(format!("不支援的函數 {}()", v)),
                    };
                    let mut args = Vec::new();
                    if !self.is_op(")") {
                        args.push(self.expr()?);
                        while self.is_op(",") {
                            self.i += 1;
                            args.push(self.expr()?);
                        }
                    }
                    self.expect(")")?;
                    return Ok(Node::Call(f, args));
                }
                if name == "pi" {
                    return Ok(Node::Num(std::f64::consts::PI));
                }
                match (self.res)(&name) {
                    Some(Res::Val(x)) => Ok(Node::Num(x)),
                    Some(Res::Arg(i)) => Ok(Node::Arg(i)),
                    None => err(format!("未定義的名稱 \"{}\"", v)),
                }
            }
            Tok::Op { v, .. } => err(format!("語法錯誤，靠近 \"{}\"", v)),
        }
    }
}

#[inline]
pub fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}
#[inline]
fn b2f(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}

/// JavaScript Math.pow semantics.
#[inline]
pub fn js_pow(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        return f64::NAN;
    }
    if y == 0.0 {
        return 1.0;
    }
    if x.abs() == 1.0 && y.is_infinite() {
        return f64::NAN;
    }
    x.powf(y)
}
/// JavaScript Math.round (round half up toward +inf).
#[inline]
pub fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let r = x.floor();
    if x - r >= 0.5 {
        r + 1.0
    } else {
        r
    }
}
pub fn js_max(v: &[f64]) -> f64 {
    let mut m = f64::NEG_INFINITY;
    for &x in v {
        if x.is_nan() {
            return f64::NAN;
        }
        if x > m {
            m = x;
        }
    }
    m
}
pub fn js_min(v: &[f64]) -> f64 {
    let mut m = f64::INFINITY;
    for &x in v {
        if x.is_nan() {
            return f64::NAN;
        }
        if x < m {
            m = x;
        }
    }
    m
}
#[inline]
pub fn js_sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

impl Node {
    pub fn eval(&self, a: &[f64]) -> f64 {
        match self {
            Node::Num(v) => *v,
            Node::Arg(i) => a.get(*i).copied().unwrap_or(f64::NAN),
            Node::Neg(x) => -x.eval(a),
            Node::Not(x) => b2f(!truthy(x.eval(a))),
            Node::Add(x, y) => x.eval(a) + y.eval(a),
            Node::Sub(x, y) => x.eval(a) - y.eval(a),
            Node::Mul(x, y) => x.eval(a) * y.eval(a),
            Node::Div(x, y) => x.eval(a) / y.eval(a),
            Node::Pow(x, y) => js_pow(x.eval(a), y.eval(a)),
            Node::Cmp(op, x, y) => {
                let (p, q) = (x.eval(a), y.eval(a));
                b2f(match op {
                    CmpOp::Lt => p < q,
                    CmpOp::Gt => p > q,
                    CmpOp::Le => p <= q,
                    CmpOp::Ge => p >= q,
                    CmpOp::Eq => p == q,
                    CmpOp::Ne => p != q,
                })
            }
            // JS: ((a)&&(b)?1:0) -- b is only evaluated when a is truthy (no side effects here)
            Node::And(x, y) => b2f(truthy(x.eval(a)) && truthy(y.eval(a))),
            Node::Or(x, y) => b2f(truthy(x.eval(a)) || truthy(y.eval(a))),
            Node::Tern(c, x, y) => {
                if truthy(c.eval(a)) {
                    x.eval(a)
                } else {
                    y.eval(a)
                }
            }
            Node::Call(f, args) => {
                let g = |i: usize| args.get(i).map(|n| n.eval(a)).unwrap_or(f64::NAN);
                match f {
                    Func::Sqrt => g(0).sqrt(),
                    Func::Sin => g(0).sin(),
                    Func::Cos => g(0).cos(),
                    Func::Tan => g(0).tan(),
                    Func::Asin => g(0).asin(),
                    Func::Acos => g(0).acos(),
                    Func::Atan => g(0).atan(),
                    Func::Atan2 => g(0).atan2(g(1)),
                    Func::Exp => g(0).exp(),
                    Func::Log => g(0).ln(),
                    Func::Log10 => g(0).log10(),
                    Func::Abs => g(0).abs(),
                    Func::Sign => js_sign(g(0)),
                    Func::Floor => g(0).floor(),
                    Func::Ceil => g(0).ceil(),
                    Func::Round => js_round(g(0)),
                    Func::Trunc => g(0).trunc(),
                    Func::Min => {
                        let v: Vec<f64> = args.iter().map(|n| n.eval(a)).collect();
                        js_min(&v)
                    }
                    Func::Max => {
                        let v: Vec<f64> = args.iter().map(|n| n.eval(a)).collect();
                        js_max(&v)
                    }
                    Func::Pow => js_pow(g(0), g(1)),
                    Func::Sinh => g(0).sinh(),
                    Func::Cosh => g(0).cosh(),
                    Func::Tanh => g(0).tanh(),
                    Func::Mod => g(0) % g(1),
                    Func::Sat => {
                        let (x, lo, hi) = (g(0), g(1), g(2));
                        if x < lo {
                            lo
                        } else if x > hi {
                            hi
                        } else {
                            x
                        }
                    }
                    Func::Step => b2f(g(0) >= 0.0),
                }
            }
        }
    }
}

fn compile(src: &str, res: impl Fn(&str) -> Option<Res>) -> Result<Node, ExprError> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return err("空白運算式");
    }
    let mut p = Parser { t: &toks, i: 0, res, list: false, depth: 0, ops: 0 };
    let n = p.expr()?;
    if p.i < toks.len() {
        return err(format!("多餘的內容，靠近 \"{}\"", tok_text(&toks[p.i])));
    }
    Ok(n)
}
fn tok_text(t: &Tok) -> String {
    match t {
        Tok::Num { v, .. } => crate::util::js_num_str(*v),
        Tok::Id { v } => v.clone(),
        Tok::Op { v, .. } => v.to_string(),
    }
}

/// `^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$`
pub fn plain_number(s: &str) -> Option<f64> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    if i < c.len() && (c[i] == '+' || c[i] == '-') {
        i += 1;
    }
    let (_, len) = match_number(&c, i)?;
    if i + len != c.len() {
        return None;
    }
    let neg = c[0] == '-';
    let body: String = c[i..].iter().collect();
    let v = parse_js_float(&body);
    Some(if neg { -v } else { v })
}

/// Evaluate a constant expression with a symbol table.
pub fn eval_const(src: &str, syms: &Syms) -> Result<f64, ExprError> {
    let s = src.trim();
    if s.is_empty() {
        return err("數值不可空白");
    }
    if let Some(v) = plain_number(s) {
        return Ok(v);
    }
    let n = compile(s, |nm| syms.get(nm).map(|v| Res::Val(*v)))?;
    let v = n.eval(&[]);
    if v.is_nan() {
        return err("結果不是有效數值");
    }
    Ok(v)
}

/// Evaluate a list "1 2 3", "1, -2", "0 1m 2m" (space before a sign without space after splits items).
pub fn eval_list(src: &str, syms: &Syms) -> Result<Vec<f64>, ExprError> {
    let s = src.trim();
    if s.is_empty() {
        return Ok(vec![]);
    }
    let toks = tokenize(s)?;
    let mut out = Vec::new();
    let res = |nm: &str| syms.get(nm).map(|v| Res::Val(*v));
    let mut p = Parser { t: &toks, i: 0, res, list: true, depth: 0, ops: 0 };
    while p.i < toks.len() {
        if p.is_op(",") {
            p.i += 1;
            continue;
        }
        p.ops = 0;
        let n = p.expr()?;
        out.push(n.eval(&[]));
    }
    Ok(out)
}

/// "(t1,v1) (t2,v2) ..." -> [(t,v),...]
pub fn eval_pairs(src: &str, syms: &Syms) -> Result<Vec<(f64, f64)>, ExprError> {
    let s: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] == '(' {
            // innermost group without nested parentheses: /\(([^()]*)\)/g
            let mut j = i + 1;
            while j < s.len() && s[j] != '(' && s[j] != ')' {
                j += 1;
            }
            if j < s.len() && s[j] == ')' {
                let inner: String = s[i + 1..j].iter().collect();
                let parts = eval_list(&inner, syms)?;
                if parts.len() != 2 {
                    return err("每一組需要 (時間, 數值)");
                }
                out.push((parts[0], parts[1]));
                i = j + 1;
                continue;
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    if out.is_empty() {
        return err("至少需要一組 (時間, 數值)");
    }
    Ok(out)
}

/// A compiled function of named arguments.
#[derive(Clone, Debug)]
pub struct CompiledFn {
    pub node: Node,
    pub nargs: usize,
}
impl CompiledFn {
    #[inline]
    pub fn call(&self, args: &[f64]) -> f64 {
        self.node.eval(args)
    }
}

/// Compile f(args) where args are variable names (case-insensitive); syms are constants.
pub fn compile_fn(src: &str, arg_names: &[&str], syms: &Syms) -> Result<CompiledFn, ExprError> {
    let mut map: HashMap<String, usize> = HashMap::new();
    for (i, a) in arg_names.iter().enumerate() {
        // JS: map[a] = '_a' + i -- a later duplicate name overrides the earlier one
        map.insert(a.to_lowercase(), i);
    }
    let node = compile(src, |nm| {
        if let Some(i) = map.get(nm) {
            return Some(Res::Arg(*i));
        }
        syms.get(nm).map(|v| Res::Val(*v))
    })?;
    Ok(CompiledFn { node, nargs: arg_names.len() })
}

/// Parameter file text: "name = expr" lines, % # // comments, `limit` lines skipped.
pub fn parse_param_text(text: &str) -> (Syms, Vec<String>) {
    let mut syms = Syms::new();
    let mut errors = Vec::new();
    for (li, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let mut l = line.to_string();
        if let Some(p) = l.find(['%', '#']) {
            l.truncate(p);
        }
        if let Some(p) = l.find("//") {
            l.truncate(p);
        }
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        let lc: Vec<char> = l.chars().collect();
        if lc.len() > 5 && l.to_lowercase().starts_with("limit") && js_space(lc[5]) {
            continue;
        }
        // ^([A-Za-z_][A-Za-z0-9_]*)\s*(=|\s)\s*(.+)$
        let mut j = 0;
        let ok_start = !lc.is_empty() && (lc[0].is_ascii_alphabetic() || lc[0] == '_');
        let mut parsed: Option<(String, String)> = None;
        if ok_start {
            while j < lc.len() && (lc[j].is_ascii_alphanumeric() || lc[j] == '_') {
                j += 1;
            }
            let name: String = lc[..j].iter().collect();
            let rest = &lc[j..];
            let mut w = 0;
            while w < rest.len() && js_space(rest[w]) {
                w += 1;
            }
            if w < rest.len() && rest[w] == '=' {
                let mut k = w + 1;
                while k < rest.len() && js_space(rest[k]) {
                    k += 1;
                }
                if k < rest.len() {
                    parsed = Some((name, rest[k..].iter().collect()));
                } else if w >= 1 {
                    parsed = Some((name, rest[w..].iter().collect()));
                }
            } else if w >= 1 && w < rest.len() {
                parsed = Some((name, rest[w..].iter().collect()));
            }
        }
        match parsed {
            None => errors.push(format!("第 {} 行格式錯誤：{}", li + 1, line)),
            Some((name, ex)) => match eval_const(&ex, &syms) {
                Ok(v) => {
                    syms.insert(name.to_lowercase(), v);
                }
                Err(e) => errors.push(format!("第 {} 行：{}", li + 1, e.0)),
            },
        }
    }
    (syms, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(s: &str) -> f64 {
        eval_const(s, &Syms::new()).unwrap()
    }
    #[test]
    fn numbers() {
        assert_eq!(c("10m"), 0.01);
        assert_eq!(c("2.2uF"), 2.2e-6);
        assert_eq!(c("1meg"), 1e6);
        assert_eq!(c("1M"), 1e6);
        assert_eq!(c("1k"), 1000.0);
        assert_eq!(c("1e3"), 1000.0);
        assert_eq!(c(".5"), 0.5);
        assert_eq!(c("1."), 1.0);
        assert_eq!(c("-2^2"), -4.0);
        assert_eq!(c("2^3^2"), 512.0);
        assert_eq!(c("1<2 && 3"), 1.0);
        assert_eq!(c("0 ? 5 : 7"), 7.0);
        assert_eq!(c("round(-2.5)"), -2.0);
        assert_eq!(c("min(3,1,2)"), 1.0);
        assert!((c("2*pi") - 6.283185307179586).abs() < 1e-15);
    }
    #[test]
    fn lists() {
        let s = Syms::new();
        assert_eq!(eval_list("1 -2", &s).unwrap(), vec![1.0, -2.0]);
        assert_eq!(eval_list("1 - 2", &s).unwrap(), vec![-1.0]);
        assert_eq!(eval_list("1-2", &s).unwrap(), vec![-1.0]);
        assert_eq!(eval_list("0 1m 2m", &s).unwrap(), vec![0.0, 1e-3, 2e-3]);
        assert_eq!(eval_pairs("(0,0) (1m,1)", &s).unwrap(), vec![(0.0, 0.0), (1e-3, 1.0)]);
    }
    #[test]
    fn params() {
        let (s, e) = parse_param_text("Vin = 100\nD 0.5 % duty\nVo=Vin*D");
        assert!(e.is_empty(), "{:?}", e);
        assert_eq!(s["vo"], 50.0);
    }
    #[test]
    fn fns() {
        let f = compile_fn("x1*x2+t", &["t", "x1", "x2"], &Syms::new()).unwrap();
        assert_eq!(f.call(&[1.0, 2.0, 3.0]), 7.0);
    }
    /// crafted input must give an error, never a stack overflow
    #[test]
    fn hostile_input_is_bounded() {
        let deep = format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000));
        assert!(eval_const(&deep, &Syms::new()).is_err());
        let neg = format!("{}1", "-".repeat(100_000));
        assert!(eval_const(&neg, &Syms::new()).is_err());
        let chain = vec!["1"; 200_000].join("+");
        assert!(eval_const(&chain, &Syms::new()).is_err());
        let list_item = format!("1 2 {}", vec!["1"; 200_000].join("+"));
        assert!(eval_list(&list_item, &Syms::new()).is_err());
        // ordinary sizes still work
        assert_eq!(c(&vec!["1"; 500].join("+")), 500.0);
        assert_eq!(c(&format!("{}2{}", "(".repeat(40), ")".repeat(40))), 2.0);
        assert_eq!(eval_list(&vec!["1"; 5000].join(" "), &Syms::new()).unwrap().len(), 5000);
    }
}
