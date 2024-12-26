use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Error {
    /// A Number that indicates the error type that occurred.
    /// This MUST be an integer.
    pub code: String,
    /// A String providing a short description of the error.
    /// The message SHOULD be limited to a concise single sentence.
    pub message: String,
    /// A Primitive or Structured value that contains additional information about the error.
    /// This may be omitted.
    /// The value of this member is defined by the Server (e.g. detailed error information, nested errors etc.).
    pub data: serde_json::Value,
}

impl Error {
    /// Creates an error with an empty data field
    pub fn new_empty(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            data: json!({}),
        }
    }
}
