use anyhow::{Ok, Result, anyhow};

/// Protocol constants for TT Riing Quad HID communication
mod protocol_consts {
    // Command codes
    pub const CMD_INIT: u8 = 0x33;
    pub const CMD_GET_FW_VERSION: u8 = 0x50;
    pub const CMD_GET_DATA: u8 = 0x51;
    pub const CMD_SET_SPEED: u8 = 0x51;
    pub const CMD_SET_RGB: u8 = 0x52;

    // Prefix bytes
    pub const PREFIX_0: u8 = 0x00;
    pub const PREFIX_1_FE: u8 = 0xFE;
    pub const PREFIX_1_32: u8 = 0x32;
    pub const PREFIX_1_33: u8 = 0x33;

    // SetSpeed/SetRgb extra
    pub const SPEED_FLAG: u8 = 0x01;

    // Response buffer length
    pub const RESPONSE_LEN: usize = 193;

    // Response offsets
    pub const STATUS_OFFSET: usize = 2;
    pub const FW_MAJOR_OFFSET: usize = 0;
    pub const FW_MINOR_OFFSET: usize = 1;
    pub const FW_PATCH_OFFSET: usize = 2;
    pub const DATA_SPEED_OFFSET: usize = 2;
    pub const DATA_RPM_LOW_OFFSET: usize = 3;
    pub const DATA_RPM_HIGH_OFFSET: usize = 4;
}

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
        use protocol_consts::*;
        match *self {
            Command::Init => vec![PREFIX_0, PREFIX_1_FE, CMD_INIT],
            Command::GetFirmwareVersion => vec![PREFIX_0, PREFIX_1_33, CMD_GET_FW_VERSION],
            Command::GetData { port } => vec![PREFIX_0, PREFIX_1_33, CMD_GET_DATA, port],
            Command::SetSpeed { port, speed } => vec![
                PREFIX_0,
                PREFIX_1_32,
                CMD_SET_SPEED,
                port,
                SPEED_FLAG,
                speed,
            ],
            Command::SetRgb {
                port,
                mode,
                ref colors,
            } => {
                let mut buf = Vec::with_capacity(5 + 3 * colors.len());
                buf.extend_from_slice(&[PREFIX_0, PREFIX_1_32, CMD_SET_RGB, port, mode]);
                for &(g, r, b) in colors {
                    buf.extend_from_slice(&[g, r, b]);
                }
                buf
            }
        }
    }

    pub fn expected_response_len(&self) -> usize {
        use protocol_consts::RESPONSE_LEN;
        RESPONSE_LEN
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    Status(u8),
    FirmwareVersion { major: u8, minor: u8, patch: u8 },
    Data { speed: u8, rpm: u16 },
}

impl Response {
    pub fn parse(cmd: Command, buf: &[u8]) -> Result<Self> {
        use protocol_consts::*;
        match cmd {
            Command::Init | Command::SetSpeed { .. } | Command::SetRgb { .. } => {
                let code = buf
                    .get(STATUS_OFFSET)
                    .copied()
                    .ok_or_else(|| anyhow!("Empty status. Buf: {:?}", buf))?;
                Ok(Response::Status(code))
            }
            Command::GetFirmwareVersion => {
                if buf.len() < 3 {
                    return Err(anyhow!("Buf too small for FW"));
                }
                Ok(Response::FirmwareVersion {
                    major: buf[FW_MAJOR_OFFSET],
                    minor: buf[FW_MINOR_OFFSET],
                    patch: buf[FW_PATCH_OFFSET],
                })
            }
            Command::GetData { .. } => {
                if buf.len() < 5 {
                    return Err(anyhow!("Buf too small for Data"));
                }
                let speed = buf[DATA_SPEED_OFFSET];
                let rpm =
                    u16::from(buf[DATA_RPM_HIGH_OFFSET]) << 8 | u16::from(buf[DATA_RPM_LOW_OFFSET]);
                Ok(Response::Data { speed, rpm })
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{Command, Response};

    #[test]
    fn set_speed_bytes_and_parse_status() {
        let cmd = Command::SetSpeed {
            port: 2,
            speed: 123,
        };
        let bytes = cmd.to_bytes();
        assert_eq!(bytes, vec![0x00, 0x32, 0x51, 2, 0x01, 123]);

        let mut buf = [0u8; 193];
        buf[2] = 0xFC;
        let resp = Response::parse(cmd, &buf).unwrap();
        assert_eq!(resp, Response::Status(0xFC));
    }

    #[test]
    fn get_data_parse() {
        let cmd = Command::GetData { port: 1 };
        let mut buf = [0u8; 193];
        buf[2] = 55;
        buf[3] = 0x10;
        buf[4] = 0x20; // rpm = 0x2010 = 8208
        let resp = Response::parse(cmd, &buf).unwrap();
        match resp {
            Response::Data { speed, rpm } => {
                assert_eq!(speed, 55);
                assert_eq!(rpm, 0x2010);
            }
            _ => panic!("expected Data"),
        }
    }

    #[test]
    fn set_rgb_bytes() {
        let colors = vec![(1, 2, 3); 52];
        let cmd = Command::SetRgb {
            port: 3,
            mode: 0x24,
            colors: colors.clone(),
        };
        let bytes = cmd.to_bytes();
        assert_eq!(bytes[0..5], [0x00, 0x32, 0x52, 3, 0x24]);
        // payload
        for chunk in bytes[5..].chunks(3) {
            assert_eq!(chunk, &[1, 2, 3]);
        }
        assert_eq!(bytes.len(), 5 + 52 * 3);
    }
}
