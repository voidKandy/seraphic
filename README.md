<div align="center">
  <h1>seraphic</h1>
</div>
<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/seraphic">
    <img src="https://img.shields.io/crates/v/seraphic.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/seraphic">
    <img src="https://img.shields.io/crates/d/seraphic.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- docs -->
  <a href="https://docs.rs/seraphic">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="docs.rs docs" />
  </a>
</div>


A synchronous, lightweight crate for creating your own JSON RPC 2.0 protocol.

> **_WARNING_**:
This is very early in development and is subject to significant change.

## What is `seraphic`?
`seraphic` provides a straightforward way of defining your very own JSON RPC 2.0 based protocol messages using Rust macros.

## A quick refresher on JSON RPC
Json rpc messages are structured as follows: 
```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Request {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Response {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<Error>,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    pub data: Option<serde_json::Value>,
}
```
Manually creating these structs as JSON is easy enough, but organizing all the methods, requests and responses can quickly get hectic. `seraphic` offers a quick an easy way to define all of these!


## Getting started
#### `RpcNamespace` 
> A trait for defining how the methods of your RPC protocol are separated
```rust
#[derive(RpcNamespace, Clone, Copy, PartialEq, Eq)]
#[namespace(separator=":")]
enum MyNamespace {
    Foo,
    Bar,
    Baz
}
```
The variants of the namespace enum define the method namespaces of your protocol. They are simply the variants' names in lowercase; so the above code will define your methods to have the namespaces "foo", "bar" and "baz", with methods appearing after a ':'.

If the `separator` argument isn't passed it defaults to '_'.
#### `RpcRequest` & `RpcResponse` 
> traits for defining the requests/responses that are used by your protocol
```rust
#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
#[rpc_request(namespace = "MyNamespace:foo")]
struct SomeFooRequest {
    field1: String,
    field2: u32,
    field3: serde_json::Value,
}
```
Each method in your namespace maps to a *single* request you've defined. Method names are defined by the whatever the name of your request is before the word "Request". So, the above struct's corresponding method would be "foo:someFoo". The syntax for mapping a request to a namespace is: `<Namespace struct name>:<namespace variant>`
> **NOTE:**
> 
> Any struct you want to derive `RpcRequest` on MUST have a name ending with the word "Request" and all of it's fields MUST be types that implement `serde::Serialize` and `serde::Deserialize`

Each `RpcRequest` should have a corresponding `RpcResponse` struct. This can be done in two ways: 
+ Make sure another struct with *the same prefix* but with the word "Response" instead of "Request" is in scope
    ```rust 
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct SomeFooResponse {}
    ```
+ pass a `response` argument in the `rpc_request` proc macro attribute
    ```rust
    #[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
    #[rpc_request(namespace = "MyNamespace:foo", response="SomeResponse")]
    struct SomeFooRequest {
        ...
    }
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct SomeResponse {}
    
    // If some response isn't the response to some other `RpcRequest` already
    // This is fine because `RpcResponse` is a flag trait
    impl RpcResponse for SomeResponse {}
    ```
**Keep in mind**:  
+ Both `RpcRequest` and `RpcResponse` structs MUST implement `serde::Serialize`, `serde::Deserialize`, `Clone` and `Debug`
+ mutliple `RpcRequests` can have the same corresponding `RpcResponse`
+ If a `response` argument *is* passed in the `rpc_request` macros, the macro assumes the struct already implements `RpcResponse`, if not, the proc macros assumes the corresponding *Response* struct *does not* implement `RpcResponse` and will implement it for you.

#### `RequestWrapper` and `ResponseWrapper` 
> simply enums that include all of the `RpcRequest` and `RpcResponse` structs included in your protocol.

You *do not* need to manually define these enums. Instead you can use the `wrapper` macro, which has the following syntax:
```
wrapper!(<ResponseWrapper|RequestWrapper>, <Name of your enum>, [<Each variant type of your enum>])
```
```rust
wrapper!(ResponseWrapper, MyResponse, [SomeFooResponse]);
// expands to:
#[derive(ResponseWrapper)]
enum MyResponse {
  Some(SomeFooResponse)
}
wrapper!(RequestWrapper, MyRequest, [SomeFooRequest]);
// expands to:
#[derive(RequestWrapper)]
enum MyRequest {
  Some(SomeFooRequest)
}
```

These structs need only to implement `Debug`
#### `Message<Rq,Rs>` 
> The main type you will interact with for passing your messages.`Rq` is a `RequestWrapper` type and `Rs` is a `ResponseWrapper` type.

Referring to the [tests](https://github.com/voidKandy/seraphic/tree/dev/tests) might be helpful


