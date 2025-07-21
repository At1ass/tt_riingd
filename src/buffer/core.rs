//! Core buffer types and traits.
//!
//! This module contains the fundamental building blocks for the buffer system:
//! traits, base types, and common functionality shared across all buffer types.

use anyhow::{Result, bail};
use std::sync::Arc;

use super::Layout;
use crate::drivers::registry::ControllerSpec;

/// Trait for types that can be stored in buffers.
///
/// This trait defines the requirements for data types that can be efficiently
/// stored and manipulated in the buffer system. It provides type-specific
/// information needed for proper layout calculation.
pub trait BufferElement: Copy + Default + Send + Sync {
    /// Returns the zero/default value for this element type.
    fn zero() -> Self {
        Self::default()
    }

    /// Returns the number of elements per channel for this type.
    ///
    /// - For speed data (`u8`): returns 1 (one speed value per fan channel)
    /// - For RGB data (`Rgb`): returns 0 (LED count is taken from hardware spec)
    fn leds_per_channel() -> usize;
}

/// Raw buffer data container with fixed capacity.
///
/// This structure holds the actual data array and provides basic access methods.
/// The capacity is determined at compile time for optimal performance.
pub struct BufferData<T: BufferElement, const CAPACITY: usize> {
    pub data: [T; CAPACITY],
}

impl<T: BufferElement, const CAPACITY: usize> Default for BufferData<T, CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: BufferElement, const CAPACITY: usize> BufferData<T, CAPACITY> {
    pub fn new() -> Self {
        Self {
            data: [T::zero(); CAPACITY],
        }
    }

    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }
}

/// Core trait defining buffer access strategies using closure-based API.
///
/// This trait replaces the traditional guard-based approach with a closure-based API
/// that eliminates `Send` trait issues and improves performance by capturing locks
/// only once per operation batch.
pub trait BufferStrategy<T: BufferElement, const CAPACITY: usize> {
    /// Execute a closure with mutable access to buffer data (async).
    fn with_write_access<R, F>(&self, f: F) -> impl std::future::Future<Output = Result<R>> + Send
    where
        F: FnOnce(&mut [T]) -> Result<R> + Send,
        R: Send;

    /// Execute a closure with read-only access to buffer data (async).
    fn with_read_access<R, F>(&self, f: F) -> impl std::future::Future<Output = Result<R>> + Send
    where
        F: FnOnce(&[T]) -> Result<R> + Send,
        R: Send;

    /// Execute a closure with mutable access to buffer data (sync).
    fn try_with_write_access<R, F>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&mut [T]) -> R;

    /// Execute a closure with read-only access to buffer data (sync).
    fn try_with_read_access<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&[T]) -> R;

    /// Swap read and write buffers (double-buffer only).
    fn swap_buffers(&self) -> Result<()>;

    /// Returns the total capacity of the buffer.
    fn capacity(&self) -> usize {
        CAPACITY
    }

    /// Take a snapshot of current buffer data for transmission.
    fn take_snapshot(
        &self,
    ) -> impl std::future::Future<Output = Result<Box<BufferData<T, CAPACITY>>>> + Send;

    /// Synchronous version of `take_snapshot` for single buffers.
    fn try_take_snapshot(&mut self) -> Option<Box<BufferData<T, CAPACITY>>>;

    /// Restore previously taken snapshot data.
    fn restore_snapshot(
        &self,
        data: Box<BufferData<T, CAPACITY>>,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Synchronous version of `restore_snapshot` for single buffers.
    fn try_restore_snapshot(&mut self, data: Box<BufferData<T, CAPACITY>>) -> Result<()>;
}

/// Owned snapshot of buffer data for async-safe transmission.
#[derive(Clone)]
pub struct Snapshot<T, const CAPACITY: usize>
where
    T: BufferElement,
{
    pub data: Arc<BufferData<T, CAPACITY>>,
    pub layout: Layout,
}

