use ams::{
    AmsAddress, AmsHeader, Command, FLAG_REQUEST, Packet,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub fn request(command: Command, invoke_id: u32, data: Vec<u8>) -> Vec<u8> {
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

pub async fn start_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(adsim_server::serve(listener));
    addr
}

pub async fn send(stream: &mut TcpStream, frame: &[u8]) -> Packet {
    stream.write_all(frame).await.expect("write request");
    let mut head = [0u8; 6];
    stream.read_exact(&mut head).await.expect("frame header");
    let len = u32::from_le_bytes(head[2..6].try_into().expect("length")) as usize;
    let mut response = vec![0u8; 6 + len];
    response[..6].copy_from_slice(&head);
    stream
        .read_exact(&mut response[6..])
        .await
        .expect("frame body");
    Packet::decode(&response).expect("response frame")
}
