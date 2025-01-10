use core::panic;
use std::{
    fs::File,
    io::Write,
    thread::{self, sleep},
    time::Duration,
};

use seraphic::{
    client::Client, connection::InitializeConnectionMessage, error::ErrorCode, server::Server,
};
use tracing::Level;

use crate::{TestConnection, TestInitRequest, TestInitResponse};

#[test]
fn test_client_server_init_works() {
    // tracing::subscriber::set_global_default(
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();
    // )
    // .expect("setting default subscriber failed");

    let addr = "127.0.0.1:5567";

    let server_thread = thread::spawn(move || {
        let server_span = tracing::span!(Level::INFO, "server_thread", addr = addr);
        let _enter = server_span.enter();

        let init_response = TestInitResponse {};
        let server = Server::from(TestConnection::listen(addr).unwrap());
        let req = server.initialize(init_response).unwrap();
        assert!(TestInitRequest::matches(&req));
        tracing::warn!("server received init, ready to shutdown");
        let next = server.conn.receiver.recv().unwrap();
        assert!(matches!(next, seraphic::Message::Shutdown(false)));
        assert!(
            server.conn.handle_shutdown(&next).unwrap(),
            "expected to receive a shutdown"
        );
        // server.conn.sender.send(seraphic::Message::Exit).unwrap();

        server.threads.join().unwrap();
    });
    sleep(Duration::from_secs(1));

    let client_thread = thread::spawn(move || {
        let client_span = tracing::span!(Level::INFO, "client_thread", addr = addr);
        let _enter = client_span.enter();

        let client = Client::from(TestConnection::connect(addr).unwrap());

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

        client.threads.join().unwrap();
    });

    assert!(client_thread.join().is_ok());
    assert!(server_thread.join().is_ok());
}
