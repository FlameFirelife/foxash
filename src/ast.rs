#[derive(Debug, Clone, PartialEq)]
pub enum TypeName {
    Text,
    Number,
    Boolean,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Number(String),
    Boolean(bool),
    Nothing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub size: Option<Expr>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ButtonStyle {
    pub size: Option<Expr>,
    pub text_color: Option<String>,
    pub button_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ButtonEvent {
    Pressed,
    Hovered,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Not,
    Negate,
    Positive,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Variable(String),
    List(Vec<Expr>),
    Window,
    GraphicBox,
    GraphicButton,
    ButtonEvent {
        name: String,
        event: ButtonEvent,
    },

    Index {
        collection: Box<Expr>,
        index: Box<Expr>,
    },

    Binary {
        left: Box<Expr>,
        operator: BinaryOperator,
        right: Box<Expr>,
    },

    Unary {
        operator: UnaryOperator,
        expression: Box<Expr>,
    },

    Call {
        name: String,
        arguments: Vec<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Define {
        name: String,
        value: Expr,
    },

    Assign {
        name: String,
        value: Expr,
    },

    Write {
        expression: Expr,
        style: Option<TextStyle>,
    },

    Get {
        name: String,
        input_type: TypeName,
        prompt: Expr,
        inside: Option<String>,
        button: Option<String>,
        button_style: ButtonStyle,
    },

    When {
        condition: Expr,
        body: Vec<Statement>,
        otherwise: Option<Vec<Statement>>,
    },

    AddToList {
        value: Expr,
        list: String,
    },

    Function {
        name: String,
        parameters: Vec<String>,
        body: Vec<Statement>,
    },

    Give(Expr),

    Loop {
        name: String,
        body: Vec<Statement>,
    },

    Save {
        variable: String,
        path: Expr,
    },

    Read {
        name: String,
        path: Expr,
    },

    SetSize {
        name: String,
        width: Expr,
        height: Expr,
    },

    DrawInside {
        name: String,
        body: Vec<Statement>,
    },

    SetBackground(String),

    ShowImage {
        path: String,
        width: Option<Expr>,
        height: Option<Expr>,
    },

    SetButtonText {
        name: String,
        text: Expr,
    },

    SetButtonStyle {
        name: String,
        style: ButtonStyle,
    },

    MakeWindow {
        name: String,
        appear: bool,
    },

    Restart(String),
    End(String),

    Expression(Expr),
}
