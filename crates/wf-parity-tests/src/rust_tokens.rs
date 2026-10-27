//! Conservative Rust token navigation; comments/literals cannot create test items.

use crate::CheckError;

#[derive(Clone)]
pub(crate) struct Token {
    pub(crate) text: String,
    pub(crate) line: usize,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    index: usize,
    line: usize,
}

impl Cursor<'_> {
    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.index + offset).copied()
    }

    fn advance(&mut self) {
        if self.peek(0) == Some(b'\n') {
            self.line += 1;
        }
        self.index += 1;
    }

    fn block_comment(&mut self) -> Result<(), CheckError> {
        self.index += 2;
        let mut depth = 1;
        while depth != 0 {
            let Some(byte) = self.peek(0) else {
                return Err(CheckError::new("unterminated Rust block comment"));
            };
            if byte == b'/' && self.peek(1) == Some(b'*') {
                depth += 1;
                self.index += 2;
            } else if byte == b'*' && self.peek(1) == Some(b'/') {
                depth -= 1;
                self.index += 2;
            } else {
                self.advance();
            }
        }
        Ok(())
    }

    fn quoted(&mut self, quote: u8) -> Result<(), CheckError> {
        self.advance();
        while let Some(byte) = self.peek(0) {
            self.advance();
            if byte == quote {
                return Ok(());
            }
            if byte == b'\\' && self.peek(0).is_some() {
                self.advance();
            }
        }
        Err(CheckError::new("unterminated Rust literal"))
    }

    fn raw_string(&mut self) -> Result<bool, CheckError> {
        let prefix = if self.peek(0) == Some(b'r') {
            1
        } else if self.peek(0) == Some(b'b') && self.peek(1) == Some(b'r') {
            2
        } else {
            return Ok(false);
        };
        let mut hashes = 0;
        while self.peek(prefix + hashes) == Some(b'#') {
            hashes += 1;
        }
        if self.peek(prefix + hashes) != Some(b'"') {
            return Ok(false);
        }
        self.index += prefix + hashes + 1;
        while let Some(byte) = self.peek(0) {
            if byte == b'"' && (1..=hashes).all(|offset| self.peek(offset) == Some(b'#')) {
                self.index += hashes + 1;
                return Ok(true);
            }
            self.advance();
        }
        Err(CheckError::new("unterminated Rust raw string"))
    }

    fn character_literal(&self) -> bool {
        if self.peek(0) != Some(b'\'') {
            return false;
        }
        if self.peek(1) == Some(b'\\') {
            return true;
        }
        let mut offset = 1;
        while self.peek(offset).is_some_and(|byte| byte >= 128) {
            offset += 1;
        }
        if offset == 1 {
            offset += 1;
        }
        self.peek(offset) == Some(b'\'')
    }
}

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, CheckError> {
    let mut cursor = Cursor {
        bytes: source.as_bytes(),
        index: 0,
        line: 1,
    };
    let mut tokens = Vec::new();
    while let Some(byte) = cursor.peek(0) {
        if byte.is_ascii_whitespace() {
            cursor.advance();
            continue;
        }
        if byte == b'/' && cursor.peek(1) == Some(b'/') {
            while cursor.peek(0).is_some_and(|value| value != b'\n') {
                cursor.advance();
            }
            continue;
        }
        if byte == b'/' && cursor.peek(1) == Some(b'*') {
            cursor.block_comment()?;
            continue;
        }
        if cursor.raw_string()? {
            continue;
        }
        if byte == b'"' || cursor.character_literal() {
            cursor.quoted(byte)?;
            continue;
        }
        let start = cursor.index;
        let line = cursor.line;
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            while cursor
                .peek(0)
                .is_some_and(|value| value.is_ascii_alphanumeric() || value == b'_')
            {
                cursor.advance();
            }
        } else {
            cursor.advance();
            while cursor
                .peek(0)
                .is_some_and(|value| value >= 128 && value & 0xc0 == 0x80)
            {
                cursor.advance();
            }
        }
        let text = source
            .get(start..cursor.index)
            .ok_or_else(|| CheckError::new("unsupported Rust source token boundary"))?
            .to_owned();
        tokens.push(Token { text, line });
    }
    Ok(tokens)
}
