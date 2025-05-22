use anyhow::{Result, anyhow};
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
