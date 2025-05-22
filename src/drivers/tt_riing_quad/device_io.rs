use anyhow::{Result, Ok, anyhow};
use hidapi::{HidDevice, HidError};

pub trait DeviceIO: Send + 'static {
    fn write(&self, buf: &[u8]) -> Result<usize>;
    fn read(&self, buf: &mut [u8], timeout: i32) -> Result<()>;
}

impl DeviceIO for HidDevice {
    fn write(&self, buf: &[u8]) -> Result<usize> {
        Self::write(self, buf).map_err(|e| anyhow!("{e}"))
    }
    fn read(&self, buf: &mut [u8], timeout: i32) -> Result<()> {
        let n = Self::read_timeout(self, buf, timeout)?;
        if n > 0 {
            Ok(())
        } else {
            Err(HidError::HidApiError {
                message: ("IncompleteRead".to_string()),
            })
            .map_err(|e| anyhow!("{e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::drivers::tt_riing_quad::controller::READ_TIMEOUT;

    use super::DeviceIO;
    use std::sync::Mutex;
    use anyhow::Result;

    struct StubIo {
        written: Mutex<Vec<Vec<u8>>>,
        responses: Mutex<Vec<Vec<u8>>>,
    }

    impl StubIo {
        fn new(resps: Vec<Vec<u8>>) -> Self {
            StubIo {
                written: Mutex::new(vec![]),
                responses: Mutex::new(resps),
            }
        }
        fn written(&self) -> Vec<Vec<u8>> {
            self.written.lock().unwrap().clone()
        }
    }

    impl DeviceIO for StubIo {
        fn write(&self, buf: &[u8]) -> Result<usize> {
            self.written.lock().unwrap().push(buf.to_vec());
            Ok(buf.len())
        }
        fn read(&self, buf: &mut [u8], _timeout: i32) -> Result<()> {
            let mut resp = self.responses.lock().unwrap();
            let next = resp.remove(0);
            buf[..next.len()].copy_from_slice(&next);
            Ok(())
        }
    }

    #[test]
    fn stub_io_cycle() {
        let stub = StubIo::new(vec![vec![0xAA]]);
        let n = stub.write(&[1,2,3]).unwrap();
        assert_eq!(n, 3);
        let mut buf = [0u8;1];
        stub.read(&mut buf, READ_TIMEOUT).unwrap();
        assert_eq!(buf[0], 0xAA);
        assert_eq!(stub.written(), vec![vec![1,2,3]]);
    }
}
