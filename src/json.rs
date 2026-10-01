use crate::value::Value;
use std::collections::BTreeMap;

pub fn parse(source: &str) -> Result<Value, String> {
    let mut parser = JsonParser {
        characters: source.chars().collect(),
        position: 0,
    };

    parser.skip_whitespace();
    let value = parser.parse_value()?;
    parser.skip_whitespace();

    if parser.peek().is_some() {
        return Err(parser.error("Unexpected text after the JSON value"));
    }

    Ok(value)
}

pub fn stringify(value: &Value) -> Result<String, String> {
    match value {
        Value::Text(text) => Ok(quote(text)),
        Value::Number(number) if number.is_finite() => Ok(number.to_string()),
        Value::Number(_) => Err("JSON cannot store an infinite or invalid number".to_string()),
        Value::Boolean(value) => Ok(value.to_string()),
        Value::Nothing => Ok("null".to_string()),
        Value::List(values) => {
            let values = values
                .iter()
                .map(stringify)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", values.join(", ")))
        }
        Value::Object(values) => {
            let values = values
                .iter()
                .map(|(key, value)| Ok(format!("{}: {}", quote(key), stringify(value)?)))
                .collect::<Result<Vec<_>, String>>()?;
            Ok(format!("{{{}}}", values.join(", ")))
        }
        Value::Window | Value::GraphicBox => {
            Err("Windows and boxes cannot be saved as JSON".to_string())
        }
    }
}

pub(crate) fn quote(text: &str) -> String {
    let mut result = String::from("\"");

    for character in text.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\u{08}' => result.push_str("\\b"),
            '\u{0C}' => result.push_str("\\f"),
            character if character <= '\u{1F}' => {
                result.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => result.push(character),
        }
    }

    result.push('"');
    result
}

struct JsonParser {
    characters: Vec<char>,
    position: usize,
}

impl JsonParser {
    fn parse_value(&mut self) -> Result<Value, String> {
        self.skip_whitespace();

        match self.peek() {
            Some('n') => {
                self.expect_word("null")?;
                Ok(Value::Nothing)
            }
            Some('t') => {
                self.expect_word("true")?;
                Ok(Value::Boolean(true))
            }
            Some('f') => {
                self.expect_word("false")?;
                Ok(Value::Boolean(false))
            }
            Some('"') => self.parse_string().map(Value::Text),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some('-' | '0'..='9') => self.parse_number(),
            Some(_) => Err(self.error("Expected a JSON value")),
            None => Err(self.error("Expected a JSON value, found the end of the file")),
        }
    }

    fn parse_array(&mut self) -> Result<Value, String> {
        self.advance();
        self.skip_whitespace();
        let mut values = Vec::new();

        if self.consume(']') {
            return Ok(Value::List(values));
        }

        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();

            if self.consume(']') {
                return Ok(Value::List(values));
            }

            if !self.consume(',') {
                return Err(self.error("Expected ',' or ']' in the JSON array"));
            }

            self.skip_whitespace();
        }
    }

    fn parse_object(&mut self) -> Result<Value, String> {
        self.advance();
        self.skip_whitespace();
        let mut values = BTreeMap::new();

        if self.consume('}') {
            return Ok(Value::Object(values));
        }

        loop {
            if self.peek() != Some('"') {
                return Err(self.error("Expected a quoted key in the JSON object"));
            }

            let key = self.parse_string()?;
            self.skip_whitespace();

            if !self.consume(':') {
                return Err(self.error("Expected ':' after the JSON object key"));
            }

            let value = self.parse_value()?;
            values.insert(key, value);
            self.skip_whitespace();

            if self.consume('}') {
                return Ok(Value::Object(values));
            }

            if !self.consume(',') {
                return Err(self.error("Expected ',' or '}' in the JSON object"));
            }

            self.skip_whitespace();
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.advance();
        let mut result = String::new();

        loop {
            match self.advance() {
                Some('"') => return Ok(result),
                Some('\\') => {
                    let escaped = self
                        .advance()
                        .ok_or_else(|| self.error("Unfinished escape in JSON string"))?;

                    match escaped {
                        '"' => result.push('"'),
                        '\\' => result.push('\\'),
                        '/' => result.push('/'),
                        'b' => result.push('\u{08}'),
                        'f' => result.push('\u{0C}'),
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        'u' => {
                            let first = self.parse_hex_quad()?;
                            let scalar = if (0xD800..=0xDBFF).contains(&first) {
                                if !self.consume('\\') || !self.consume('u') {
                                    return Err(self.error(
                                        "High surrogate must be followed by a low surrogate",
                                    ));
                                }

                                let second = self.parse_hex_quad()?;
                                if !(0xDC00..=0xDFFF).contains(&second) {
                                    return Err(self.error("Invalid low surrogate in JSON string"));
                                }

                                0x10000
                                    + (((first as u32 - 0xD800) << 10) | (second as u32 - 0xDC00))
                            } else if (0xDC00..=0xDFFF).contains(&first) {
                                return Err(self.error("Unexpected low surrogate in JSON string"));
                            } else {
                                first as u32
                            };

                            result.push(char::from_u32(scalar).ok_or_else(|| {
                                self.error("Invalid Unicode escape in JSON string")
                            })?);
                        }
                        _ => return Err(self.error("Unknown escape in JSON string")),
                    }
                }
                Some(character) if character <= '\u{1F}' => {
                    return Err(
                        self.error("JSON strings cannot contain unescaped control characters")
                    );
                }
                Some(character) => result.push(character),
                None => return Err(self.error("Unfinished JSON string")),
            }
        }
    }

    fn parse_hex_quad(&mut self) -> Result<u16, String> {
        let mut value = 0u16;

        for _ in 0..4 {
            let digit = self
                .advance()
                .and_then(|character| character.to_digit(16))
                .ok_or_else(|| self.error("Invalid Unicode escape in JSON string"))?;
            value = (value << 4) | digit as u16;
        }

        Ok(value)
    }

    fn parse_number(&mut self) -> Result<Value, String> {
        let start = self.position;
        self.consume('-');

        match self.peek() {
            Some('0') => {
                self.advance();
                if matches!(self.peek(), Some('0'..='9')) {
                    return Err(self.error("JSON numbers cannot have leading zeroes"));
                }
            }
            Some('1'..='9') => {
                self.advance();
                while matches!(self.peek(), Some('0'..='9')) {
                    self.advance();
                }
            }
            _ => return Err(self.error("Expected a digit in JSON number")),
        }

        if self.consume('.') {
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(self.error("Expected a digit after the decimal point"));
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.advance();
            }
        }

        if matches!(self.peek(), Some('e' | 'E')) {
            self.advance();
            if matches!(self.peek(), Some('+' | '-')) {
                self.advance();
            }
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(self.error("Expected a digit in the JSON exponent"));
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.advance();
            }
        }

        let text = self.characters[start..self.position]
            .iter()
            .collect::<String>();
        let number = text
            .parse::<f64>()
            .map_err(|_| self.error("Invalid number in JSON file"))?;
        if !number.is_finite() {
            return Err(self.error("JSON number is outside Foxash's supported range"));
        }

        Ok(Value::Number(number))
    }

    fn expect_word(&mut self, word: &str) -> Result<(), String> {
        for expected in word.chars() {
            if self.advance() != Some(expected) {
                return Err(self.error(&format!("Expected '{}'", word)));
            }
        }
        Ok(())
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\n' | '\r' | '\t')) {
            self.advance();
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.characters.get(self.position).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.position += 1;
        Some(character)
    }

    fn error(&self, message: &str) -> String {
        format!("{} at character {}", message, self.position + 1)
    }
}
