//! Buffer strategies for single and double buffering.
//!
//! This module provides different buffering strategies that can be used
//! with the generic buffer system for optimal performance in different scenarios.

use anyhow::{Result, bail};
use std::sync::atomic::AtomicUsize;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing::error;

use super::core::{BufferData, BufferElement, BufferStrategy};

/// Single-buffered strategy for synchronous access patterns.
///
/// This strategy provides direct synchronous access to buffer data and is suitable
/// for scenarios where simple, blocking access is sufficient. It uses `Option<Box<_>>`
/// to support snapshot operations where data can be temporarily moved out.
pub struct SingleBufferStrategy<T, const CAPACITY: usize>
where
    T: BufferElement,
{
    data: Option<Box<BufferData<T, CAPACITY>>>,
}

impl<T: BufferElement, const CAPACITY: usize> Default for SingleBufferStrategy<T, CAPACITY> {
    fn default() -> Self {
        Self {
            data: Some(Box::new(BufferData::new())),
        }
    }
}

impl<T: BufferElement, const CAPACITY: usize> BufferStrategy<T, CAPACITY>
    for SingleBufferStrategy<T, CAPACITY>
{
    async fn with_write_access<R, F>(&self, _f: F) -> Result<R>
    where
        F: FnOnce(&mut [T]) -> Result<R> + Send,
        R: Send,
    {
        bail!("SingleBuffer doesn't support async write access - use try_with_write_access instead")
    }

    async fn with_read_access<R, F>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&[T]) -> Result<R> + Send,
        R: Send,
    {
        if let Some(data) = self.data.as_ref() {
            f(data.as_slice())
        } else {
            bail!("Buffer data is not initialized")
        }
    }

    fn try_with_write_access<R, F>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&mut [T]) -> R,
    {
        let data = self.data.as_mut()?;
        Some(f(data.as_mut_slice()))
    }

    fn try_with_read_access<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&[T]) -> R,
    {
        let data = self.data.as_ref()?;
        Some(f(data.as_slice()))
    }

    fn swap_buffers(&self) -> Result<()> {
        // No-op for single buffer
        Ok(())
    }

    async fn take_snapshot(&self) -> Result<Box<BufferData<T, CAPACITY>>> {
        bail!("SingleBuffer snapshot requires mutable access - use try_take_snapshot instead")
    }

    fn try_take_snapshot(&mut self) -> Option<Box<BufferData<T, CAPACITY>>> {
        self.data.take()
    }

    async fn restore_snapshot(&self, _data: Box<BufferData<T, CAPACITY>>) -> Result<()> {
        bail!("SingleBuffer restore requires mutable access - use try_restore_snapshot instead")
    }

    fn try_restore_snapshot(&mut self, data: Box<BufferData<T, CAPACITY>>) -> Result<()> {
        if self.data.is_some() {
            bail!("Buffer already has data");
        }
        self.data = Some(data);
        Ok(())
    }
}

/// Double-buffered strategy for concurrent read/write access patterns.
///
/// This strategy maintains two buffers to allow concurrent reading and writing operations.
/// It uses atomic index switching to provide lock-free buffer swapping and is designed
/// for high-performance scenarios where producers and consumers operate concurrently.
pub struct DoubleBufferStrategy<T, const CAPACITY: usize>
where
    T: BufferElement,
{
    data: [RwLock<Option<Box<BufferData<T, CAPACITY>>>>; 2],
    write_index: AtomicUsize,
}

impl<T, const CAPACITY: usize> DoubleBufferStrategy<T, CAPACITY>
where
    T: BufferElement,
{
    pub fn new() -> Self {
        Self {
            data: [
                RwLock::new(Some(Box::new(BufferData::new()))),
                RwLock::new(Some(Box::new(BufferData::new()))),
            ],
            write_index: AtomicUsize::new(0),
        }
    }

    async fn get_write_buffer(&self) -> RwLockWriteGuard<'_, Option<Box<BufferData<T, CAPACITY>>>> {
        let index = self.write_index.load(std::sync::atomic::Ordering::SeqCst);
        self.data[index].write().await
    }

    async fn get_read_buffer(&self) -> RwLockReadGuard<'_, Option<Box<BufferData<T, CAPACITY>>>> {
        let index = self.write_index.load(std::sync::atomic::Ordering::SeqCst) ^ 1;
        self.data[index].read().await
    }

    async fn get_read_buffer_mut(
        &self,
    ) -> RwLockWriteGuard<'_, Option<Box<BufferData<T, CAPACITY>>>> {
        let index = self.write_index.load(std::sync::atomic::Ordering::SeqCst) ^ 1;
        self.data[index].write().await
    }

    fn swap_buffers(&self) {
        self.write_index
            .fetch_xor(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl<T: BufferElement, const CAPACITY: usize> Default for DoubleBufferStrategy<T, CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: BufferElement, const CAPACITY: usize> BufferStrategy<T, CAPACITY>
    for DoubleBufferStrategy<T, CAPACITY>
{
    async fn with_write_access<R, F>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut [T]) -> Result<R> + Send,
        R: Send,
    {
        let mut guard = self.get_write_buffer().await;
        if let Some(data) = guard.as_mut() {
            f(data.as_mut_slice())
        } else {
            bail!("Buffer data is not initialized")
        }
    }

    async fn with_read_access<R, F>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&[T]) -> Result<R> + Send,
        R: Send,
    {
        let guard = self.get_read_buffer().await;
        if let Some(data) = guard.as_ref() {
            f(data.as_slice())
        } else {
            bail!("Buffer data is not initialized")
        }
    }

    fn try_with_write_access<R, F>(&mut self, _f: F) -> Option<R>
    where
        F: FnOnce(&mut [T]) -> R,
    {
        error!("DoubleBuffer doesn't support sync write access - use with_write_access instead");
        None
    }

    fn try_with_read_access<R, F>(&self, _f: F) -> Option<R>
    where
        F: FnOnce(&[T]) -> R,
    {
        error!("DoubleBuffer doesn't support sync read access - use with_read_access instead");
        None
    }

    fn swap_buffers(&self) -> Result<()> {
        self.swap_buffers();
        Ok(())
    }

    async fn take_snapshot(&self) -> Result<Box<BufferData<T, CAPACITY>>> {
        self.swap_buffers();
        let mut guard = self.get_read_buffer_mut().await;
        guard
            .take()
            .ok_or_else(|| anyhow::anyhow!("Buffer data is not initialized"))
    }

    fn try_take_snapshot(&mut self) -> Option<Box<BufferData<T, CAPACITY>>> {
        error!("DoubleBuffer snapshot requires async access - use take_snapshot instead");
        None
    }

    async fn restore_snapshot(&self, data: Box<BufferData<T, CAPACITY>>) -> Result<()> {
        let mut guard = self.get_read_buffer_mut().await;
        guard.replace(data);
        Ok(())
    }

    fn try_restore_snapshot(&mut self, _data: Box<BufferData<T, CAPACITY>>) -> Result<()> {
        bail!("DoubleBuffer restore requires async access - use restore_snapshot instead");
    }
}
