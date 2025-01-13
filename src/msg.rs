use crate::{
    Error as RpcError, RequestWrapper, ResponseWrapper, RpcRequest, RpcResponse, JSONRPC_FIELD,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

pub type MessageId = String;
#[derive(Debug, Clone, PartialEq)]
pub enum Message<Rq, Rs> {
    Req { id: MessageId, req: Rq },
    Res { id: MessageId, res: Rs },
    Err { id: MessageId, err: RpcError },
}

impl<'de, Rq, Rs> Deserialize<'de> for Message<Rq, Rs>
where
    Rq: RequestWrapper,
    Rs: ResponseWrapper,
{
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let json = <Value as Deserialize>::deserialize(d)?;

        // request deserialization MUST come first, Response can result in a false positive
        if let Ok(req) = serde_json::from_value::<Request>(json.clone()) {
            let id = req.id.clone();

            let req = Rq::try_from_req(req).map_err(|err| {
                serde::de::Error::custom(format!(
                    "Err converting from deserialized Request to wrapper: {err:#?}",
                ))
            })?;

            return Ok(Self::Req { id, req });
        }

        if let Ok(res) = serde_json::from_value::<Response>(json) {
            let id = res.id.clone();
            match Rs::try_from_res(res).map_err(|err| {
                serde::de::Error::custom(format!(
                    "Err converting from deserialized Response to wrapper: {err:#?}",
                ))
            })? {
                Ok(res) => return Ok(Self::Res { id, res }),
                Err(err) => return Ok(Self::Err { id, err }),
            }
        }

        Err(serde::de::Error::custom(
            "Failed to deserialize any Message variant",
        ))
    }
}

impl<Rq, Rs> Serialize for Message<Rq, Rs>
where
    Rq: RequestWrapper,
    Rs: ResponseWrapper,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Req { id, req } => {
                let req: Request = req.into_req(id);
                req.serialize(serializer)
            }
            Self::Res { id, res } => {
                let res: Response = res.into_res(id);
                res.serialize(serializer)
            }
            Self::Err { id, err } => {
                let err_res = Response::from_error(id, err.clone());
                err_res.serialize(serializer)
            }
        }
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
