use crate::ast::{BinaryOperator, Expr, Literal, Statement, TypeName, UnaryOperator};
use crate::lexer::{Token, TokenKind};
use std::mem::discriminant;

#[derive(Debug)]
pub struct ParserError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Statement>, ParserError> {
        let mut statements = Vec::new();

        self.skip_separators();

        while !self.check(&TokenKind::Eof) {
            statements.push(self.parse_statement()?);
            self.skip_separators();
        }

        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParserError> {
        match self.peek_kind() {
            TokenKind::Define => self.parse_define(),
            TokenKind::Get => self.parse_get(),
            TokenKind::Write => self.parse_write(),
            TokenKind::When => self.parse_when(),
            TokenKind::Add => self.parse_add_to_list(),
            TokenKind::Function => self.parse_function(),
            TokenKind::Give => self.parse_give(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::Restart => self.parse_restart(),
            TokenKind::End => self.parse_end(),
            TokenKind::Save => self.parse_save(),
            TokenKind::Identifier(_) => self.parse_identifier_statement(),

            _ => {
                let token = self.peek().clone();

                Err(self.error_at(&token, "Expected a statement"))
            }
        }
    }

    fn parse_define(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::Define)?;

        let name = self.expect_identifier("Expected a variable name")?;

        self.expect_simple(TokenKind::Equals)?;

        let value = self.parse_expression()?;
        self.finish_statement()?;

        Ok(Statement::Define { name, value })
    }

    fn parse_get(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::Get)?;

        let name = self.expect_identifier("Expected a variable name after 'get'")?;

        let input_type = if self.match_simple(TokenKind::As) {
            self.parse_type_name()?
        } else {
            // Untyped input defaults to text.
            TypeName::Text
        };

        self.expect_simple(TokenKind::Equals)?;

        let prompt = self.parse_expression()?;
        self.finish_statement()?;

