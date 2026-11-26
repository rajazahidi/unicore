//! Minimal zero-dependency JSON encoder/decoder for uniload registries and CLI output.

use std::collections::BTreeMap;
use std::fmt;

/// Lightweight JSON DOM value covering all structures used by uniload.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            JsonValue::Object(map) => Some(map),
            _ => None,
        }
    }
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&to_json_pretty(self))
    }
}

/// Serialize a `JsonValue` into human-readable, 2-space indented JSON.
pub fn to_json_pretty(value: &JsonValue) -> String {
    let mut out = String::new();
    write_pretty(value, 0, &mut out);
    out
}

fn write_escaped_str(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_pretty(value: &JsonValue, indent: usize, out: &mut String) {
    match value {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonValue::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
                out.push_str(&format!("{}", *n as i64));
            } else {
                out.push_str(&n.to_string());
            }
        }
        JsonValue::Str(s) => write_escaped_str(s, out),
        JsonValue::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            let pad = "  ".repeat(indent + 1);
            for (idx, item) in items.iter().enumerate() {
                out.push_str(&pad);
                write_pretty(item, indent + 1, out);
                if idx + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&"  ".repeat(indent));
            out.push(']');
        }
        JsonValue::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            let pad = "  ".repeat(indent + 1);
            let total = map.len();
            for (idx, (k, v)) in map.iter().enumerate() {
                out.push_str(&pad);
                write_escaped_str(k, out);
                out.push_str(": ");
                write_pretty(v, indent + 1, out);
                if idx + 1 < total {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&"  ".repeat(indent));
            out.push('}');
        }
    }
}

/// Parse a UTF-8 JSON document into a `JsonValue`.
pub fn parse_json(input: &str) -> Result<JsonValue, String> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    let value = parser.parse_value()?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return Err(format!("Unexpected trailing data at byte {}", parser.pos));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if matches!(b, b' ' | b'\n' | b'\r' | b'\t') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.pos).copied()
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), String> {
        match self.peek() {
            Some(b) if b == expected => {
                self.pos += 1;
                Ok(())
            }
            Some(other) => Err(format!("Expected '{}', found '{}' at {}", expected as char, other as char, self.pos)),
            None => Err(format!("Unexpected EOF while expecting '{}'", expected as char)),
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", JsonValue::Null),
            Some(b't') => self.parse_literal(b"true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", JsonValue::Bool(false)),
            Some(b'"') => self.parse_string().map(JsonValue::Str),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(other) => Err(format!("Unexpected character '{}' at {}", other as char, self.pos)),
            None => Err("Unexpected end of JSON input".to_string()),
        }
    }

    fn parse_literal(&mut self, lit: &[u8], out: JsonValue) -> Result<JsonValue, String> {
        if self.bytes[self.pos..].starts_with(lit) {
            self.pos += lit.len();
            Ok(out)
        } else {
            Err(format!("Invalid literal at byte {}", self.pos))
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.pos += 1;
                let esc = *self.bytes.get(self.pos).ok_or("Unterminated escape")?;
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'b' => out.push('\u{08}'),
                    b'f' => out.push('\u{0C}'),
                    b'u' => {
                        let hex = self
                            .bytes
                            .get(self.pos..self.pos + 4)
                            .ok_or("Incomplete \\u escape")?;
                        let code = u32::from_str_radix(
                            std::str::from_utf8(hex).map_err(|_| "Invalid UTF-8 in \\u escape")?,
                            16,
                        )
                        .map_err(|_| "Invalid hex in \\u escape")?;
                        self.pos += 4;
                        let ch = char::from_u32(code).ok_or("Invalid Unicode scalar")?;
                        out.push(ch);
                    }
                    other => return Err(format!("Unknown escape '\\{}'", other as char)),
                }
            } else {
                // Decode valid UTF-8 character directly from slice.
                let tail = std::str::from_utf8(&self.bytes[self.pos..])
                    .map_err(|_| "Invalid UTF-8 inside JSON string")?;
                let ch = tail.chars().next().ok_or("Unexpected EOF")?;
                out.push(ch);
                self.pos += ch.len_utf8();
            }
        }
        Err("Unterminated JSON string".to_string())
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            if matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                self.pos += 1;
            } else {
                break;
            }
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|_| "Invalid number")?;
        let val: f64 = raw.parse().map_err(|_| format!("Invalid JSON number '{raw}'"))?;
        Ok(JsonValue::Number(val))
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect_byte(b'[')?;
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(items));
                }
                _ => return Err(format!("Expected ',' or ']' at byte {}", self.pos)),
            }
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect_byte(b'{')?;
        let mut map = BTreeMap::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(map));
        }
        loop {
            if self.peek() != Some(b'"') {
                return Err(format!("Expected string key at byte {}", self.pos));
            }
            let key = self.parse_string()?;
            self.expect_byte(b':')?;
            let val = self.parse_value()?;
            map.insert(key, val);
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(map));
                }
                _ => return Err(format!("Expected ',' or '}}' at byte {}", self.pos)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_windows_paths_and_nested_objects() {
        let mut inner = BTreeMap::new();
        inner.insert(
            "source_path".to_string(),
            JsonValue::Str(r#"C:\Users\WIN11\App "One"\setup.exe"#.to_string()),
        );
        inner.insert("allow".to_string(), JsonValue::Bool(true));
        inner.insert(
            "abis".to_string(),
            JsonValue::Array(vec![JsonValue::Str("arm64-v8a".to_string())]),
        );

        let root = JsonValue::Object(inner);
        let serialized = to_json_pretty(&root);
        let parsed = parse_json(&serialized).expect("roundtrip parse should succeed");
        assert_eq!(parsed, root);
    }
}
