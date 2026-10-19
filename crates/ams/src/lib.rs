//! AMS/TCP codec for the ADS subset used by adsim. No IO.

pub const FLAG_REQUEST: u16 = 0x0004;
pub const FLAG_RESPONSE: u16 = 0x0005;

pub const AMS_TCP_HEADER_LEN: usize = 6;
pub const AMS_HEADER_LEN: usize = 32;
pub const FRAME_HEADER_LEN: usize = AMS_TCP_HEADER_LEN + AMS_HEADER_LEN;

const DEVICE_NAME_LEN: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmsAddress {
    pub net_id: [u8; 6],
    pub port: u16,
}

impl AmsAddress {
    pub const fn new(net_id: [u8; 6], port: u16) -> Self {
        Self { net_id, port }
    }

    fn write(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.net_id);
        out.extend_from_slice(&self.port.to_le_bytes());
    }

    fn read(bytes: &[u8]) -> Self {
        let mut net_id = [0; 6];
        net_id.copy_from_slice(&bytes[..6]);
        let port = u16::from_le_bytes(bytes[6..8].try_into().expect("8-byte address"));
        Self { net_id, port }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    ReadDeviceInfo,
    Read,
    Write,
    ReadState,
    WriteControl,
    AddNotification,
    DeleteNotification,
    Notification,
    ReadWrite,
}

impl Command {
    pub const fn id(self) -> u16 {
        match self {
            Self::ReadDeviceInfo => 0x0001,
            Self::Read => 0x0002,
            Self::Write => 0x0003,
            Self::ReadState => 0x0004,
            Self::WriteControl => 0x0005,
            Self::AddNotification => 0x0006,
            Self::DeleteNotification => 0x0007,
            Self::Notification => 0x0008,
            Self::ReadWrite => 0x0009,
        }
    }

    pub const fn from_id(id: u16) -> Option<Self> {
        match id {
            0x0001 => Some(Self::ReadDeviceInfo),
            0x0002 => Some(Self::Read),
            0x0003 => Some(Self::Write),
            0x0004 => Some(Self::ReadState),
            0x0005 => Some(Self::WriteControl),
            0x0006 => Some(Self::AddNotification),
            0x0007 => Some(Self::DeleteNotification),
            0x0008 => Some(Self::Notification),
            0x0009 => Some(Self::ReadWrite),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmsHeader {
    pub target: AmsAddress,
    pub source: AmsAddress,
    pub command: Command,
    pub flags: u16,
    pub error_code: u32,
    pub invoke_id: u32,
}

impl AmsHeader {
    fn write(self, data_len: usize, out: &mut Vec<u8>) {
        self.target.write(out);
        self.source.write(out);
        out.extend_from_slice(&self.command.id().to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&(data_len as u32).to_le_bytes());
        out.extend_from_slice(&self.error_code.to_le_bytes());
        out.extend_from_slice(&self.invoke_id.to_le_bytes());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub header: AmsHeader,
    pub data: Vec<u8>,
}

impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(FRAME_HEADER_LEN + self.data.len());
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&((AMS_HEADER_LEN + self.data.len()) as u32).to_le_bytes());
        self.header.write(self.data.len(), &mut out);
        out.extend_from_slice(&self.data);
        out
    }

    pub fn decode(frame: &[u8]) -> Result<Self, DecodeError> {
        if frame.len() < FRAME_HEADER_LEN {
            return Err(DecodeError::TooShort);
        }
        if frame[0] != 0 || frame[1] != 0 {
            return Err(DecodeError::ReservedNotZero);
        }

        let declared = u32::from_le_bytes(frame[2..6].try_into().expect("4-byte length")) as usize;
        if declared < AMS_HEADER_LEN || frame.len() != AMS_TCP_HEADER_LEN + declared {
            return Err(DecodeError::BadLength);
        }

        let header = &frame[AMS_TCP_HEADER_LEN..FRAME_HEADER_LEN];
        let command_id =
            u16::from_le_bytes(header[16..18].try_into().expect("2-byte command"));
        let command =
            Command::from_id(command_id).ok_or(DecodeError::UnknownCommand(command_id))?;
        let data_len =
            u32::from_le_bytes(header[20..24].try_into().expect("4-byte data length")) as usize;
        if data_len != declared - AMS_HEADER_LEN {
            return Err(DecodeError::BadLength);
        }

        Ok(Self {
            header: AmsHeader {
                target: AmsAddress::read(&header[0..8]),
                source: AmsAddress::read(&header[8..16]),
                command,
                flags: u16::from_le_bytes(header[18..20].try_into().expect("2-byte flags")),
                error_code: u32::from_le_bytes(
                    header[24..28].try_into().expect("4-byte error code"),
                ),
                invoke_id: u32::from_le_bytes(
                    header[28..32].try_into().expect("4-byte invoke id"),
                ),
            },
            data: frame[FRAME_HEADER_LEN..].to_vec(),
        })
    }

    pub fn response(&self, data: Vec<u8>) -> Self {
        Self {
            header: AmsHeader {
                target: self.header.source,
                source: self.header.target,
                command: self.header.command,
                flags: FLAG_RESPONSE,
                error_code: 0,
                invoke_id: self.header.invoke_id,
            },
            data,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    TooShort,
    ReservedNotZero,
    BadLength,
    UnknownCommand(u16),
}

pub fn read_state_response(result: u32, ads_state: u16, device_state: u16) -> Vec<u8> {
    let mut data = Vec::with_capacity(8);
    data.extend_from_slice(&result.to_le_bytes());
    data.extend_from_slice(&ads_state.to_le_bytes());
    data.extend_from_slice(&device_state.to_le_bytes());
    data
}

pub fn read_device_info_response(result: u32, version: (u8, u8, u16), name: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + 4 + DEVICE_NAME_LEN);
    data.extend_from_slice(&result.to_le_bytes());
    data.push(version.0);
    data.push(version.1);
    data.extend_from_slice(&version.2.to_le_bytes());

    let mut name_bytes = [0u8; DEVICE_NAME_LEN];
    for (slot, byte) in name_bytes.iter_mut().zip(name.as_bytes()) {
        *slot = *byte;
    }
    data.extend_from_slice(&name_bytes);
    data
}
