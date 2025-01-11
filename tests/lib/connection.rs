use crate::{TestInitRequest, TestInitResponse};
use seraphic::{
    client::ClientConnection, connection::InitializeConnectionMessage, server::ServerConnection,
};
use std::{
    thread::{self, sleep},
    time::Duration,
};
use tracing::Level;

#[test]
fn test_client_server_init_works() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let addr = "127.0.0.1:5567";

    let server_thread = thread::spawn(move || {
        let server_span = tracing::span!(Level::INFO, "server_thread", addr = addr);
        let _enter = server_span.enter();

        let init_response = TestInitResponse {};
        let server = ServerConnection::<TestInitRequest>::incoming(addr)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let req = server.initialize(init_response).unwrap();
        assert!(TestInitRequest::matches(&req));
        tracing::warn!("server received init, ready to shutdown");
        let next = server.conn.receiver.recv().unwrap();
        assert!(matches!(next, seraphic::Message::Shutdown(false)));
        assert!(
            server.conn.handle_shutdown(&next).unwrap(),
            "expected to receive a shutdown"
        );

        server.conn.io_threads.join().unwrap();
    });
    sleep(Duration::from_secs(1));

    let client_thread = thread::spawn(move || {
        let client_span = tracing::span!(Level::INFO, "client_thread", addr = addr);
        let _enter = client_span.enter();

        let client = ClientConnection::connect(addr).unwrap();

        let res = client.initialize(TestInitRequest {}).unwrap().unwrap();
        assert!(TestInitRequest::matches(&res));
        tracing::warn!("client sending shutdown");
        client
            .conn
            .sender
            .send(seraphic::Message::Shutdown(false))
            .unwrap();

        let next = client.conn.receiver.recv().unwrap();
        assert!(matches!(next, seraphic::Message::Shutdown(true)));
        assert!(
            client.conn.handle_shutdown(&next).unwrap(),
            "expected to receive a shutdown"
        );

        client.conn.io_threads.join().unwrap();
    });

    assert!(client_thread.join().is_ok());
    assert!(server_thread.join().is_ok());
}

#[test]
fn test_concurrent_connections_to_server() {
    // for server_connection_result in ServerConnection::<TestInitRequest>::incoming("127.0.0.1:3333").unwrap().into_iter() {
    //     let conn = server_connection_result.unwrap();
    //     thread::spawn(move ||{
    //         let init_rq = conn.initialize(TestInitResponse {}).unwrap();
    //     })
    //
    //
    // }
}
