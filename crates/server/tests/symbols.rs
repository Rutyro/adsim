mod common;

use ams::{Command, Packet};
use common::{request, send, start_server};
use tokio::net::TcpStream;

const SYM_HNDBYNAME: u32 = 0xF003;
const SYM_VALBYHND: u32 = 0xF005;
const SYM_RELEASEHND: u32 = 0xF006;
const SYM_UPLOAD: u32 = 0xF00B;
const SYM_UPLOADINFO2: u32 = 0xF00F;

fn read_request(index_group: u32, index_offset: u32, length: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&index_group.to_le_bytes());
    data.extend_from_slice(&index_offset.to_le_bytes());
    data.extend_from_slice(&length.to_le_bytes());
    data
}

fn write_request(index_group: u32, index_offset: u32, value: &[u8]) -> Vec<u8> {
    let mut data = read_request(index_group, index_offset, value.len() as u32);
    data.extend_from_slice(value);
    data
}

fn handle_request(name: &str, invoke_id: u32) -> Vec<u8> {
    let name = format!("{name}\0");
    let mut data = Vec::new();
    data.extend_from_slice(&SYM_HNDBYNAME.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&(name.len() as u32).to_le_bytes());
    data.extend_from_slice(name.as_bytes());
    request(Command::ReadWrite, invoke_id, data)
}

fn result(packet: &Packet) -> u32 {
    u32::from_le_bytes(packet.data[0..4].try_into().expect("result"))
}

fn length(packet: &Packet) -> usize {
    u32::from_le_bytes(packet.data[4..8].try_into().expect("length")) as usize
}

fn payload(packet: &Packet) -> &[u8] {
    &packet.data[8..8 + length(packet)]
}

#[tokio::test]
async fn reports_symbol_upload_info2() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let response = send(
        &mut stream,
        &request(Command::Read, 1, read_request(SYM_UPLOADINFO2, 0, 64)),
    )
    .await;

    assert_eq!(result(&response), 0);
    assert_eq!(length(&response), 24);
    let info = payload(&response);
    assert_eq!(
        u32::from_le_bytes(info[0..4].try_into().expect("symbol count")),
        15
    );
    assert!(u32::from_le_bytes(info[4..8].try_into().expect("symbol size")) > 0);
    assert_eq!(&info[8..24], &[0u8; 16]);
}

#[tokio::test]
async fn uploads_symbol_entries() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let info_response = send(
        &mut stream,
        &request(Command::Read, 1, read_request(SYM_UPLOADINFO2, 0, 64)),
    )
    .await;
    let info = payload(&info_response);
    let upload_size = u32::from_le_bytes(info[4..8].try_into().expect("symbol size"));

    let response = send(
        &mut stream,
        &request(Command::Read, 2, read_request(SYM_UPLOAD, 0, upload_size)),
    )
    .await;

    assert_eq!(result(&response), 0);
    assert_eq!(length(&response), upload_size as usize);
    let blob = payload(&response);
    assert!(
        blob.windows(b"MAIN.stAxisX.fActPos".len())
            .any(|window| window == b"MAIN.stAxisX.fActPos")
    );
    assert!(blob.windows(5).any(|window| window == b"LREAL"));
}

#[tokio::test]
async fn reads_and_writes_a_symbol_through_a_handle() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let response = send(
        &mut stream,
        &handle_request("MAIN.stAxisX.fActPos", 10),
    )
    .await;
    assert_eq!(result(&response), 0);
    assert_eq!(length(&response), 4);
    let handle = u32::from_le_bytes(payload(&response)[0..4].try_into().expect("handle"));
    assert_ne!(handle, 0);

    let response = send(
        &mut stream,
        &request(
            Command::Read,
            11,
            read_request(SYM_VALBYHND, handle, 8),
        ),
    )
    .await;
    assert_eq!(result(&response), 0);
    assert_eq!(payload(&response), &[0u8; 8]);

    let response = send(
        &mut stream,
        &request(
            Command::Write,
            12,
            write_request(SYM_VALBYHND, handle, &1.5f64.to_le_bytes()),
        ),
    )
    .await;
    assert_eq!(result(&response), 0);

    let response = send(
        &mut stream,
        &request(
            Command::Read,
            13,
            read_request(SYM_VALBYHND, handle, 8),
        ),
    )
    .await;
    assert_eq!(payload(&response), &1.5f64.to_le_bytes());
}

#[tokio::test]
async fn releases_a_symbol_handle() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let response = send(
        &mut stream,
        &handle_request("MAIN.stAxisX.fActPos", 20),
    )
    .await;
    let handle = u32::from_le_bytes(payload(&response)[0..4].try_into().expect("handle"));

    let response = send(
        &mut stream,
        &request(
            Command::Write,
            21,
            write_request(SYM_RELEASEHND, 0, &handle.to_le_bytes()),
        ),
    )
    .await;
    assert_eq!(result(&response), 0);

    let response = send(
        &mut stream,
        &request(
            Command::Read,
            22,
            read_request(SYM_VALBYHND, handle, 8),
        ),
    )
    .await;
    assert_eq!(result(&response), 0x0701);
    assert_eq!(length(&response), 0);
}

#[tokio::test]
async fn reads_symbol_info_by_name() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let name = b"MAIN.stAxisX.fActPos\0";
    let mut data = Vec::new();
    data.extend_from_slice(&0xF009u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&4096u32.to_le_bytes());
    data.extend_from_slice(&(name.len() as u32).to_le_bytes());
    data.extend_from_slice(name);

    let response = send(&mut stream, &request(Command::ReadWrite, 25, data)).await;

    assert_eq!(result(&response), 0);
    let entry = payload(&response);
    let entry_len = u32::from_le_bytes(entry[0..4].try_into().expect("entry length"));
    assert_eq!(entry_len as usize, entry.len());
    assert_eq!(&entry[4..8], &0x4020u32.to_le_bytes());
    assert_eq!(&entry[30..50], b"MAIN.stAxisX.fActPos");
    assert_eq!(entry[50], 0);
}

#[tokio::test]
async fn add_notification_returns_result_and_handle() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let mut data = Vec::new();
    data.extend_from_slice(&0xF008u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    let response = send(&mut stream, &request(Command::AddNotification, 26, data)).await;

    assert_eq!(response.data.len(), 8);
    assert_eq!(result(&response), 0x0701);
    assert_eq!(&response.data[4..8], &0u32.to_le_bytes());
}

#[tokio::test]
async fn reads_a_symbol_by_index_group_and_offset() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let response = send(
        &mut stream,
        &request(Command::Read, 30, read_request(0x4020, 0, 8)),
    )
    .await;

    assert_eq!(result(&response), 0);
    assert_eq!(payload(&response), &[0u8; 8]);
}
