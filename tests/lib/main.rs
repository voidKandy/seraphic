pub mod connection;
use seraphic::connection::{Connection, InitializeConnectionMessage};
use seraphic::{
    derive::{RpcNamespace, RpcRequest},
    RpcNamespace, RpcRequest, RpcResponse,
};
use serde::{Deserialize, Serialize};

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
