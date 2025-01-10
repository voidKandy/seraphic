pub mod connection;

use crossbeam_channel::unbounded;
use seraphic::{
    client::Client,
    derive::{RpcNamespace, RpcRequest},
    error::{ErrorCode, ErrorKind},
    msg::{Request, Response},
    server::Server,
    RpcNamespace, RpcRequest, RpcResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::to_value;

use seraphic::msg::MessageId;

use seraphic::connection::{Connection, InitializeConnectionMessage};

#[derive(RpcNamespace, Clone, Copy, PartialEq, Eq)]
pub enum TestNS {
    Test,
}

pub type TestConnection = Connection<TestInitRequest>;

#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
#[rpc_request(namespace = "TestNS:test")]
pub struct TestInitRequest {}

impl InitializeConnectionMessage for TestInitRequest {
    const ID: &str = "test_init";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestInitResponse {}
