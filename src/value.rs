use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
    Boolean(bool),
    List(Vec<Value>),
    Nothing,
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Text(value) => write!(formatter, "{}", value),

            Value::Number(value) => {
                if value.fract() == 0.0 {
                    write!(formatter, "{}", *value as i64)
                } else {
                    write!(formatter, "{}", value)
                }
            }

            Value::Boolean(value) => write!(formatter, "{}", value),

            Value::List(values) => {
                write!(formatter, "[")?;

                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        write!(formatter, ", ")?;
                    }

                    write!(formatter, "{}", value)?;
                }

                write!(formatter, "]")
            }

            Value::Nothing => write!(formatter, "nothing"),
        }
    }
}
