use crate::ast::{BinaryOperator, Expr, Literal, Statement, TypeName, UnaryOperator};
use crate::value::Value;

use rand::Rng;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::time::Duration;

#[derive(Debug)]
pub struct RuntimeError {
    pub message: String,
}

#[derive(Debug, Clone)]
struct Function {
    parameters: Vec<String>,
    body: Vec<Statement>,
}

#[derive(Debug)]
enum ControlFlow {
    Runtime(RuntimeError),
    Return(Value),
    Restart(String),
    End(String),
}

impl From<RuntimeError> for ControlFlow {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

pub struct Runtime {
    scopes: Vec<HashMap<String, Value>>,
    functions: HashMap<String, Function>,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
        }
    }

    pub fn execute(&mut self, statements: &[Statement]) -> Result<(), RuntimeError> {
        match self.execute_statements(statements) {
            Ok(()) => Ok(()),

            Err(ControlFlow::Runtime(error)) => Err(error),

            Err(ControlFlow::Return(_)) => Err(RuntimeError {
                message: "'give' can only be used inside a function".to_string(),
            }),

            Err(ControlFlow::Restart(name)) => Err(RuntimeError {
                message: format!("Cannot restart '{}' outside a loop", name),
            }),

            Err(ControlFlow::End(name)) => Err(RuntimeError {
                message: format!("Cannot end '{}' outside a loop", name),
            }),
        }
    }

    fn execute_statements(&mut self, statements: &[Statement]) -> Result<(), ControlFlow> {
        for statement in statements {
            self.execute_statement(statement)?;
        }

        Ok(())
    }

    fn value_to_json(&self, value: &Value) -> String {
        match value {
            Value::Text(text) => {
                format!("\"{}\"", self.escape_json(text))
            }

            Value::Number(number) => number.to_string(),

            Value::Boolean(boolean) => boolean.to_string(),

            Value::List(values) => {
                let items = values
                    .iter()
                    .map(|value| self.value_to_json(value))
                    .collect::<Vec<String>>()
                    .join(", ");

                format!("[{}]", items)
            }

            Value::Nothing => "null".to_string(),
        }
    }

    fn escape_json(&self, text: &str) -> String {
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    }

    fn execute_statement(&mut self, statement: &Statement) -> Result<(), ControlFlow> {
        match statement {
            Statement::Define { name, value } => {
                let value = self.evaluate(value)?;
                self.define_variable(name, value);
                Ok(())
            }

            Statement::Assign { name, value } => {
                let value = self.evaluate(value)?;

                if !self.assign_variable(name, value) {
                    return Err(RuntimeError {
                        message: format!("Variable '{}' has not been defined", name),
                    }
                    .into());
                }

                Ok(())
            }

            Statement::Save { variable, path } => {
                let value = self.get_variable(variable).ok_or_else(|| RuntimeError {
                    message: format!("Variable '{}' has not been defined", variable),
                })?;

                let path = self.evaluate(path)?;

                let path = match path {
                    Value::Text(path) => path,

                    _ => {
                        return Err(RuntimeError {
                            message: "Save path must be text".to_string(),
                        }
                        .into());
                    }
                };

                let json = self.value_to_json(&value);

                fs::write(&path, json).map_err(|error| RuntimeError {
                    message: format!("Could not save '{}' to '{}': {}", variable, path, error),
                })?;

                Ok(())
            }

            Statement::Write(expression) => {
                let value = self.evaluate(expression)?;
                println!("{}", value);
                Ok(())
            }

            Statement::Get {
                name,
                input_type,
                prompt,
            } => {
                let prompt = self.evaluate(prompt)?;

                print!("{}", prompt);

                io::stdout().flush().map_err(|error| RuntimeError {
                    message: format!("Could not flush output: {}", error),
                })?;

                let mut input = String::new();

                io::stdin()
                    .read_line(&mut input)
                    .map_err(|error| RuntimeError {
                        message: format!("Could not read input: {}", error),
                    })?;

                let input = input.trim_end_matches(&['\r', '\n'][..]);

                let value = self.convert_input(input, input_type)?;
                self.define_variable(name, value);

                Ok(())
            }

            Statement::When {
                condition,
                body,
                otherwise,
            } => {
                let condition_value = self.evaluate(condition)?;

                match condition_value {
                    Value::Boolean(true) => {
                        self.execute_statements(body)?;
                    }

                    Value::Boolean(false) => {
                        if let Some(otherwise_body) = otherwise {
                            self.execute_statements(otherwise_body)?;
                        }
                    }

                    _ => {
                        return Err(RuntimeError {
                            message: "When condition must be boolean".to_string(),
                        }
                        .into());
                    }
                }

                Ok(())
            }

            Statement::AddToList { value, list } => {
                let value = self.evaluate(value)?;

                if !self.add_to_list(list, value) {
                    if self.get_variable(list).is_none() {
                        return Err(RuntimeError {
                            message: format!("Variable '{}' has not been defined", list),
                        }
                        .into());
                    }

                    return Err(RuntimeError {
                        message: format!("Variable '{}' is not a list", list),
                    }
                    .into());
                }

                Ok(())
            }

            Statement::Function {
                name,
                parameters,
                body,
            } => {
                self.functions.insert(
                    name.clone(),
                    Function {
                        parameters: parameters.clone(),
                        body: body.clone(),
                    },
                );

                Ok(())
            }

            Statement::Give(expression) => {
                let value = self.evaluate(expression)?;
                Err(ControlFlow::Return(value))
            }

            Statement::Loop { name, body } => self.execute_named_loop(name, body),

            Statement::Restart(name) => Err(ControlFlow::Restart(name.clone())),

            Statement::End(name) => Err(ControlFlow::End(name.clone())),

            Statement::Expression(expression) => {
                self.evaluate(expression)?;
                Ok(())
            }
        }
    }

    fn execute_named_loop(&mut self, name: &str, body: &[Statement]) -> Result<(), ControlFlow> {
        loop {
            match self.execute_statements(body) {
                Ok(()) => {
                    // Named loops repeat until they receive
                    // their matching end command.
                    continue;
                }

                Err(ControlFlow::Restart(restart_name)) => {
                    if restart_name == name {
                        continue;
                    }

                    return Err(ControlFlow::Restart(restart_name));
                }

                Err(ControlFlow::End(end_name)) => {
                    if end_name == name {
                        return Ok(());
                    }

                    return Err(ControlFlow::End(end_name));
                }

                Err(other) => {
                    return Err(other);
                }
            }
        }
    }

    fn evaluate(&mut self, expression: &Expr) -> Result<Value, RuntimeError> {
        match expression {
            Expr::Literal(literal) => self.evaluate_literal(literal),

            Expr::Variable(name) => self.get_variable(name).ok_or_else(|| RuntimeError {
                message: format!("Variable '{}' has not been defined", name),
            }),

            Expr::List(expressions) => {
                let mut values = Vec::new();

                for expression in expressions {
                    values.push(self.evaluate(expression)?);
                }

                Ok(Value::List(values))
            }

            Expr::Index { collection, index } => {
                let collection = self.evaluate(collection)?;
                let index = self.evaluate(index)?;

                self.evaluate_index(collection, index)
            }

            Expr::Unary {
                operator,
                expression,
            } => {
                let value = self.evaluate(expression)?;
                self.evaluate_unary(operator, value)
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                if matches!(operator, BinaryOperator::And) {
                    let left = self.evaluate(left)?;

                    match left {
                        Value::Boolean(false) => {
                            return Ok(Value::Boolean(false));
                        }

                        Value::Boolean(true) => {
                            let right = self.evaluate(right)?;

                            return match right {
                                Value::Boolean(value) => Ok(Value::Boolean(value)),

                                _ => Err(RuntimeError {
                                    message: "Right side of 'and' must be boolean".to_string(),
                                }),
                            };
                        }

                        _ => {
                            return Err(RuntimeError {
                                message: "Left side of 'and' must be boolean".to_string(),
                            });
                        }
                    }
                }

                if matches!(operator, BinaryOperator::Or) {
                    let left = self.evaluate(left)?;

                    match left {
                        Value::Boolean(true) => {
                            return Ok(Value::Boolean(true));
                        }

                        Value::Boolean(false) => {
                            let right = self.evaluate(right)?;

                            return match right {
                                Value::Boolean(value) => Ok(Value::Boolean(value)),

                                _ => Err(RuntimeError {
                                    message: "Right side of 'or' must be boolean".to_string(),
                                }),
                            };
                        }

                        _ => {
                            return Err(RuntimeError {
                                message: "Left side of 'or' must be boolean".to_string(),
                            });
                        }
                    }
                }

                let left = self.evaluate(left)?;
                let right = self.evaluate(right)?;

                self.evaluate_binary(left, operator, right)
            }

            Expr::Call { name, arguments } => self.call_function(name, arguments),
        }
    }

    fn evaluate_literal(&self, literal: &Literal) -> Result<Value, RuntimeError> {
        match literal {
            Literal::String(value) => Ok(Value::Text(value.clone())),

            Literal::Number(value) => {
                let number = value.parse::<f64>().map_err(|_| RuntimeError {
                    message: format!("Invalid number '{}'", value),
                })?;

                Ok(Value::Number(number))
            }

            Literal::Boolean(value) => Ok(Value::Boolean(*value)),

            Literal::Nothing => Ok(Value::Nothing),
        }
    }

    fn evaluate_unary(
        &self,
        operator: &UnaryOperator,
        value: Value,
    ) -> Result<Value, RuntimeError> {
        match operator {
            UnaryOperator::Not => match value {
                Value::Boolean(value) => Ok(Value::Boolean(!value)),

                _ => Err(RuntimeError {
                    message: "Operator 'not' requires a boolean".to_string(),
                }),
            },

            UnaryOperator::Negate => match value {
                Value::Number(value) => Ok(Value::Number(-value)),

                _ => Err(RuntimeError {
                    message: "Unary '-' requires a number".to_string(),
                }),
            },

            UnaryOperator::Positive => match value {
                Value::Number(value) => Ok(Value::Number(value)),

                _ => Err(RuntimeError {
                    message: "Unary '+' requires a number".to_string(),
                }),
            },
        }
    }

    fn evaluate_binary(
        &self,
        left: Value,
        operator: &BinaryOperator,
        right: Value,
    ) -> Result<Value, RuntimeError> {
        match operator {
            BinaryOperator::Add => match (left, right) {
                (Value::Number(left), Value::Number(right)) => Ok(Value::Number(left + right)),

                (Value::Text(left), right) => Ok(Value::Text(format!("{}{}", left, right))),

                (left, Value::Text(right)) => Ok(Value::Text(format!("{}{}", left, right))),

                _ => Err(RuntimeError {
                    message: "Operator '+' requires numbers or text".to_string(),
                }),
            },

            BinaryOperator::Subtract => self.numeric_operation(left, right, |a, b| a - b, "-"),

            BinaryOperator::Multiply => self.numeric_operation(left, right, |a, b| a * b, "*"),

            BinaryOperator::Divide => {
                let denominator = self.require_number(&right)?;

                if denominator == 0.0 {
                    return Err(RuntimeError {
                        message: "Cannot divide by zero".to_string(),
                    });
                }

                let numerator = self.require_number(&left)?;
                Ok(Value::Number(numerator / denominator))
            }

            BinaryOperator::Modulo => {
                let denominator = self.require_number(&right)?;

                if denominator == 0.0 {
                    return Err(RuntimeError {
                        message: "Cannot modulo by zero".to_string(),
                    });
                }

                let numerator = self.require_number(&left)?;
                Ok(Value::Number(numerator % denominator))
            }

            BinaryOperator::Equal => Ok(Value::Boolean(left == right)),

            BinaryOperator::NotEqual => Ok(Value::Boolean(left != right)),

            BinaryOperator::Greater => self.comparison_operation(left, right, |a, b| a > b, ">"),

            BinaryOperator::Less => self.comparison_operation(left, right, |a, b| a < b, "<"),

            BinaryOperator::GreaterEqual => {
                self.comparison_operation(left, right, |a, b| a >= b, ">=")
            }

            BinaryOperator::LessEqual => {
                self.comparison_operation(left, right, |a, b| a <= b, "<=")
            }

            BinaryOperator::And | BinaryOperator::Or => {
                unreachable!("logical operators are handled earlier");
            }
        }
    }

    fn numeric_operation<F>(
        &self,
        left: Value,
        right: Value,
        operation: F,
        operator_name: &str,
    ) -> Result<Value, RuntimeError>
    where
        F: FnOnce(f64, f64) -> f64,
    {
        let left = self.require_number(&left)?;
        let right = self.require_number(&right)?;

        Ok(Value::Number(operation(left, right)))
    }

    fn comparison_operation<F>(
        &self,
        left: Value,
        right: Value,
        operation: F,
        operator_name: &str,
    ) -> Result<Value, RuntimeError>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        let left = self.require_number(&left)?;
        let right = self.require_number(&right)?;

        Ok(Value::Boolean(operation(left, right)))
    }

    fn require_number(&self, value: &Value) -> Result<f64, RuntimeError> {
        match value {
            Value::Number(value) => Ok(*value),

            _ => Err(RuntimeError {
                message: "This operation requires numbers".to_string(),
            }),
        }
    }

    fn evaluate_index(&self, collection: Value, index: Value) -> Result<Value, RuntimeError> {
        let index = match index {
            Value::Number(value) if value.is_finite() && value.fract() == 0.0 && value >= 1.0 => {
                value as usize
            }

            Value::Number(_) => {
                return Err(RuntimeError {
                    message: "List indexes must be positive whole numbers".to_string(),
                });
            }

            _ => {
                return Err(RuntimeError {
                    message: "List indexes must be numbers".to_string(),
                });
            }
        };

        match collection {
            Value::List(values) => values.get(index - 1).cloned().ok_or_else(|| RuntimeError {
                message: format!("List index {} is out of bounds", index),
            }),

            _ => Err(RuntimeError {
                message: "Only lists can be indexed".to_string(),
            }),
        }
    }

    fn call_function(&mut self, name: &str, arguments: &[Expr]) -> Result<Value, RuntimeError> {
        if name == "wait" {
            return self.call_wait(arguments);
        }

        if name == "random" {
            return self.call_random(arguments);
        }

        let function = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError {
                message: format!("Function '{}' has not been defined", name),
            })?;

        if arguments.len() != function.parameters.len() {
            return Err(RuntimeError {
                message: format!(
                    "Function '{}' expects {} argument(s), but received {}",
                    name,
                    function.parameters.len(),
                    arguments.len()
                ),
            });
        }

        let mut argument_values = Vec::new();

        for argument in arguments {
            argument_values.push(self.evaluate(argument)?);
        }

        let mut local_scope = HashMap::new();

        for (parameter, value) in function.parameters.iter().zip(argument_values) {
            local_scope.insert(parameter.clone(), value);
        }

        self.scopes.push(local_scope);

        let result = match self.execute_statements(&function.body) {
            Ok(()) => Ok(Value::Nothing),

            Err(ControlFlow::Return(value)) => Ok(value),

            Err(ControlFlow::Runtime(error)) => Err(error),

            Err(ControlFlow::Restart(name)) => Err(RuntimeError {
                message: format!("Cannot restart '{}' from inside a function", name),
            }),

            Err(ControlFlow::End(name)) => Err(RuntimeError {
                message: format!("Cannot end '{}' from inside a function", name),
            }),
        };

        self.scopes.pop();

        result
    }

    fn call_wait(&mut self, arguments: &[Expr]) -> Result<Value, RuntimeError> {
        if arguments.len() != 1 {
            return Err(RuntimeError {
                message: "wait() expects exactly one argument".to_string(),
            });
        }

        let seconds = self.evaluate(&arguments[0])?;

        let seconds = match seconds {
            Value::Number(value) if value >= 0.0 => value,

            Value::Number(_) => {
                return Err(RuntimeError {
                    message: "wait() cannot use a negative duration".to_string(),
                });
            }

            _ => {
                return Err(RuntimeError {
                    message: "wait() expects a number of seconds".to_string(),
                });
            }
        };

        std::thread::sleep(Duration::from_secs_f64(seconds));

        Ok(Value::Nothing)
    }

    fn call_random(&mut self, arguments: &[Expr]) -> Result<Value, RuntimeError> {
        if arguments.len() != 2 {
            return Err(RuntimeError {
                message: "random() expects exactly two arguments".to_string(),
            });
        }

        let minimum = self.evaluate(&arguments[0])?;
        let maximum = self.evaluate(&arguments[1])?;

        let minimum = self.random_bound(&minimum, "minimum")?;
        let maximum = self.random_bound(&maximum, "maximum")?;

        if minimum > maximum {
            return Err(RuntimeError {
                message: "random() minimum cannot be greater than maximum".to_string(),
            });
        }

        let mut generator = rand::thread_rng();
        let result = generator.gen_range(minimum..=maximum);

        Ok(Value::Number(result as f64))
    }

    fn random_bound(&self, value: &Value, name: &str) -> Result<i64, RuntimeError> {
        match value {
            Value::Number(value)
                if value.is_finite()
                    && value.fract() == 0.0
                    && *value >= i64::MIN as f64
                    && *value <= i64::MAX as f64 =>
            {
                Ok(*value as i64)
            }

            Value::Number(_) => Err(RuntimeError {
                message: format!("random() {} must be a whole number", name),
            }),

            _ => Err(RuntimeError {
                message: format!("random() {} must be a number", name),
            }),
        }
    }

    fn convert_input(&self, input: &str, input_type: &TypeName) -> Result<Value, RuntimeError> {
        match input_type {
            TypeName::Text => Ok(Value::Text(input.to_string())),

            TypeName::Number => {
                let number = input.parse::<f64>().map_err(|_| RuntimeError {
                    message: format!("'{}' is not a valid number", input),
                })?;

                Ok(Value::Number(number))
            }

            TypeName::Boolean => match input {
                "true" | "True" | "TRUE" => Ok(Value::Boolean(true)),

                "false" | "False" | "FALSE" => Ok(Value::Boolean(false)),

                _ => Err(RuntimeError {
                    message: format!("'{}' is not a valid boolean", input),
                }),
            },
        }
    }

    fn define_variable(&mut self, name: &str, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), value);
        }
    }

    fn assign_variable(&mut self, name: &str, value: Value) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return true;
            }
        }

        false
    }

    fn add_to_list(&mut self, name: &str, value: Value) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(existing) = scope.get_mut(name) {
                if let Value::List(items) = existing {
                    items.push(value);
                    return true;
                }

                return false;
            }
        }

        false
    }

    fn get_variable(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value.clone());
            }
        }

        None
    }
}
