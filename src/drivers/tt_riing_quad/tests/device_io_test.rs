use super::super::{controller::READ_TIMEOUT, device_io::DeviceIO};
use anyhow::Result;
use std::sync::Mutex;

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
    let n = stub.write(&[1, 2, 3]).unwrap();
    std::assert_eq!(n, 3);
    let mut buf = [0u8; 1];
    stub.read(&mut buf, READ_TIMEOUT).unwrap();
    std::assert_eq!(buf[0], 0xAA);
    std::assert_eq!(stub.written(), vec![vec![1, 2, 3]]);
}
