use anyhow::{Ok, Result, anyhow};

#[derive(Clone, Debug)]
pub enum Command {
    Init,
    GetFirmwareVersion,
    GetData {
        port: u8,
    },
    SetSpeed {
        port: u8,
        speed: u8,
    },
    SetRgb {
        port: u8,
        mode: u8,
        colors: Vec<(u8, u8, u8)>,
    },
}

impl Command {
    pub fn to_bytes(&self) -> Vec<u8> {
        match *self {
            Command::Init => vec![0x00, 0xFE, 0x033],
            Command::GetFirmwareVersion => vec![0x00, 0x33, 0x50],
            Command::GetData { port } => vec![0x00, 0x33, 0x51, port],
            Command::SetSpeed { port, speed } => vec![0x00, 0x32, 0x51, port, 0x01, speed],
            Command::SetRgb {
                port,
                mode,
                ref colors,
            } => {
                let mut buf = Vec::with_capacity(5 + 3 * colors.len());
                buf.extend_from_slice(&[0x00, 0x32, 0x52, port, mode]);
                for &(g, r, b) in colors {
                    buf.extend_from_slice(&[g, r, b]);
                }
                buf
            }
        }
    }

    pub fn expected_response_len(&self) -> usize {
        match *self {
            Command::Init | Command::SetSpeed { .. } | Command::SetRgb { .. } => 193,
            Command::GetFirmwareVersion => 193,
            Command::GetData { .. } => 193,
        }
    }
}

pub enum Response {
    Status(u8),
    FirmwareVersion { major: u8, minor: u8, patch: u8 },
    Data { speed: u8, rpm: u16 },
}

impl Response {
    pub fn parse(cmd: Command, buf: &[u8]) -> Result<Self> {
        match cmd {
            Command::Init | Command::SetSpeed { .. } | Command::SetRgb { .. } => {
                let code = buf
                    .get(2)
                    .copied()
                    .ok_or_else(|| anyhow!("Empty status. Buf: {:?}", buf))?;
                Ok(Response::Status(code))
            }
            Command::GetFirmwareVersion => {
                if buf.len() < 3 {
                    return Err(anyhow!("Buf too small for FW"));
                }
                Ok(Response::FirmwareVersion {
                    major: buf[0],
                    minor: buf[1],
                    patch: buf[2],
                })
            }
            Command::GetData { .. } => {
                if buf.len() < 5 {
                    return Err(anyhow!("Buf too small for Data"));
                }
                let speed = buf[2];
                let rpm = u16::from(buf[4]) << 8 | u16::from(buf[3]);
                Ok(Response::Data { speed, rpm })
            }
        }
    }
}
