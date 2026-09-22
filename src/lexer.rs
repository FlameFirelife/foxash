#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Number(String),
    String(String),

    Define,
    Write,
    Get,
    As,
    When,
    Otherwise,
    Function,
    Give,
    Add,
    To,
    Save,

    Loop,
    Restart,
    End,

    And,
    Or,
    Not,

    True,
    False,
    Nothing,

    Text,
    NumberType,
    Boolean,

    EqualEqual,
    Equals,
    NotEqual,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    Greater,
    Less,
    GreaterEqual,
    LessEqual,

    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,

    Colon,
    Comma,
    Semicolon,

    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct LexerError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

pub struct Lexer {
    source: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexerError> {
        let mut tokens = Vec::new();

        while let Some(character) = self.peek() {
            let line = self.line;
            let column = self.column;

            match character {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }

                '\n' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Newline, line, column));
                }

                '/' if self.peek_next() == Some('/') => {
                    self.skip_comment();
                }

                '"' => {
                    tokens.push(self.read_string(line, column)?);
                }

                '0'..='9' => {
                    tokens.push(self.read_number(line, column));
                }

                'a'..='z' | 'A'..='Z' | '_' => {
                    tokens.push(self.read_identifier(line, column));
                }

                '=' if self.peek_next() == Some('=') => {
                    self.advance();
                    self.advance();
                    tokens.push(self.token(TokenKind::EqualEqual, line, column));
                }

                '=' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Equals, line, column));
                }

                '!' if self.peek_next() == Some('=') => {
                    self.advance();
                    self.advance();
                    tokens.push(self.token(TokenKind::NotEqual, line, column));
                }

                '!' => {
                    return Err(self.error("Unexpected character '!'", line, column));
                }

                '>' if self.peek_next() == Some('=') => {
                    self.advance();
                    self.advance();
                    tokens.push(self.token(TokenKind::GreaterEqual, line, column));
                }

                '>' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Greater, line, column));
                }

                '<' if self.peek_next() == Some('=') => {
                    self.advance();
                    self.advance();
                    tokens.push(self.token(TokenKind::LessEqual, line, column));
                }

                '<' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Less, line, column));
                }

                '+' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Plus, line, column));
                }

                '-' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Minus, line, column));
                }

                '*' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Star, line, column));
                }

                '/' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Slash, line, column));
                }

                '%' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Percent, line, column));
                }

                '(' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::LeftParen, line, column));
                }

                ')' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::RightParen, line, column));
                }

                '[' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::LeftBracket, line, column));
                }

                ']' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::RightBracket, line, column));
                }

                '{' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::LeftBrace, line, column));
                }

                '}' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::RightBrace, line, column));
                }

                ':' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Colon, line, column));
                }

                ',' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Comma, line, column));
                }

                ';' => {
                    self.advance();
                    tokens.push(self.token(TokenKind::Semicolon, line, column));
                }

                _ => {
                    return Err(self.error(
                        &format!("Unexpected character '{}'", character),
                        line,
                        column,
                    ));
                }
            }
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            line: self.line,
            column: self.column,
        });

        Ok(tokens)
    }

    fn read_string(&mut self, line: usize, column: usize) -> Result<Token, LexerError> {
        self.advance();

        let mut value = String::new();

        while let Some(character) = self.peek() {
            match character {
                '"' => {
                    self.advance();

                    return Ok(self.token(TokenKind::String(value), line, column));
                }

                '\\' => {
                    self.advance();

                    let escaped = match self.peek() {
                        Some('n') => {
                            self.advance();
                            '\n'
                        }

                        Some('t') => {
                            self.advance();
                            '\t'
                        }

                        Some('r') => {
                            self.advance();
                            '\r'
                        }

                        Some('"') => {
                            self.advance();
                            '"'
                        }

                        Some('\\') => {
                            self.advance();
                            '\\'
                        }

                        Some('0') => {
                            self.advance();
                            '\0'
                        }

                        Some(other) => {
                            return Err(self.error(
                                &format!("Unknown escape sequence '\\{}'", other),
                                self.line,
                                self.column,
                            ));
                        }

                        None => {
                            return Err(self.error(
                                "Unfinished string escape",
                                self.line,
                                self.column,
                            ));
                        }
                    };

                    value.push(escaped);
                }

                '\n' => {
                    return Err(self.error(
                        "Strings cannot contain an unescaped newline",
                        self.line,
                        self.column,
                    ));
                }

                _ => {
                    value.push(character);
                    self.advance();
                }
            }
        }

        Err(self.error("Unfinished string", line, column))
    }

    fn read_number(&mut self, line: usize, column: usize) -> Token {
        let mut value = String::new();
        let mut decimal_point_found = false;

        while let Some(character) = self.peek() {
            match character {
                '0'..='9' => {
                    value.push(character);
                    self.advance();
                }

                '.' if !decimal_point_found => {
                    decimal_point_found = true;
                    value.push(character);
                    self.advance();
                }

                _ => break,
            }
        }

        self.token(TokenKind::Number(value), line, column)
    }

    fn read_identifier(&mut self, line: usize, column: usize) -> Token {
        let mut value = String::new();

        while let Some(character) = self.peek() {
            if character.is_ascii_alphanumeric() || character == '_' {
                value.push(character);
                self.advance();
            } else {
                break;
            }
        }

        let kind = match value.as_str() {
            "define" => TokenKind::Define,
            "write" => TokenKind::Write,
            "get" => TokenKind::Get,
            "as" => TokenKind::As,
            "when" => TokenKind::When,
            "otherwise" => TokenKind::Otherwise,
            "function" => TokenKind::Function,
            "give" => TokenKind::Give,
            "add" => TokenKind::Add,
            "to" => TokenKind::To,
            "save" => TokenKind::Save,

            "loop" => TokenKind::Loop,
            "restart" => TokenKind::Restart,
            "end" => TokenKind::End,

            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,

            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "nothing" => TokenKind::Nothing,

            "text" => TokenKind::Text,
            "number" => TokenKind::NumberType,
            "boolean" => TokenKind::Boolean,

            _ => TokenKind::Identifier(value),
        };

        self.token(kind, line, column)
    }

    fn skip_comment(&mut self) {
        self.advance();
        self.advance();

        while let Some(character) = self.peek() {
            if character == '\n' {
                break;
            }

            self.advance();
        }
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.position).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.source.get(self.position + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let character = self.peek()?;

        self.position += 1;

        if character == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }

        Some(character)
    }

    fn token(&self, kind: TokenKind, line: usize, column: usize) -> Token {
        Token { kind, line, column }
    }

    fn error(&self, message: &str, line: usize, column: usize) -> LexerError {
        LexerError {
            message: message.to_string(),
            line,
            column,
        }
    }
}
