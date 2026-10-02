use crate::ast::{
    BinaryOperator, ButtonEvent, ButtonStyle, Expr, Literal, Statement, TextStyle, TypeName,
    UnaryOperator,
};
use crate::graphics::{
    launch_window, ButtonView, ImageAsset, InputField, SceneItem, TextLine, WindowEvent,
    WindowHandle, WindowScene,
};
use crate::json;
use crate::value::Value;

use rand::Rng;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
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

#[derive(Clone)]
struct WindowConfig {
    width: i32,
    height: i32,
    background: u32,
    drawing: Vec<DrawingItem>,
    visible: bool,
    submit_button: Option<ButtonView>,
}

#[derive(Clone)]
struct BoxConfig {
    width: i32,
    height: i32,
}

#[derive(Clone)]
struct ButtonConfig {
    label: String,
    text_size: i32,
    text_color: u32,
    button_color: u32,
}

#[derive(Clone)]
enum DrawingItem {
    Text {
        expression: Expr,
        style: Option<TextStyle>,
    },
    Input {
        name: String,
        input_type: TypeName,
        prompt: String,
        width: i32,
        height: i32,
    },
    Image {
        path: String,
        width: Option<i32>,
        height: Option<i32>,
    },
}

#[derive(Clone)]
enum DeferredDrawing {
    Text {
        expression: Expr,
        style: Option<TextStyle>,
    },
    Image {
        path: String,
        width: Option<i32>,
        height: Option<i32>,
    },
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
    windows: HashMap<String, WindowConfig>,
    boxes: HashMap<String, BoxConfig>,
    buttons: HashMap<String, ButtonConfig>,
    window_handles: HashMap<String, WindowHandle>,
    pending_button_presses: VecDeque<String>,
    drawing_into: Option<String>,
    asset_directory: Option<PathBuf>,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            windows: HashMap::new(),
            boxes: HashMap::new(),
            buttons: HashMap::new(),
            window_handles: HashMap::new(),
            pending_button_presses: VecDeque::new(),
            drawing_into: None,
            asset_directory: None,
        }
    }

    pub fn with_asset_directory(asset_directory: Option<PathBuf>) -> Self {
        Self {
            asset_directory,
            ..Self::new()
        }
    }

    pub fn execute(&mut self, statements: &[Statement]) -> Result<(), RuntimeError> {
        let result = match self.execute_statements(statements) {
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
        };

        if result.is_ok() {
            for window in self.window_handles.values_mut() {
                window.wait_until_closed();
            }
            self.window_handles.clear();
        } else {
            self.close_graphics_windows();
        }

        result
    }

    fn execute_statements(&mut self, statements: &[Statement]) -> Result<(), ControlFlow> {
        for statement in statements {
            self.execute_statement(statement)?;
        }

        Ok(())
    }

    fn execute_statement(&mut self, statement: &Statement) -> Result<(), ControlFlow> {
        match statement {
            Statement::Define { name, value } => {
                let value = match value {
                    Expr::Window => {
                        self.windows.insert(
                            name.clone(),
                            WindowConfig {
                                width: 640,
                                height: 480,
                                background: rgb(255, 255, 255),
                                drawing: Vec::new(),
                                visible: false,
                                submit_button: None,
                            },
                        );
                        Value::Window
                    }
                    Expr::GraphicBox => {
                        self.boxes.insert(
                            name.clone(),
                            BoxConfig {
                                width: 280,
                                height: 28,
                            },
                        );
                        Value::GraphicBox
                    }
                    Expr::GraphicButton => {
                        self.buttons.insert(
                            name.clone(),
                            ButtonConfig {
                                label: name.clone(),
                                text_size: 14,
                                text_color: rgb(0, 0, 0),
                                button_color: rgb(230, 230, 230),
                            },
                        );
                        Value::Button
                    }
                    _ => self.evaluate(value)?,
                };
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

                let json = json::stringify(&value).map_err(|message| RuntimeError { message })?;

                fs::write(&path, json).map_err(|error| RuntimeError {
                    message: format!("Could not save '{}' to '{}': {}", variable, path, error),
                })?;

                Ok(())
            }

            Statement::Read { name, path } => {
                let path = self.evaluate(path)?;
                let path = match path {
                    Value::Text(path) => path,
                    _ => {
                        return Err(RuntimeError {
                            message: "Read path must be text".to_string(),
                        }
                        .into());
                    }
                };

                let source = fs::read_to_string(&path).map_err(|error| RuntimeError {
                    message: format!("Could not read JSON file '{}': {}", path, error),
                })?;
                let value = json::parse(&source).map_err(|error| RuntimeError {
                    message: format!("Could not read JSON file '{}': {}", path, error),
                })?;
                self.define_variable(name, value);
                Ok(())
            }

            Statement::SetSize {
                name,
                width,
                height,
            } => {
                let width = self.dimension(width, "width")?;
                let height = self.dimension(height, "height")?;
                match self.get_variable(name) {
                    Some(Value::Window) => {
                        let config = self.windows.get_mut(name).ok_or_else(|| RuntimeError {
                            message: format!("Window '{}' has not been defined", name),
                        })?;
                        config.width = width;
                        config.height = height;
                    }
                    Some(Value::GraphicBox) => {
                        let config = self.boxes.get_mut(name).ok_or_else(|| RuntimeError {
                            message: format!("Box '{}' has not been defined", name),
                        })?;
                        config.width = width;
                        config.height = height;
                    }
                    Some(_) => {
                        return Err(RuntimeError {
                            message: format!("'{}' is not a window or box", name),
                        }
                        .into());
                    }
                    None => {
                        return Err(RuntimeError {
                            message: format!("Variable '{}' has not been defined", name),
                        }
                        .into());
                    }
                }
                Ok(())
            }

            Statement::DrawInside { name, body } => {
                if self.get_variable(name) != Some(Value::Window) {
                    return Err(RuntimeError {
                        message: format!("'{}' is not a window", name),
                    }
                    .into());
                }

                if self.drawing_into.is_some() {
                    return Err(RuntimeError {
                        message: "A window cannot be drawn inside another window".to_string(),
                    }
                    .into());
                }

                self.windows
                    .get_mut(name)
                    .ok_or_else(|| RuntimeError {
                        message: format!("Window '{}' has not been defined", name),
                    })?
                    .drawing
                    .clear();
                self.drawing_into = Some(name.clone());
                let result = self.execute_statements(body);
                self.drawing_into = None;
                result?;
                Ok(())
            }

            Statement::SetBackground(color) => {
                let window = self.drawing_into.clone().ok_or_else(|| RuntimeError {
                    message: "'background' can only be set inside a 'draw inside' block"
                        .to_string(),
                })?;
                let color = parse_color(color)?;
                let config = self.windows.get_mut(&window).ok_or_else(|| RuntimeError {
                    message: format!("Window '{}' has not been defined", window),
                })?;
                config.background = color;
                Ok(())
            }

            Statement::ShowImage {
                path,
                width,
                height,
            } => {
                let window = self.drawing_into.clone().ok_or_else(|| RuntimeError {
                    message: "'show' can only be used inside a 'draw inside' block".to_string(),
                })?;
                let dimensions = match (width, height) {
                    (Some(width), Some(height)) => Some((
                        self.dimension(width, "image width")?,
                        self.dimension(height, "image height")?,
                    )),
                    _ => None,
                };
                self.windows
                    .get_mut(&window)
                    .ok_or_else(|| RuntimeError {
                        message: format!("Window '{}' has not been defined", window),
                    })?
                    .drawing
                    .push(DrawingItem::Image {
                        path: path.clone(),
                        width: dimensions.map(|value| value.0),
                        height: dimensions.map(|value| value.1),
                    });
                Ok(())
            }

            Statement::SetButtonText { name, text } => {
                if self.get_variable(name) != Some(Value::Button) {
                    return Err(RuntimeError {
                        message: format!("'{}' is not a button", name),
                    }
                    .into());
                }
                let label = self.evaluate(text)?.to_string();
                self.buttons
                    .get_mut(name)
                    .ok_or_else(|| RuntimeError {
                        message: format!("Button '{}' has not been defined", name),
                    })?
                    .label = label;
                self.update_button_windows(name)?;
                Ok(())
            }

            Statement::SetButtonStyle { name, style } => {
                if self.get_variable(name) != Some(Value::Button) {
                    return Err(RuntimeError {
                        message: format!("'{}' is not a button", name),
                    }
                    .into());
                }
                self.apply_button_style(name, style)?;
                self.update_button_windows(name)?;
                Ok(())
            }

            Statement::MakeWindow { name, appear } => {
                if self.get_variable(name) != Some(Value::Window) {
                    return Err(RuntimeError {
                        message: format!("'{}' is not a window", name),
                    }
                    .into());
                }

                if *appear {
                    self.show_window(name)?;
                } else {
                    if let Some(mut handle) = self.window_handles.remove(name) {
                        handle.close();
                    }
                    if let Some(config) = self.windows.get_mut(name) {
                        config.visible = false;
                    }
                }

                Ok(())
            }

            Statement::Write { expression, style } => {
                if let Some(window) = self.drawing_into.clone() {
                    let config = self.windows.get_mut(&window).ok_or_else(|| RuntimeError {
                        message: format!("Window '{}' has not been defined", window),
                    })?;
                    config.drawing.push(DrawingItem::Text {
                        expression: expression.clone(),
                        style: style.clone(),
                    });
                    return Ok(());
                }

                let value = self.evaluate(expression)?;
                println!("{}", value);
                Ok(())
            }

            Statement::Get {
                name,
                input_type,
                prompt,
                inside,
                button,
                button_style,
            } => {
                let prompt = self.evaluate(prompt)?;

                if let Some(window) = self.drawing_into.clone() {
                    let prompt = prompt.to_string();
                    let default_button_view = self.button_view(button.as_deref(), button_style)?;
                    let button_view = if button.is_none() && *button_style == ButtonStyle::default()
                    {
                        self.windows
                            .get(&window)
                            .and_then(|config| config.submit_button.clone())
                            .unwrap_or(default_button_view)
                    } else {
                        default_button_view
                    };
                    let (width, height) = if let Some(box_name) = inside {
                        if self.get_variable(box_name) != Some(Value::GraphicBox) {
                            return Err(RuntimeError {
                                message: format!("'{}' is not a box", box_name),
                            }
                            .into());
                        }
                        let config = self.boxes.get(box_name).ok_or_else(|| RuntimeError {
                            message: format!("Box '{}' has not been defined", box_name),
                        })?;
                        (config.width, config.height)
                    } else {
                        let config = self.windows.get(&window).ok_or_else(|| RuntimeError {
                            message: format!("Window '{}' has not been defined", window),
                        })?;
                        ((config.width - 40).max(80), 28)
                    };

                    let config = self.windows.get_mut(&window).ok_or_else(|| RuntimeError {
                        message: format!("Window '{}' has not been defined", window),
                    })?;
                    if let Some(existing) = &config.submit_button {
                        if existing.name.is_some()
                            && button_view.name.is_some()
                            && existing.name != button_view.name
                        {
                            return Err(RuntimeError {
                                message: "Inputs in one window must use the same submit button"
                                    .to_string(),
                            }
                            .into());
                        }
                    }
                    config.submit_button = Some(button_view);
                    config.drawing.push(DrawingItem::Input {
                        name: name.clone(),
                        input_type: input_type.clone(),
                        prompt,
                        width,
                        height,
                    });
                    return Ok(());
                }

                if inside.is_some() || button.is_some() || *button_style != ButtonStyle::default() {
                    return Err(RuntimeError {
                        message: "Input boxes and buttons can only be used inside a window"
                            .to_string(),
                    }
                    .into());
                }

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
                if let Expr::ButtonEvent { name, event } = condition {
                    self.wait_for_button_event(name, event)?;
                    self.execute_statements(body)?;
                    return Ok(());
                }

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

            Expr::Window => Ok(Value::Window),

            Expr::GraphicBox => Ok(Value::GraphicBox),

            Expr::GraphicButton => Ok(Value::Button),

            Expr::ButtonEvent { .. } => Err(RuntimeError {
                message: "Button events can only be used directly in a when condition".to_string(),
            }),

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
        _operator_name: &str,
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
        _operator_name: &str,
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
        match collection {
            Value::List(values) => {
                let index = match index {
                    Value::Number(value)
                        if value.is_finite() && value.fract() == 0.0 && value >= 1.0 =>
                    {
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

                values.get(index - 1).cloned().ok_or_else(|| RuntimeError {
                    message: format!("List index {} is out of bounds", index),
                })
            }

            Value::Object(values) => match index {
                Value::Text(key) => values.get(&key).cloned().ok_or_else(|| RuntimeError {
                    message: format!("JSON object has no key '{}'", key),
                }),
                _ => Err(RuntimeError {
                    message: "JSON object keys must be text".to_string(),
                }),
            },

            _ => Err(RuntimeError {
                message: "Only lists and JSON objects can be indexed".to_string(),
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

    fn dimension(&mut self, expression: &Expr, label: &str) -> Result<i32, RuntimeError> {
        let value = self.evaluate(expression)?;
        match value {
            Value::Number(value)
                if value.is_finite() && value.fract() == 0.0 && (1.0..=4096.0).contains(&value) =>
            {
                Ok(value as i32)
            }
            Value::Number(_) => Err(RuntimeError {
                message: format!(
                    "Window and box {} must be a whole number from 1 to 4096",
                    label
                ),
            }),
            _ => Err(RuntimeError {
                message: format!("Window and box {} must be a number", label),
            }),
        }
    }

    fn button_view(
        &mut self,
        name: Option<&str>,
        style: &ButtonStyle,
    ) -> Result<ButtonView, RuntimeError> {
        let (label, text_size, text_color, button_color) = if let Some(name) = name {
            if self.get_variable(name) != Some(Value::Button) {
                return Err(RuntimeError {
                    message: format!("'{}' is not a button", name),
                });
            }
            self.apply_button_style(name, style)?;
            let button = self.buttons.get(name).ok_or_else(|| RuntimeError {
                message: format!("Button '{}' has not been defined", name),
            })?;
            (
                button.label.clone(),
                button.text_size,
                button.text_color,
                button.button_color,
            )
        } else {
            ("Submit".to_string(), 14, rgb(0, 0, 0), rgb(230, 230, 230))
        };

        let text_size = if let Some(size) = &style.size {
            match self.evaluate(size)? {
                Value::Number(value)
                    if value.is_finite()
                        && value.fract() == 0.0
                        && (8.0..=96.0).contains(&value) =>
                {
                    value as i32
                }
                Value::Number(_) => {
                    return Err(RuntimeError {
                        message: "Button text size must be a whole number from 8 to 96".to_string(),
                    });
                }
                _ => {
                    return Err(RuntimeError {
                        message: "Button text size must be a number".to_string(),
                    });
                }
            }
        } else {
            text_size
        };
        Ok(ButtonView {
            name: name.map(str::to_string),
            label,
            text_size,
            text_color: style
                .text_color
                .as_deref()
                .map(parse_color)
                .transpose()?
                .unwrap_or(text_color),
            button_color: style
                .button_color
                .as_deref()
                .map(parse_color)
                .transpose()?
                .unwrap_or(button_color),
        })
    }

    fn apply_button_style(&mut self, name: &str, style: &ButtonStyle) -> Result<(), RuntimeError> {
        let text_size = if let Some(size) = &style.size {
            match self.evaluate(size)? {
                Value::Number(value)
                    if value.is_finite()
                        && value.fract() == 0.0
                        && (8.0..=96.0).contains(&value) =>
                {
                    Some(value as i32)
                }
                Value::Number(_) => {
                    return Err(RuntimeError {
                        message: "Button text size must be a whole number from 8 to 96".to_string(),
                    });
                }
                _ => {
                    return Err(RuntimeError {
                        message: "Button text size must be a number".to_string(),
                    });
                }
            }
        } else {
            None
        };
        let text_color = style.text_color.as_deref().map(parse_color).transpose()?;
        let button_color = style.button_color.as_deref().map(parse_color).transpose()?;
        let button = self.buttons.get_mut(name).ok_or_else(|| RuntimeError {
            message: format!("Button '{}' has not been defined", name),
        })?;
        if let Some(size) = text_size {
            button.text_size = size;
        }
        if let Some(color) = text_color {
            button.text_color = color;
        }
        if let Some(color) = button_color {
            button.button_color = color;
        }
        Ok(())
    }

    fn update_button_windows(&mut self, name: &str) -> Result<(), RuntimeError> {
        let button = self.buttons.get(name).ok_or_else(|| RuntimeError {
            message: format!("Button '{}' has not been defined", name),
        })?;
        let view = ButtonView {
            name: Some(name.to_string()),
            label: button.label.clone(),
            text_size: button.text_size,
            text_color: button.text_color,
            button_color: button.button_color,
        };
        let window_names = self
            .windows
            .iter()
            .filter_map(|(window_name, config)| {
                config
                    .submit_button
                    .as_ref()
                    .filter(|submit| submit.name.as_deref() == Some(name))
                    .map(|_| window_name.clone())
            })
            .collect::<Vec<_>>();
        for window_name in window_names {
            if let Some(config) = self.windows.get_mut(&window_name) {
                config.submit_button = Some(view.clone());
            }
            if let Some(handle) = self.window_handles.get(&window_name) {
                handle
                    .update_button(view.clone())
                    .map_err(|message| RuntimeError { message })?;
            }
        }
        Ok(())
    }

    fn wait_for_button_event(
        &mut self,
        name: &str,
        expected: &ButtonEvent,
    ) -> Result<(), RuntimeError> {
        if self.get_variable(name) != Some(Value::Button) {
            return Err(RuntimeError {
                message: format!("'{}' is not a button", name),
            });
        }

        if matches!(expected, ButtonEvent::Pressed) {
            if let Some(index) = self
                .pending_button_presses
                .iter()
                .position(|button| button == name)
            {
                self.pending_button_presses.remove(index);
                return Ok(());
            }
        }

        let window_name = self
            .windows
            .iter()
            .find_map(|(window_name, config)| {
                config
                    .submit_button
                    .as_ref()
                    .filter(|button| button.name.as_deref() == Some(name))
                    .map(|_| window_name.clone())
            })
            .ok_or_else(|| RuntimeError {
                message: format!("Button '{}' is not attached to an input in a window", name),
            })?;
        let handle = self
            .window_handles
            .get_mut(&window_name)
            .ok_or_else(|| RuntimeError {
                message: format!("The window for button '{}' is not open", name),
            })?;

        loop {
            match handle
                .wait_for_event()
                .map_err(|message| RuntimeError { message })?
            {
                WindowEvent::ButtonPressed(button)
                    if button == name && matches!(expected, ButtonEvent::Pressed) =>
                {
                    return Ok(())
                }
                WindowEvent::ButtonHovered(button)
                    if button == name && matches!(expected, ButtonEvent::Hovered) =>
                {
                    return Ok(())
                }
                WindowEvent::Closed => {
                    return Err(RuntimeError {
                        message: format!("The window for button '{}' was closed", name),
                    });
                }
                _ => {}
            }
        }
    }

    fn load_image(
        &self,
        path: &str,
        width: Option<i32>,
        height: Option<i32>,
    ) -> Result<ImageAsset, RuntimeError> {
        let requested_path = Path::new(path);
        let resolved_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else if let Some(directory) = &self.asset_directory {
            let beside_program = directory.join(requested_path);
            if beside_program.exists() {
                beside_program
            } else {
                requested_path.to_path_buf()
            }
        } else {
            requested_path.to_path_buf()
        };
        load_image(&resolved_path.to_string_lossy(), width, height)
    }

    fn show_window(&mut self, name: &str) -> Result<(), RuntimeError> {
        if self.window_handles.contains_key(name) {
            return Ok(());
        }

        let config = self
            .windows
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError {
                message: format!("Window '{}' has not been defined", name),
            })?;

        let first_input = config
            .drawing
            .iter()
            .position(|item| matches!(item, DrawingItem::Input { .. }));
        let mut initial_items = Vec::new();
        let mut deferred_items = Vec::new();
        let mut input_definitions = Vec::new();

        for (index, item) in config.drawing.iter().enumerate() {
            match item {
                DrawingItem::Text { expression, style } => {
                    if first_input.map_or(true, |input_index| index < input_index) {
                        initial_items.push(SceneItem::Text(self.make_text_line(
                            expression,
                            style.as_ref(),
                            config.background,
                        )?));
                    } else {
                        deferred_items.push(DeferredDrawing::Text {
                            expression: expression.clone(),
                            style: style.clone(),
                        });
                    }
                }
                DrawingItem::Image {
                    path,
                    width,
                    height,
                } => {
                    if first_input.map_or(true, |input_index| index < input_index) {
                        initial_items
                            .push(SceneItem::Image(self.load_image(path, *width, *height)?));
                    } else {
                        deferred_items.push(DeferredDrawing::Image {
                            path: path.clone(),
                            width: *width,
                            height: *height,
                        });
                    }
                }
                DrawingItem::Input {
                    name,
                    input_type,
                    prompt,
                    width,
                    height,
                } => {
                    input_definitions.push((name.clone(), input_type.clone()));
                    initial_items.push(SceneItem::Input(InputField {
                        prompt: prompt.clone(),
                        width: *width,
                        height: *height,
                    }));
                }
            }
        }

        let submit_button = config.submit_button.unwrap_or(ButtonView {
            name: None,
            label: "Submit".to_string(),
            text_size: 14,
            text_color: rgb(0, 0, 0),
            button_color: rgb(230, 230, 230),
        });

        let scene = WindowScene {
            title: name.to_string(),
            width: config.width,
            height: config.height,
            background: config.background,
            items: initial_items,
            submit_button,
        };

        let mut handle = launch_window(scene).map_err(|message| RuntimeError { message })?;

        if !input_definitions.is_empty() {
            loop {
                let (raw_values, button_name) = handle
                    .wait_for_input()
                    .map_err(|message| RuntimeError { message })?;
                if raw_values.len() != input_definitions.len() {
                    let _ = handle.reject_input(
                        "The window returned an unexpected number of answers".to_string(),
                    );
                    continue;
                }

                let mut converted_values = Vec::with_capacity(raw_values.len());
                let mut input_error = None;
                for ((_, input_type), raw_value) in input_definitions.iter().zip(&raw_values) {
                    let value = if matches!(input_type, TypeName::Text) {
                        raw_value.clone()
                    } else {
                        raw_value.trim().to_string()
                    };
                    match self.convert_input(&value, input_type) {
                        Ok(value) => converted_values.push(value),
                        Err(error) => {
                            input_error = Some(error.message);
                            break;
                        }
                    }
                }

                if let Some(error) = input_error {
                    let _ = handle.reject_input(error);
                    continue;
                }

                for ((variable, _), value) in input_definitions.iter().zip(converted_values) {
                    self.define_variable(variable, value);
                }

                if let Some(button) = button_name {
                    self.pending_button_presses.push_back(button);
                }

                let mut completed_items = Vec::with_capacity(deferred_items.len());
                for item in &deferred_items {
                    match item {
                        DeferredDrawing::Text { expression, style } => {
                            completed_items.push(SceneItem::Text(self.make_text_line(
                                expression,
                                style.as_ref(),
                                config.background,
                            )?));
                        }
                        DeferredDrawing::Image {
                            path,
                            width,
                            height,
                        } => {
                            completed_items
                                .push(SceneItem::Image(self.load_image(path, *width, *height)?));
                        }
                    }
                }

                handle
                    .complete_input(completed_items)
                    .map_err(|message| RuntimeError { message })?;
                break;
            }
        }

        if let Some(config) = self.windows.get_mut(name) {
            config.visible = true;
        }
        self.window_handles.insert(name.to_string(), handle);

        Ok(())
    }

    fn close_graphics_windows(&mut self) {
        for window in self.window_handles.values_mut() {
            window.close();
        }
        self.window_handles.clear();
    }

    fn make_text_line(
        &mut self,
        expression: &Expr,
        style: Option<&TextStyle>,
        background: u32,
    ) -> Result<TextLine, RuntimeError> {
        let text = self.evaluate(expression)?.to_string();
        let (size, color) = if let Some(style) = style {
            let size = if let Some(size) = &style.size {
                match self.evaluate(size)? {
                    Value::Number(value)
                        if value.is_finite()
                            && value.fract() == 0.0
                            && (8.0..=96.0).contains(&value) =>
                    {
                        value as i32
                    }
                    Value::Number(_) => {
                        return Err(RuntimeError {
                            message: "Text size must be a whole number from 8 to 96".to_string(),
                        });
                    }
                    _ => {
                        return Err(RuntimeError {
                            message: "Text size must be a number".to_string(),
                        });
                    }
                }
            } else {
                12
            };
            let color = if let Some(color) = &style.color {
                parse_color(color)?
            } else {
                opposite_color(background)
            };
            (size, color)
        } else {
            (12, opposite_color(background))
        };

        Ok(TextLine { text, size, color })
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

fn parse_color(value: &str) -> Result<u32, RuntimeError> {
    let named = match value.to_ascii_lowercase().as_str() {
        "black" => Some((0, 0, 0)),
        "white" => Some((255, 255, 255)),
        "red" => Some((255, 0, 0)),
        "green" => Some((0, 128, 0)),
        "blue" => Some((0, 0, 255)),
        "yellow" => Some((255, 255, 0)),
        "pink" => Some((255, 192, 203)),
        "purple" => Some((128, 0, 128)),
        "orange" => Some((255, 165, 0)),
        "gray" | "grey" => Some((128, 128, 128)),
        "cyan" => Some((0, 255, 255)),
        "magenta" => Some((255, 0, 255)),
        "brown" => Some((165, 42, 42)),
        "navy" => Some((0, 0, 128)),
        "teal" => Some((0, 128, 128)),
        _ => None,
    };

    if let Some((red, green, blue)) = named {
        return Ok(rgb(red, green, blue));
    }

    let hex = value.strip_prefix('#').unwrap_or(value);
    let expanded = if hex.len() == 3 {
        let mut result = String::with_capacity(6);
        for digit in hex.chars() {
            result.push(digit);
            result.push(digit);
        }
        result
    } else if hex.len() == 6 {
        hex.to_string()
    } else {
        return Err(RuntimeError {
            message: format!(
                "Unknown color '{}'; use a color name or 3- or 6-digit hex color",
                value
            ),
        });
    };

    let red = u32::from_str_radix(&expanded[0..2], 16).map_err(|_| RuntimeError {
        message: format!("Invalid hexadecimal color '{}'", value),
    })?;
    let green = u32::from_str_radix(&expanded[2..4], 16).map_err(|_| RuntimeError {
        message: format!("Invalid hexadecimal color '{}'", value),
    })?;
    let blue = u32::from_str_radix(&expanded[4..6], 16).map_err(|_| RuntimeError {
        message: format!("Invalid hexadecimal color '{}'", value),
    })?;

    Ok(rgb(red, green, blue))
}

fn load_image(
    path: &str,
    width: Option<i32>,
    height: Option<i32>,
) -> Result<ImageAsset, RuntimeError> {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "json" => "application/json",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        _ => {
            return Err(RuntimeError {
                message: format!(
                    "Foxash cannot display '{}'; supported files include PNG, JPEG, GIF, BMP, WebP, SVG, and JSON",
                    path
                ),
            });
        }
    };
    let contents = fs::read(path).map_err(|error| RuntimeError {
        message: format!("Could not show '{}': {}", path, error),
    })?;
    Ok(ImageAsset {
        name: path.to_string(),
        mime_type: mime_type.to_string(),
        contents,
        width,
        height,
    })
}

fn rgb(red: u32, green: u32, blue: u32) -> u32 {
    red | (green << 8) | (blue << 16)
}

fn opposite_color(color: u32) -> u32 {
    let red = color & 0xff;
    let green = (color >> 8) & 0xff;
    let blue = (color >> 16) & 0xff;
    if red * 299 + green * 587 + blue * 114 >= 128_000 {
        rgb(0, 0, 0)
    } else {
        rgb(255, 255, 255)
    }
}