/// High-level buffer with layout management and typed operations.
///
/// This is the main buffer type that combines a buffering strategy with layout
/// information for hardware controllers. It provides typed, safe access to buffer
/// data with automatic bounds checking and controller-aware indexing.
pub struct Buffer<
    T,
    const CAPACITY: usize,
    B = super::strategies::SingleBufferStrategy<T, CAPACITY>,
> where
    T: BufferElement,
    B: BufferStrategy<T, CAPACITY>,
{
    pub data: B,
    pub layout: Layout,
    _marker: std::marker::PhantomData<T>,
}

impl<T, const CAPACITY: usize, B> Default for Buffer<T, CAPACITY, B>
where
    T: BufferElement,
    B: BufferStrategy<T, CAPACITY> + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const CAPACITY: usize, B> Buffer<T, CAPACITY, B>
where
    T: BufferElement,
    B: BufferStrategy<T, CAPACITY> + Default,
{
    pub fn new() -> Self {
        Self {
            data: B::default(),
            layout: Layout::default(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Create a buffer from hardware controller specifications.
    pub(crate) fn from_cache(controller_specs: &[ControllerSpec]) -> Self {
        let mut layout = Layout::new();

        controller_specs.iter().for_each(|spec| {
            let leds_per_channel = if T::leds_per_channel() == 1 {
                1
            } else {
                spec.leds as usize
            };

            let _ = layout.add_controller(
                spec.id.clone(),
                spec.channels as usize,
                leds_per_channel,
                (1u32 << spec.channels) - 1,
            );
        });

        Self {
            data: B::default(),
            layout,
            _marker: std::marker::PhantomData,
        }
    }

    /// Fill a specific controller channel with a value.
    pub async fn fill_controller_channel(
        &self,
        controller_id: &str,
        channel: usize,
        value: T,
    ) -> Result<()> {
        if let Some(entry) = self
            .layout
            .controllers
            .iter()
            .find(|c| c.controller_id == controller_id)
        {
            let start = entry.offset + channel;
            let end = start + channel * entry.leds_per_channel;

            self.data
                .with_write_access(move |data| {
                    if end <= data.len() {
                        data[start..end].fill(value);
                        Ok(())
                    } else {
                        bail!(
                            "Range out of bounds: start={}, end={}, len={}",
                            start,
                            end,
                            data.len()
                        )
                    }
                })
                .await
        } else {
            bail!("Controller '{}' not found in layout", controller_id);
        }
    }

    /// Execute a batch operation on buffer data.
    pub async fn batch_fill<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut [T]) -> Result<()> + Send,
    {
        self.data.with_write_access(f).await
    }

    pub async fn take_snapshot(&self) -> Result<Snapshot<T, CAPACITY>> {
        let data = self.data.take_snapshot().await?;
        Ok(Snapshot {
            data: Arc::new(*data),
            layout: self.layout.clone(),
        })
    }

    pub async fn restore_snapshot(&self, snapshot: Snapshot<T, CAPACITY>) -> Result<()> {
        let data = Arc::try_unwrap(snapshot.data)
            .map_err(|_| anyhow::anyhow!("Snapshot data is still referenced elsewhere"))?;
        self.data.restore_snapshot(Box::new(data)).await
    }

    pub fn try_take_snapshot(&mut self) -> Snapshot<T, CAPACITY> {
        let data = self.data.try_take_snapshot().unwrap();
        Snapshot {
            data: Arc::new(*data),
            layout: self.layout.clone(),
        }
    }

    pub fn try_restore_snapshot(&mut self, snapshot: Snapshot<T, CAPACITY>) -> Result<()> {
        let data = Arc::try_unwrap(snapshot.data)
            .map_err(|_| anyhow::anyhow!("Snapshot data is still referenced elsewhere"))?;
        self.data.try_restore_snapshot(Box::new(data))
    }
}
