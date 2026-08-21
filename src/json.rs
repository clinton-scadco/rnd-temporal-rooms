//! A JSON value, a parser and a writer, in `std`.
//!
//! The workbench needs both directions: the browser posts a graph document and
//! the server answers with a render snapshot. Nothing else in the crate has
//! ever needed a dependency and this does not either -- it is a few hundred
//! lines, and owning it means the wire format is a thing we can reason about
//! rather than a thing a derive macro decided.
//!
//! One decision worth naming. JavaScript numbers are `f64`, so an integer past
//! 2^53 does not survive the trip. Ticks reach 10^18 and item counters reach
//! whatever a billion machines produce, so `Json::big` emits anything above the
//! safe range as a *string* and the client formats both shapes. Silently losing
//! the low digits of a counter would be exactly the kind of unfaithful
//! reporting the rest of this crate exists to avoid.

use std::fmt::Write as _;

/// The largest integer a JavaScript `Number` holds exactly.
pub const SAFE_INT: u128 = (1u128 << 53) - 1;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    /// Integers stay integers. `f64` is not allowed to round a tick.
    Int(i128),
    Real(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    // ------------------------------------------------------------ building

    pub fn obj() -> Json {
        Json::Obj(Vec::new())
    }

    /// Append a field. Chained, because building a snapshot is mostly this.
    pub fn set(mut self, k: &str, v: impl Into<Json>) -> Json {
        if let Json::Obj(ref mut fields) = self {
            fields.push((k.to_string(), v.into()));
        }
        self
    }

    /// A count that may be past the range a JS number holds exactly.
    pub fn big(n: u128) -> Json {
        if n <= SAFE_INT {
            Json::Int(n as i128)
        } else {
            Json::Str(n.to_string())
        }
    }

    pub fn arr<T: Into<Json>>(items: impl IntoIterator<Item = T>) -> Json {
        Json::Arr(items.into_iter().map(Into::into).collect())
    }

    // ------------------------------------------------------------- reading

    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(fields) => fields.iter().find(|(n, _)| n == k).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Field lookup that treats a missing field and a null field alike, which
    /// is what every optional property in the document wants.
    pub fn at(&self, k: &str) -> &Json {
        match self.get(k) {
            Some(Json::Null) | None => &Json::Null,
            Some(v) => v,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i128(&self) -> Option<i128> {
        match self {
            Json::Int(n) => Some(*n),
            Json::Real(f) if f.fract() == 0.0 => Some(*f as i128),
            // A big count arrives back as the string it was sent as.
            Json::Str(s) => s.parse().ok(),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.as_i128().and_then(|n| u64::try_from(n).ok())
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Int(n) => Some(*n as f64),
            Json::Real(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> &[Json] {
        match self {
            Json::Arr(v) => v,
            _ => &[],
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    // ------------------------------------------------------------- writing

    pub fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Json::Real(f) => {
                if f.is_finite() {
                    let _ = write!(out, "{f}");
                } else {
                    out.push_str("null");
                }
            }
            Json::Str(s) => escape(s, out),
            Json::Arr(items) => {
                out.push('[');
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    it.write(out);
                }
                out.push(']');
            }
            Json::Obj(fields) => {
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    escape(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }

    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        s
    }
}

impl From<bool> for Json {
    fn from(b: bool) -> Json {
        Json::Bool(b)
    }
}
impl From<&str> for Json {
    fn from(s: &str) -> Json {
        Json::Str(s.to_string())
    }
}
impl From<String> for Json {
    fn from(s: String) -> Json {
        Json::Str(s)
    }
}
impl From<f64> for Json {
    fn from(f: f64) -> Json {
        Json::Real(f)
    }
}
impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(v: Option<T>) -> Json {
        match v {
            Some(v) => v.into(),
            None => Json::Null,
        }
    }
}
impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(v: Vec<T>) -> Json {
        Json::Arr(v.into_iter().map(Into::into).collect())
    }
}

macro_rules! from_int {
    ($($t:ty),*) => { $(
        impl From<$t> for Json {
            fn from(n: $t) -> Json { Json::Int(n as i128) }
        }
    )* };
}
from_int!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

/// JSON strings, with `<` and `>` escaped as well. The artifact embeds an
/// exported trace inside a `<script>` tag, and a `</script>` hiding in a node
/// name would end the tag.
fn escape(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// =============================================================== the parser

pub fn parse(src: &str) -> Result<Json, String> {
    let b = src.as_bytes();
    let mut p = P { b, i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != b.len() {
        return Err(format!("trailing input at byte {}", p.i));
    }
    Ok(v)
}

struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(format!("expected `{}` at byte {}", c as char, self.i))
        }
    }

    fn lit(&mut self, word: &str) -> bool {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            None => Err("unexpected end of input".into()),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') if self.lit("true") => Ok(Json::Bool(true)),
            Some(b'f') if self.lit("false") => Ok(Json::Bool(false)),
            Some(b'n') if self.lit("null") => Ok(Json::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected `{}` at byte {}", c as char, self.i)),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut fields = Vec::new();
        self.ws();
        if self.eat(b'}') {
            return Ok(Json::Obj(fields));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            self.expect(b':')?;
            self.ws();
            let v = self.value()?;
            fields.push((k, v));
            self.ws();
            if self.eat(b',') {
                continue;
            }
            self.expect(b'}')?;
            return Ok(Json::Obj(fields));
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Ok(Json::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            self.expect(b']')?;
            return Ok(Json::Arr(items));
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut s = String::new();
        loop {
            let c = self.peek().ok_or("unterminated string")?;
            self.i += 1;
            match c {
                b'"' => return Ok(s),
                b'\\' => {
                    let e = self.peek().ok_or("unterminated escape")?;
                    self.i += 1;
                    match e {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'/' => s.push('/'),
                        b'b' => s.push('\u{8}'),
                        b'f' => s.push('\u{c}'),
                        b'n' => s.push('\n'),
                        b'r' => s.push('\r'),
                        b't' => s.push('\t'),
                        b'u' => s.push(self.unicode()?),
                        c => return Err(format!("bad escape `\\{}`", c as char)),
                    }
                }
                // The bytes of a multi-byte character are copied through
                // unexamined; they cannot collide with `"` or `\`.
                c => {
                    let start = self.i - 1;
                    self.i = (start + utf8_len(c)).min(self.b.len());
                    match std::str::from_utf8(&self.b[start..self.i]) {
                        Ok(part) => s.push_str(part),
                        Err(_) => return Err(format!("invalid UTF-8 at byte {start}")),
                    }
                }
            }
        }
    }

    fn unicode(&mut self) -> Result<char, String> {
        let hex = |p: &Self, at: usize| -> Result<u32, String> {
            let s = p
                .b
                .get(at..at + 4)
                .and_then(|h| std::str::from_utf8(h).ok())
                .ok_or("truncated \\u escape")?;
            u32::from_str_radix(s, 16).map_err(|_| "bad \\u escape".to_string())
        };
        let hi = hex(self, self.i)?;
        self.i += 4;
        // A surrogate pair is two escapes that mean one character.
        if (0xD800..0xDC00).contains(&hi) && self.b.get(self.i) == Some(&b'\\') {
            let lo = hex(self, self.i + 2)?;
            if (0xDC00..0xE000).contains(&lo) {
                self.i += 6;
                let c = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                return char::from_u32(c).ok_or_else(|| "bad surrogate pair".to_string());
            }
        }
        char::from_u32(hi).ok_or_else(|| format!("bad code point U+{hi:04X}"))
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        self.eat(b'-');
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        let mut real = false;
        if self.peek() == Some(b'.') {
            real = true;
            self.i += 1;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            real = true;
            self.i += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.i += 1;
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
        if !real {
            if let Ok(n) = text.parse::<i128>() {
                return Ok(Json::Int(n));
            }
        }
        text.parse::<f64>().map(Json::Real).map_err(|_| format!("bad number `{text}`"))
    }
}

fn utf8_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}
