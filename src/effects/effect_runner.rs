use async_stream::stream;
use core::fmt;
use futures::{Stream, StreamExt};
use std::{pin::Pin, sync::Arc};
use tokio::{
    sync::Mutex,
    time::{Duration, Instant},
};

type RgbStream = Arc<Mutex<Pin<Box<dyn Stream<Item = [u8; 3]> + Send>>>>;

#[derive(Clone)]
pub struct EffectRunner {
    inner: RgbStream,
}

impl fmt::Debug for EffectRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectRunner").finish_non_exhaustive()
    }
}

impl EffectRunner {
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = [u8; 3]> + Send + 'static,
    {
        Self {
            inner: Arc::new(Mutex::new(Box::pin(stream))),
        }
    }

    pub async fn next_rgb(&self) -> Option<[u8; 3]> {
        self.inner.lock().await.next().await
    }

    pub fn constant(rgb: [u8; 3]) -> Self {
        Self::new(stream! {
            loop { yield rgb; }
        })
    }

    pub fn rainbow(period: Duration) -> Self {
        Self::new(stream! {
            let start = Instant::now();
            loop {
                let t = (start.elapsed().as_secs_f32() / period.as_secs_f32()) % 1.0;
                let hue = t * 360.0;
                yield hsv_to_rgb(hue, 1.0, 1.0);
            }
        })
    }

    pub fn breathe(rgb: [u8; 3], min: f32, max: f32, period: Duration) -> Self {
        Self::new(stream! {
            let start = Instant::now();
            loop {
                let t = (start.elapsed().as_secs_f32() / period.as_secs_f32()) % 1.0;
                let phase = (t * std::f32::consts::TAU).sin();
                // sin(phase) ∈ [-1,1]  →  [0,1]
                let br = min + (max - min) * (phase + 1.0) * 0.5;
                yield scale_rgb(rgb, br);
            }
        })
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h % 360.0;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    ]
}

fn scale_rgb(rgb: [u8; 3], factor: f32) -> [u8; 3] {
    [
        ((rgb[0] as f32) * factor).clamp(0.0, 255.0).round() as u8,
        ((rgb[1] as f32) * factor).clamp(0.0, 255.0).round() as u8,
        ((rgb[2] as f32) * factor).clamp(0.0, 255.0).round() as u8,
    ]
}
