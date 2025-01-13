use seraphic::packet::TcpPacket;
use serde::{Deserialize, Serialize};
use std::thread::sleep;
use std::time::Duration;
use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct TestData {
    id: u32,
    message: String,
}

#[tokio::test]
async fn test_async_tcp_packet_read_write() {
    let listener = TcpListener::bind("127.0.0.1:7879").await.unwrap();

    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(socket);
        let received: Option<TestData> = TcpPacket::async_read(&mut reader).await.unwrap();
        assert_eq!(
            received,
            Some(TestData {
                id: 42,
                message: "Async Hello".into()
            })
        );
    });

    sleep(Duration::from_millis(100));

    let mut stream = TcpStream::connect("127.0.0.1:7879").await.unwrap();
    let test_data = TestData {
        id: 42,
        message: "Async Hello".into(),
    };
    TcpPacket::async_write(&mut stream, &test_data)
        .await
        .unwrap();
}

