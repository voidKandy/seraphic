use serde::{Deserialize, Serialize};

use crate::{MainErr, RpcRequest, RpcResponse, JSONRPC_FIELD};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum Message {
    Req(Request),
    Res(Response),
    Shutdown,
}

pub type MessageId = String;

impl Message {
    const SHUTDOWN: &'static str = "shutdown";
    pub fn id(&self) -> &str {
        match self {
            Self::Req(r) => &r.id,
            Self::Res(r) => &r.id,
            Self::Shutdown => Self::SHUTDOWN,
        }
    }

    pub fn to_send(&self) -> (MessageId, Option<serde_json::Value>) {
        let json = match self {
            Self::Req(r) => Some(serde_json::to_value(r).expect("failed to serialize request")),
            Self::Res(r) => Some(serde_json::to_value(r).expect("failed to serialize request")),
            _ => None,
        };
        (self.id().to_string(), json)
    }
}

impl TryInto<Request> for Message {
    type Error = MainErr;
    fn try_into(self) -> Result<Request, Self::Error> {
        if let Self::Req(r) = self {
            return Ok(r);
        }
        Err(std::io::Error::other("incorrect variant").into())
    }
}
impl TryInto<Response> for Message {
    type Error = MainErr;
    fn try_into(self) -> Result<Response, Self::Error> {
        if let Self::Res(r) = self {
            return Ok(r);
        }
        Err(std::io::Error::other("incorrect variant").into())
    }
}

impl From<Request> for Message {
    fn from(value: Request) -> Self {
        Self::Req(value)
    }
}
impl From<Response> for Message {
    fn from(value: Response) -> Self {
        Self::Res(value)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Request {
    /// A String specifying the version of the JSON-RPC protocol. MUST be exactly "2.0".
    pub jsonrpc: String,
    /// A String containing the name of the method to be invoked. Method names that begin with the word rpc followed by a period character (U+002E or ASCII 46) are reserved for rpc-internal methods and extensions and MUST NOT be used for anything else.
    pub method: String,
    /// A Structured value that holds the parameter values to be used during the invocation of the method. This member MAY be omitted.
    pub params: serde_json::Value,
    /// An identifier established by the Client that MUST contain a String, Number, or NULL value if included. If it is not included it is assumed to be a notification. The value SHOULD normally not be Null [1] and Numbers SHOULD NOT contain fractional parts [2]
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Response {
    /// A String specifying the version of the JSON-RPC protocol. MUST be exactly "2.0".
    pub jsonrpc: String,

    /// This member is REQUIRED on success.
    /// This member MUST NOT exist if there was an error invoking the method.
    /// The value of this member is determined by the method invoked on the Server.
    pub result: Option<serde_json::Value>,

    /// This member is REQUIRED on error.
    /// This member MUST NOT exist if there was no error triggered during invocation.
    pub error: Option<crate::error::Error>,

    /// This member is REQUIRED.
    /// It MUST be the same as the value of the id member in the Request Object.
    /// If there was an error in detecting the id in the Request object (e.g. Parse error/Invalid Request), it MUST be Null.
    pub id: String,
}

impl Request {
    pub fn from_req(id: impl ToString, req: impl RpcRequest) -> Self {
        req.into_request(id).unwrap()
    }
}

impl Response {
    pub fn new_ok(id: impl ToString, result: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_FIELD.to_string(),
            result,
            error: None,
            id: id.to_string(),
        }
    }

    pub fn from_error(id: impl ToString, error: crate::error::Error) -> Self {
        Self {
            jsonrpc: JSONRPC_FIELD.to_string(),
            result: None,
            error: Some(error),
            id: id.to_string(),
        }
    }

    pub fn from_res(id: impl ToString, res: impl RpcResponse) -> Self {
        res.into_response(id).unwrap()
    }
}
