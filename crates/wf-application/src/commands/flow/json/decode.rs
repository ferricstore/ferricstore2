//! Reading back the JSON this module writes for history fields: objects in
//! the order written, strings as binaries, integers and floats, booleans,
//! `null`, arrays. Used to restore a Flow's fields from a history event
//! (ADR-014 step 10).

use super::super::term::Term;

/// Deepest nesting read.
const MAX_DEPTH: usize = 32;

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn skip_space(&mut self) {
        while let Some((first, rest)) = self.0.split_first()
            && first.is_ascii_whitespace()
        {
            self.0 = rest;
        }
    }

    fn next(&mut self) -> Option<u8> {
        let (first, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(*first)
    }

    fn peek(&self) -> Option<u8> {
        self.0.first().copied()
    }

    fn literal(&mut self, word: &[u8], value: Term) -> Option<Term> {
        let rest = self.0.strip_prefix(word)?;
        self.0 = rest;
        Some(value)
    }

    fn hex4(&mut self) -> Option<u32> {
        let (digits, rest) = self.0.split_at_checked(4)?;
        self.0 = rest;
        u32::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()
    }

    /// A whole UTF-8 character starting with `first`.
    fn character(&mut self, first: u8) -> Option<String> {
        let width = match first {
            0x00..=0x7F => 1,
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            _ => 4,
        };
        let mut sequence = vec![first];
        for _ in 1..width {
            sequence.push(self.next()?);
        }
        String::from_utf8(sequence).ok()
    }

    fn string(&mut self) -> Option<Vec<u8>> {
        let mut out = String::new();
        loop {
            match self.next()? {
                b'"' => return Some(out.into_bytes()),
                b'\\' => {
                    let escaped = match self.next()? {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'u' => char::from_u32(self.hex4()?)?,
                        _ => return None,
                    };
                    out.push(escaped);
                }
                byte => out.push_str(&self.character(byte)?),
            }
        }
    }

    fn number(&mut self) -> Option<Term> {
        let length = self
            .0
            .iter()
            .position(|byte| !matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
            .unwrap_or(self.0.len());
        let (digits, rest) = self.0.split_at_checked(length)?;
        self.0 = rest;
        let text = std::str::from_utf8(digits).ok()?;
        text.parse::<i64>()
            .map(Term::Int)
            .ok()
            .or_else(|| text.parse::<f64>().ok().map(Term::Float))
    }

    fn value(&mut self, depth: usize) -> Option<Term> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.skip_space();
        match self.peek()? {
            b'"' => {
                self.next();
                self.string().map(Term::Bin)
            }
            b'{' => {
                self.next();
                self.object(depth)
            }
            b'[' => {
                self.next();
                self.array(depth)
            }
            b't' => self.literal(b"true", Term::Bool(true)),
            b'f' => self.literal(b"false", Term::Bool(false)),
            b'n' => self.literal(b"null", Term::Nil),
            _ => self.number(),
        }
    }

    fn object(&mut self, depth: usize) -> Option<Term> {
        let mut entries = Vec::new();
        self.skip_space();
        if self.peek()? == b'}' {
            self.next();
            return Some(Term::Map(entries));
        }
        loop {
            self.skip_space();
            if self.next()? != b'"' {
                return None;
            }
            let key = self.string()?;
            self.skip_space();
            if self.next()? != b':' {
                return None;
            }
            entries.push((key, self.value(depth.saturating_add(1))?));
            self.skip_space();
            match self.next()? {
                b',' => {}
                b'}' => return Some(Term::Map(entries)),
                _ => return None,
            }
        }
    }

    fn array(&mut self, depth: usize) -> Option<Term> {
        let mut items = Vec::new();
        self.skip_space();
        if self.peek()? == b']' {
            self.next();
            return Some(Term::List(items));
        }
        loop {
            items.push(self.value(depth.saturating_add(1))?);
            self.skip_space();
            match self.next()? {
                b',' => {}
                b']' => return Some(Term::List(items)),
                _ => return None,
            }
        }
    }
}

/// The term of JSON text; `None` when `text` is not exactly one value.
#[must_use]
pub fn decode(text: &[u8]) -> Option<Term> {
    let mut reader = Reader(text);
    let value = reader.value(0)?;
    reader.skip_space();
    reader.0.is_empty().then_some(value)
}