        Ok(Statement::Get {
            name,
            input_type,
            prompt,
        })
    }

    fn parse_write(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::Write)?;

        self.expect_simple(TokenKind::LeftParen)?;

        let expression = self.parse_expression()?;

        self.expect_simple(TokenKind::RightParen)?;
        self.finish_statement()?;

        Ok(Statement::Write(expression))
    }

    fn parse_when(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::When)?;

        let condition = self.parse_expression()?;
        let body = self.parse_control_body()?;

        // Allow a newline or semicolon between the first block
        // and the otherwise branch.
        self.skip_separators();

        let otherwise = if self.match_simple(TokenKind::Otherwise) {
            Some(self.parse_control_body()?)
        } else {
            None
        };

        Ok(Statement::When {
            condition,
            body,
            otherwise,
        })
    }

    fn parse_add_to_list(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::Add)?;

        let value = self.parse_expression()?;

        self.expect_simple(TokenKind::To)?;

        let list = self.expect_identifier("Expected a list name after 'to'")?;

        self.finish_statement()?;

        Ok(Statement::AddToList { value, list })
    }

    fn parse_function(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::Function)?;

        let name = self.expect_identifier("Expected a function name")?;

        self.expect_simple(TokenKind::LeftParen)?;

        let mut parameters = Vec::new();

        if !self.check(&TokenKind::RightParen) {
            loop {
                let parameter = self.expect_identifier("Expected a parameter name")?;

                parameters.push(parameter);

                if !self.match_simple(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect_simple(TokenKind::RightParen)?;

        let body = self.parse_brace_block()?;

        Ok(Statement::Function {
            name,
            parameters,
            body,
        })
    }

    fn parse_give(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::Give)?;

        let value = self.parse_expression()?;
        self.finish_statement()?;

        Ok(Statement::Give(value))
    }

    fn parse_loop(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::Loop)?;

        // A Foxash loop name is always an identifier/string name.
        let name = self.expect_identifier("Expected a named loop identifier after 'loop'")?;

        // Loops exclusively use square brackets.
        self.expect_simple(TokenKind::LeftBracket)?;

        self.skip_separators();

        let mut body = Vec::new();

        while !self.check(&TokenKind::RightBracket) && !self.check(&TokenKind::Eof) {
            body.push(self.parse_statement()?);
            self.skip_separators();
        }

        if self.check(&TokenKind::Eof) {
            let token = self.peek().clone();

            return Err(self.error_at(&token, format!("Loop '{}' is missing a closing ']'", name)));
        }

        self.expect_simple(TokenKind::RightBracket)?;

        Ok(Statement::Loop { name, body })
    }

    fn parse_restart(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::Restart)?;

        let name = self.expect_identifier("Expected a loop name after 'restart'")?;

        self.finish_statement()?;

        Ok(Statement::Restart(name))
    }

    fn parse_end(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::End)?;

        let name = self.expect_identifier("Expected a loop name after 'end'")?;

        self.finish_statement()?;

        Ok(Statement::End(name))
    }

    fn parse_save(&mut self) -> Result<Statement, ParserError> {
        self.expect_simple(TokenKind::Save)?;

        let variable = self.expect_identifier("Expected a variable name after 'save'")?;

        self.expect_simple(TokenKind::To)?;

        let path = self.parse_expression()?;
        self.finish_statement()?;

        Ok(Statement::Save { variable, path })
    }

    fn parse_identifier_statement(&mut self) -> Result<Statement, ParserError> {
        if self.peek_next_is(&TokenKind::Equals) {
            let name = self.expect_identifier("Expected a variable name")?;

            self.expect_simple(TokenKind::Equals)?;

            let value = self.parse_expression()?;
            self.finish_statement()?;

            return Ok(Statement::Assign { name, value });
        }

        let expression = self.parse_expression()?;
        self.finish_statement()?;

        Ok(Statement::Expression(expression))
    }

    fn parse_control_body(&mut self) -> Result<Vec<Statement>, ParserError> {
        if self.match_simple(TokenKind::Colon) {
            self.skip_separators();

            return Ok(vec![self.parse_statement()?]);
        }

        self.parse_brace_block()
    }

    fn parse_brace_block(&mut self) -> Result<Vec<Statement>, ParserError> {
        self.expect_simple(TokenKind::LeftBrace)?;

        self.skip_separators();

        let mut statements = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            statements.push(self.parse_statement()?);
            self.skip_separators();
        }

        if self.check(&TokenKind::Eof) {
            let token = self.peek().clone();

            return Err(self.error_at(&token, "Block is missing a closing '}'"));
        }

        self.expect_simple(TokenKind::RightBrace)?;

        Ok(statements)
    }

    fn parse_type_name(&mut self) -> Result<TypeName, ParserError> {
        let token = self.peek().clone();

        match token.kind {
            TokenKind::Text => {
                self.advance();
                Ok(TypeName::Text)
            }

            TokenKind::NumberType => {
                self.advance();
                Ok(TypeName::Number)
            }

            TokenKind::Boolean => {
                self.advance();
                Ok(TypeName::Boolean)
            }

            _ => Err(self.error_at(&token, "Expected 'text', 'number', or 'boolean'")),
        }
    }

    fn parse_expression(&mut self) -> Result<Expr, ParserError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ParserError> {
        let mut expression = self.parse_and()?;

        while self.match_simple(TokenKind::Or) {
            let right = self.parse_and()?;

            expression = Expr::Binary {
                left: Box::new(expression),
                operator: BinaryOperator::Or,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_and(&mut self) -> Result<Expr, ParserError> {
        let mut expression = self.parse_comparison()?;

        while self.match_simple(TokenKind::And) {
            let right = self.parse_comparison()?;

            expression = Expr::Binary {
                left: Box::new(expression),
                operator: BinaryOperator::And,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParserError> {
        let mut expression = self.parse_term()?;

        loop {
            let operator = match self.peek_kind() {
                TokenKind::EqualEqual => BinaryOperator::Equal,
                TokenKind::NotEqual => BinaryOperator::NotEqual,
                TokenKind::Greater => BinaryOperator::Greater,
                TokenKind::Less => BinaryOperator::Less,
                TokenKind::GreaterEqual => BinaryOperator::GreaterEqual,
                TokenKind::LessEqual => BinaryOperator::LessEqual,
                _ => break,
            };

            self.advance();

            let right = self.parse_term()?;

            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_term(&mut self) -> Result<Expr, ParserError> {
        let mut expression = self.parse_factor()?;

        loop {
            let operator = match self.peek_kind() {
                TokenKind::Plus => BinaryOperator::Add,
                TokenKind::Minus => BinaryOperator::Subtract,
                _ => break,
            };

            self.advance();

            let right = self.parse_factor()?;

            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_factor(&mut self) -> Result<Expr, ParserError> {
        let mut expression = self.parse_unary()?;

        loop {
            let operator = match self.peek_kind() {
                TokenKind::Star => BinaryOperator::Multiply,
                TokenKind::Slash => BinaryOperator::Divide,
                TokenKind::Percent => BinaryOperator::Modulo,
                _ => break,
            };

            self.advance();

            let right = self.parse_unary()?;

            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParserError> {
        let operator = match self.peek_kind() {
            TokenKind::Not => Some(UnaryOperator::Not),
            TokenKind::Minus => Some(UnaryOperator::Negate),
            TokenKind::Plus => Some(UnaryOperator::Positive),
            _ => None,
        };

        if let Some(operator) = operator {
            self.advance();

            let expression = self.parse_unary()?;

            return Ok(Expr::Unary {
                operator,
                expression: Box::new(expression),
            });
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParserError> {
        let mut expression = self.parse_primary()?;

        loop {
            if self.match_simple(TokenKind::LeftBracket) {
                let index = self.parse_expression()?;

                self.expect_simple(TokenKind::RightBracket)?;

                expression = Expr::Index {
                    collection: Box::new(expression),
                    index: Box::new(index),
                };

                continue;
            }

            break;
        }

        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParserError> {
        let token = self.peek().clone();

        match token.kind {
            TokenKind::String(value) => {
                self.advance();

                Ok(Expr::Literal(Literal::String(value)))
            }

            TokenKind::Number(value) => {
                self.advance();

                Ok(Expr::Literal(Literal::Number(value)))
            }

            TokenKind::True => {
                self.advance();

                Ok(Expr::Literal(Literal::Boolean(true)))
            }

            TokenKind::False => {
                self.advance();

                Ok(Expr::Literal(Literal::Boolean(false)))
            }

            TokenKind::Nothing => {
                self.advance();

                Ok(Expr::Literal(Literal::Nothing))
            }

            TokenKind::Identifier(name) => {
                self.advance();

                if self.match_simple(TokenKind::LeftParen) {
                    let arguments = self.parse_arguments()?;

                    Ok(Expr::Call { name, arguments })
                } else {
                    Ok(Expr::Variable(name))
                }
            }

            TokenKind::LeftParen => {
                self.advance();

                let expression = self.parse_expression()?;

                self.expect_simple(TokenKind::RightParen)?;

                Ok(expression)
            }

            TokenKind::LeftBracket => self.parse_list(),

            _ => Err(self.error_at(&token, "Expected an expression")),
        }
    }

    fn parse_list(&mut self) -> Result<Expr, ParserError> {
        self.expect_simple(TokenKind::LeftBracket)?;

        // Allow newlines after '['.
        self.skip_separators();

        let mut items = Vec::new();

        if !self.check(&TokenKind::RightBracket) {
            loop {
                items.push(self.parse_expression()?);

                // Allow newlines before a comma or ']'.
                self.skip_separators();

                if !self.match_simple(TokenKind::Comma) {
                    break;
                }

                // Allow newlines after a comma.
                self.skip_separators();

                // Allow a trailing comma.
                if self.check(&TokenKind::RightBracket) {
                    break;
                }
            }
        }

        self.skip_separators();
        self.expect_simple(TokenKind::RightBracket)?;

        Ok(Expr::List(items))
    }

    fn parse_arguments(&mut self) -> Result<Vec<Expr>, ParserError> {
        let mut arguments = Vec::new();

        if !self.check(&TokenKind::RightParen) {
            loop {
                arguments.push(self.parse_expression()?);

                if !self.match_simple(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect_simple(TokenKind::RightParen)?;

        Ok(arguments)
    }

    fn expect_identifier(&mut self, message: impl Into<String>) -> Result<String, ParserError> {
        let token = self.peek().clone();

        match token.kind {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }

            _ => Err(self.error_at(&token, message)),
        }
    }

    fn finish_statement(&mut self) -> Result<(), ParserError> {
        if self.match_simple(TokenKind::Semicolon) {
            return Ok(());
        }

        if self.check(&TokenKind::Newline)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::RightBracket)
            || self.check(&TokenKind::Eof)
        {
            return Ok(());
        }

        let token = self.peek().clone();

        Err(self.error_at(&token, "Expected the end of the statement"))
    }

    fn skip_separators(&mut self) {
        while self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
    }

    fn expect_simple(&mut self, expected: TokenKind) -> Result<(), ParserError> {
        if self.check(&expected) {
            self.advance();
            Ok(())
        } else {
            let token = self.peek().clone();

            Err(self.error_at(
                &token,
                format!("Expected {:?}, found {:?}", expected, token.kind),
            ))
        }
    }

    fn match_simple(&mut self, expected: TokenKind) -> bool {
        if self.check(&expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, expected: &TokenKind) -> bool {
        discriminant(self.peek_kind()) == discriminant(expected)
    }

    fn peek_next_is(&self, expected: &TokenKind) -> bool {
        if self.position + 1 >= self.tokens.len() {
            return false;
        }

        discriminant(&self.tokens[self.position + 1].kind) == discriminant(expected)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn advance(&mut self) {
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
    }

    fn error_at(&self, token: &Token, message: impl Into<String>) -> ParserError {
        ParserError {
            message: message.into(),
            line: token.line,
            column: token.column,
        }
    }
}
