use ams::{
    AmsAddress, AmsHeader, Command, FLAG_REQUEST, FLAG_RESPONSE, Packet,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn request(command: Command, invoke_id: u32, data: Vec<u8>) -> Vec<u8> {
    Packet {
        header: AmsHeader {
            target: AmsAddress::new([127, 0, 0, 1, 1, 1], 851),
            source: AmsAddress::new([10, 0, 0, 1, 1, 1], 30000),
            command,
            flags: FLAG_REQUEST,
            error_code: 0,
            invoke_id,
        },
        data,
    }
    .encode()
}

async fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
    let mut head = [0u8; 6];
    stream.read_exact(&mut head).await.expect("frame header");
    let len = u32::from_le_bytes(head[2..6].try_into().expect("length")) as usize;
    let mut frame = vec![0u8; 6 + len];
    frame[..6].copy_from_slice(&head);
    stream.read_exact(&mut frame[6..]).await.expect("frame body");
    frame
}

async fn start_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(adsim_server::serve(listener));
    addr
}

#[tokio::test]
async fn answers_read_state() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    stream
        .write_all(&request(Command::ReadState, 42, Vec::new()))
        .await
        .expect("write request");
    let frame = read_frame(&mut stream).await;
    let response = Packet::decode(&frame).expect("response frame");

    assert_eq!(response.header.flags, FLAG_RESPONSE);
    assert_eq!(response.header.command, Command::ReadState);
    assert_eq!(response.header.invoke_id, 42);
    assert_eq!(response.header.target, AmsAddress::new([10, 0, 0, 1, 1, 1], 30000));

    let result = u32::from_le_bytes(response.data[0..4].try_into().expect("result"));
    let ads_state = u16::from_le_bytes(response.data[4..6].try_into().expect("ads state"));
    let device_state = u16::from_le_bytes(response.data[6..8].try_into().expect("device state"));
    assert_eq!(result, 0);
    assert_eq!(ads_state, 5);
    assert_eq!(device_state, 0);
}

#[tokio::test]
async fn answers_read_device_info() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    stream
        .write_all(&request(Command::ReadDeviceInfo, 7, Vec::new()))
        .await
        .expect("write request");
    let response = Packet::decode(&read_frame(&mut stream).await).expect("response frame");

    let result = u32::from_le_bytes(response.data[0..4].try_into().expect("result"));
    assert_eq!(result, 0);
    assert_eq!(&response.data[8..13], b"adsim");
    assert!(response.data[13..24].iter().all(|byte| *byte == 0));
}

#[tokio::test]
async fn serves_several_requests_on_one_connection() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    for invoke_id in [1, 2] {
        stream
            .write_all(&request(Command::ReadState, invoke_id, Vec::new()))
            .await
            .expect("write request");
        let response = Packet::decode(&read_frame(&mut stream).await).expect("response frame");
        assert_eq!(response.header.invoke_id, invoke_id);
    }
}

#[tokio::test]
async fn unsupported_command_returns_ads_error() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let data = vec![0u8; 12];
    stream
        .write_all(&request(Command::Read, 3, data))
        .await
        .expect("write request");
    let response = Packet::decode(&read_frame(&mut stream).await).expect("response frame");

    let result = u32::from_le_bytes(response.data[0..4].try_into().expect("result"));
    assert_eq!(result, 0x0701);
}
