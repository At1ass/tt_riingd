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

#[derive(Debug, PartialEq, Eq)]
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
