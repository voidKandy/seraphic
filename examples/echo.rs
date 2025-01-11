use crossbeam_channel::{Receiver, TryRecvError};
use seraphic::{
    client::ClientConnection,
    connection::InitializeConnectionMessage,
    error::{Error, ErrorCode, ErrorKind},
    server::{Server, ServerConnection, ServerConnectionHandler},
    Message, MsgWrapper, Response, ResponseWrapper, RpcNamespace, RpcRequest, RpcResponse,
};
use seraphic_derive::{RequestWrapper, ResponseWrapper, RpcNamespace, RpcRequest};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Write,
    thread::{sleep, JoinHandle},
    time::Duration,
};

type MyWrapper = MsgWrapper<ReqWrapper, ResWrapper>;

#[derive(RequestWrapper, Debug)]
enum ReqWrapper {
    Foo(FooRequest),
    TriggerErr(TriggersErrorRequest),
}

#[derive(ResponseWrapper, Debug)]
enum ResWrapper {
    Foo(FooResponse),
}

#[derive(RpcNamespace, Clone, Copy, PartialEq, Eq)]
enum NS {
    Init,
    Foo,
}

#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
#[rpc_request(namespace = "NS:init")]
struct InitRequest {}

impl InitializeConnectionMessage for InitRequest {
    const ID: &str = "initialize";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitResponse {}

#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
#[rpc_request(namespace = "NS:foo")]
struct FooRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FooResponse {}

#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
#[rpc_request(namespace = "NS:foo", response = "FooResponse")]
struct TriggersErrorRequest {}

const ADDR: &str = "127.0.0.1:4569";

fn user_input_thread() -> Receiver<Message> {
    let (sender, recv) = crossbeam_channel::bounded(0);
    let stdin = std::io::stdin();
    let mut input = String::new();
    let mut req_id = 0;

    std::thread::spawn(move || {
        let mut options: HashMap<&str, Box<dyn Fn(u32) -> Message>> = HashMap::new();

        options.insert(
            "foo",
            Box::new(|id: u32| -> Message {
                Into::<Message>::into(FooRequest {}.into_request(id).unwrap())
            }),
        );
        options.insert(
            "err",
            Box::new(|id: u32| -> Message {
                Into::<Message>::into(TriggersErrorRequest {}.into_request(id).unwrap())
            }),
        );
        options.insert(
            "shutdown",
            Box::new(|_: u32| -> Message { Into::<Message>::into(Message::Shutdown(false)) }),
        );

        let mut should_exit = false;
        while !should_exit {
            println!(
                "type any of the following:\n{:#?}",
                options.keys().collect::<Vec<&&str>>()
            );

            if let Ok(bytes_read) = stdin.read_line(&mut input) {
                if bytes_read == 0 {
                    break;
                }

                let str = input.as_str().trim();

                match options.get(str) {
                    Some(get_msg_fn) => {
                        let msg = get_msg_fn(req_id);
                        should_exit = matches!(msg, Message::Shutdown(true));
                        sender.send(msg).unwrap();
                        req_id += 1;
                    }
                    _ => {
                        println!("no message associated with '{str}'");
                    }
                }
                std::io::stdout().flush().unwrap();
                input.clear();
            }
        }
    });
    recv
}

fn client_loop(client: ClientConnection<InitRequest>) {
    println!("starting client loop");
    let user_input_recv = user_input_thread();
    loop {
        match client.conn.receiver.try_recv() {
            Ok(msg) => {
                println!("client received: {msg:#?}");
                let wrapper = MyWrapper::try_from(msg).expect("failed to get wrapper");
                match wrapper {
                    MsgWrapper::Shutdown(_) => {
                        assert!(client.conn.handle_shutdown(&wrapper.into()).unwrap());
                        break;
                    }
                    MsgWrapper::Exit => {
                        unreachable!("receiving this should be handled in handle_shutdown");
                    }
                    MsgWrapper::Req { .. } => {}
                    MsgWrapper::Res { .. } => {}
                    MsgWrapper::Err { .. } => {}
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                println!("disconnected");
                break;
            }
        }

        match user_input_recv.try_recv() {
            Ok(msg) => {
                client.conn.sender.send(msg).unwrap();
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }
    }

    client.conn.io_threads.join().unwrap();
    println!("Goodbye from client!");
}

struct ServerConnHandler;
impl ServerConnectionHandler<InitRequest> for ServerConnHandler {
    fn handler(conn: &mut ServerConnection<InitRequest>) -> seraphic::server::ServerHandlerResult {
        let init_req = conn.initialize(InitResponse {}).unwrap();
        println!("server started w/ init params from client: {init_req:#?}");
        loop {
            match conn.conn.receiver.try_recv() {
                Ok(msg) => {
                    println!("server received: {msg:#?}");
                    let wrapper = MyWrapper::try_from(msg).expect("failed to get wrapper");
                    match wrapper {
                        MsgWrapper::Req { id, req } => match req {
                            ReqWrapper::Foo(_foo) => {
                                let response = FooResponse {};
                                conn.conn
                                    .sender
                                    .send(response.into_response(id).unwrap().into())
                                    .unwrap()
                            }
                            ReqWrapper::TriggerErr(_foo_err) => {
                                let error: Error = ErrorKind::other(
                                    "received req that triggers err, returning error response",
                                    ErrorCode::InternalError,
                                )
                                .into();
                                let response = Response::from_error(id, error);
                                conn.conn.sender.send(response.into()).unwrap()
                            }
                        },
                        MsgWrapper::Res { .. } => {}
                        MsgWrapper::Shutdown(_) => {
                            assert!(conn.conn.handle_shutdown(&wrapper.into()).unwrap());
                            break;
                        }
                        MsgWrapper::Err { .. } => {}
                        MsgWrapper::Exit => {
                            unreachable!("receiving this should be handled in handle_shutdown");
                        }
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    println!("disconnected");
                    break;
                }
            }
        }

        println!("Goodbye from server!");
        Ok(())
    }
}

fn main() {
    let task: JoinHandle<Result<Server<InitRequest, ServerConnHandler>, &'static str>> =
        std::thread::spawn(move || {
            let mut server = Server::<InitRequest, ServerConnHandler>::listen(ADDR).unwrap();
            let (connection, conn_shutdown) = server.next().unwrap().unwrap();
            server.spawn_connection_thread(connection, conn_shutdown);
            // server.shutdown_and_join_all_connections().unwrap();
            Ok(server)
        });
    sleep(Duration::from_secs(1));

    let client = ClientConnection::connect(ADDR).unwrap();
    let init_res = client.initialize(InitRequest {}).unwrap().unwrap();
    println!("client started w/ init params from server: {init_res:#?}");
    client_loop(client);

    let mut server = task.join().unwrap().unwrap();
    tracing::warn!("got server from thread");

    server.shutdown_and_join_all_connections().unwrap();
    server.shutdown();
}
