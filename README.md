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


A super light JSON RPC 2.0 implementation.

> **_WARNING_**:
This is very early in development and is subject to significant change.

## What is `seraphic`?
`seraphic` provides a straightforward way of defining your very own JSON RPC 2.0 based protocol, including an easy way to spin up clients and servers.

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
> simply enums that include all of the `RpcRequest` and `RpcResponse` structs included inyour protocol
```rust
#[derive(RequestWrapper, Debug)]
enum ReqWrapper {
    Foo(SomeFooRequest),
}
#[derive(ResponseWrapper, Debug)]
enum ResWrapper {
    Foo(SomeFooResponse),
}
```
These structs need only to implement `Debug`
#### `MsgWrapper<Rq,Rs>` 
> This is simply defined as a wrapper around both of your `RequestWrapper`/`ResponseWrapper`.
```rust
type MyWrapper = MsgWrapper<ReqWrapper, ResWrapper>;
```
#### `Connection<I>`
> The backbone of `Server` and `Client`

`I` is a type that implements `RpcRequest`, it defines the *request* and *response* that are exchanged by `Client` and `Server` when they first connect
```rust
#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
#[rpc_request(namespace = "MyNamespace:foo")]
struct InitRequest {}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitResponse {}
impl InitializeConnectionMessage for InitRequest {
    const ID: &str = "initialize";
}

type MyConnection = Connection<InitRequest>;
```
> **NOTE:**
> 
> While the `rpc_request` proc macro requires that you pass a `namespace` argument, the initial request and response structs *do not* need to be included in your wrappers if they are *only* used for connection initialization. The `RpcRequest` restraint on the initial request will most likely change to it's own trait down the line.

## Client/Server
The `Server<I>` and `Client<I>` structs implement `From<Connection<I>>`, so creating them is as easy as:
```rust
let serv_conn = Connection::<InitRequest>::listen("127.0.0.1:3000")?;
let server = Server::<InitRequest>::from(conn);

let client_conn = Connection::<InitRequest>::connect("127.0.0.1:3000")?;
let client = Client::<InitRequest>::from(conn);
```
Once both structs are instantiated, a connection should be initialized using your initial request struct (`InitRequest` in this example)
```rust
// this will hang until the server receives an `InitRequest` from the client, it will then send it's response and return the request it received
let init_req = server.initialize(InitResponse {})?;

// Does the inverse of what Server::<I>::initilialize does
let init_res = client.initialize(InitRequest {})?;
```
Once the connection has been initialized you can have each your client and server enter a loop and do whatever they need to.
> **Note**
> Make sure to call `join` on both `server.threads` and `client.threads` after you define your loop logic. 

Referring to the [Echo Example](https://github.com/voidKandy/seraphic/tree/refactor/examples) might be helpful


