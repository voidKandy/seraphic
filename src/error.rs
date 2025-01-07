use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::msg::Message;

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
    pub data: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ErrorCode {
    // Defined by JSON RPC:
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
    ServerErrorStart = -32099,
    ServerErrorEnd = -32000,

    Disconnect = -29900,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum ErrorKind<'e> {
    Other { str: &'e str, code: ErrorCode },
    Disconnect,
    Uninitialized(&'e Message),
}

impl<'e> Into<Error> for ErrorKind<'e> {
    fn into(self) -> Error {
        let (code, message, data) = match self {
            Self::Other { str, code } => (code, str, None),
            Self::Disconnect => (ErrorCode::Disconnect, "disconnected channel", None),
            Self::Uninitialized(message) => {
                let payload = serde_json::to_value(&message)
                    .unwrap_or_else(|e| json!(format!("malformed payload: {e:#?}")));
                (
                    ErrorCode::ServerErrorStart,
                    "uninitialized channel",
                    payload,
                )
            }
        };
    }
}

impl<'e> ErrorKind<'e> {
    pub fn other(str: &str, code: ErrorCode) -> Self {
        Self::Other { str, code }
    }
}
