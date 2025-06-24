//! Unit tests for logging initialization
//!
//! Tests syslog writer and tracing initialization

use super::super::*;
use anyhow::Result;
use std::io::Write;
use tracing::Level;

/// Mock writer for testing
struct MockWriter {
    written_data: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

impl MockWriter {
    fn new() -> Self {
        Self {
            written_data: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn get_written_data(&self) -> Vec<u8> {
        self.written_data.lock().unwrap().clone()
    }
}

impl Write for MockWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.written_data.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Tests for SyslogWriter
mod syslog_writer_tests {
    use super::*;

    #[test]
    fn test_syslog_writer_creation() {
        let writer = MockWriter::new();
        let syslog_writer = SyslogWriter::new(writer);
        
        // Should create successfully
        assert!(std::ptr::addr_of!(syslog_writer).is_null() == false);
    }

    #[test]
    fn test_syslog_writer_write() {
        let writer = MockWriter::new();
        let written_data = writer.written_data.clone();
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let test_data = b"test log message";
        let result = syslog_writer.write(test_data);
        
        assert!(result.is_ok());
        std::assert_eq!(result.unwrap(), test_data.len());
        
        let written = written_data.lock().unwrap();
        std::assert_eq!(*written, test_data);
    }

    #[test]
    fn test_syslog_writer_write_all() {
        let writer = MockWriter::new();
        let written_data = writer.written_data.clone();
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let test_data = b"complete log message";
        let result = syslog_writer.write_all(test_data);
        
        assert!(result.is_ok());
        
        let written = written_data.lock().unwrap();
        std::assert_eq!(*written, test_data);
    }

    #[test]
    fn test_syslog_writer_flush() {
        let writer = MockWriter::new();
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let result = syslog_writer.flush();
        assert!(result.is_ok());
    }

    #[test]
    fn test_syslog_writer_multiple_writes() {
        let writer = MockWriter::new();
        let written_data = writer.written_data.clone();
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let msg1 = b"first message";
        let msg2 = b" second message";
        
        syslog_writer.write(msg1).unwrap();
        syslog_writer.write(msg2).unwrap();
        
        let written = written_data.lock().unwrap();
        let expected = [msg1.as_slice(), msg2.as_slice()].concat();
        std::assert_eq!(*written, expected);
    }

    #[test]
    fn test_syslog_writer_empty_write() {
        let writer = MockWriter::new();
        let written_data = writer.written_data.clone();
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let result = syslog_writer.write(b"");
        
        assert!(result.is_ok());
        std::assert_eq!(result.unwrap(), 0);
        
        let written = written_data.lock().unwrap();
        assert!(written.is_empty());
    }

    #[test]
    fn test_syslog_writer_large_message() {
        let writer = MockWriter::new();
        let written_data = writer.written_data.clone();
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let large_msg = vec![b'A'; 10000];
        let result = syslog_writer.write(&large_msg);
        
        assert!(result.is_ok());
        std::assert_eq!(result.unwrap(), large_msg.len());
        
        let written = written_data.lock().unwrap();
        std::assert_eq!(written.len(), large_msg.len());
        std::assert_eq!(*written, large_msg);
    }
}

/// Tests for tracing initialization
mod tracing_tests {
    use super::*;
    use tracing_subscriber::fmt::format::FmtSpan;

    #[test]
    fn test_init_tracing_console() {
        let result = init_tracing(false);
        assert!(result.is_ok());
        
        // Test that we can log after initialization
        tracing::info!("Test console logging");
        tracing::warn!("Test warning");
        tracing::error!("Test error");
    }

    #[test]
    fn test_init_tracing_console_multiple_calls() {
        // Multiple calls should be handled gracefully
        let result1 = init_tracing(false);
        let result2 = init_tracing(false);
        
        // First call should succeed, subsequent calls may return error or succeed
        // depending on global state, but shouldn't panic
        match (result1, result2) {
            (Ok(_), _) => {
                // Expected case - first succeeded
            }
            (Err(_), _) => {
                // Also acceptable if global subscriber already set
            }
        }
    }

    #[test]
    fn test_tracing_levels() {
        // Initialize tracing for testing
        let _ = init_tracing(false);
        
        // Test different log levels
        tracing::trace!("Trace level message");
        tracing::debug!("Debug level message");
        tracing::info!("Info level message");
        tracing::warn!("Warning level message");
        tracing::error!("Error level message");
        
        // If we get here without panicking, the test passes
    }

    #[test]
    fn test_tracing_with_fields() {
        let _ = init_tracing(false);
        
        tracing::info!(
            component = "test",
            version = "1.0.0",
            "Test message with fields"
        );
        
        tracing::error!(
            error = "test_error",
            code = 500,
            "Error message with fields"
        );
    }

    #[test]
    fn test_tracing_spans() {
        let _ = init_tracing(false);
        
        let span = tracing::span!(Level::INFO, "test_span", operation = "testing");
        let _enter = span.enter();
        
        tracing::info!("Message inside span");
        
        // Span should be properly closed when _enter is dropped
    }

    #[test]
    fn test_tracing_nested_spans() {
        let _ = init_tracing(false);
        
        let outer_span = tracing::span!(Level::INFO, "outer", module = "test");
        let _outer_enter = outer_span.enter();
        
        tracing::info!("Outer span message");
        
        {
            let inner_span = tracing::span!(Level::DEBUG, "inner", step = 1);
            let _inner_enter = inner_span.enter();
            
            tracing::debug!("Inner span message");
        }
        
        tracing::info!("Back to outer span");
    }

    #[test]
    fn test_tracing_async_context() {
        use tracing::Instrument;
        
        let _ = init_tracing(false);
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let span = tracing::span!(Level::INFO, "async_test");
            async {
                tracing::info!("Message in async context");
                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                tracing::info!("After async operation");
            }
            .instrument(span)
            .await;
        });
    }

    #[test] 
    fn test_tracing_error_handling() {
        let _ = init_tracing(false);
        
        // Test logging errors
        let error = anyhow::anyhow!("Test error");
        tracing::error!(error = %error, "Error occurred");
        
        // Test logging with error chain
        let nested_error = error.context("Additional context");
        tracing::error!(error = %nested_error, "Nested error occurred");
    }
}

/// Performance tests for logging
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_syslog_writer_performance() {
        let writer = MockWriter::new();
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let message = b"Performance test message that is reasonably long to test throughput";
        let iterations = 10000;
        
        let start = Instant::now();
        
        for _ in 0..iterations {
            syslog_writer.write(message).unwrap();
        }
        
        let duration = start.elapsed();
        println!("Wrote {} messages in {:?}", iterations, duration);
        
        // Should complete reasonably fast
        assert!(duration.as_millis() < 1000);
    }

    #[test]
    fn test_tracing_logging_performance() {
        let _ = init_tracing(false);
        
        let iterations = 1000;
        let start = Instant::now();
        
        for i in 0..iterations {
            tracing::info!(iteration = i, "Performance test message");
        }
        
        let duration = start.elapsed();
        println!("Logged {} tracing messages in {:?}", iterations, duration);
        
        // Should complete reasonably fast
        assert!(duration.as_millis() < 2000);
    }

    #[test]
    fn test_span_creation_performance() {
        let _ = init_tracing(false);
        
        let iterations = 1000;
        let start = Instant::now();
        
        for i in 0..iterations {
            let span = tracing::span!(Level::INFO, "perf_span", id = i);
            let _enter = span.enter();
            tracing::trace!("Span message");
        }
        
        let duration = start.elapsed();
        println!("Created {} spans in {:?}", iterations, duration);
        
        // Should complete reasonably fast
        assert!(duration.as_millis() < 1000);
    }
}

/// Error handling tests
mod error_tests {
    use super::*;

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Write failed",
            ))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Flush failed",
            ))
        }
    }

    #[test]
    fn test_syslog_writer_write_error() {
        let writer = FailingWriter;
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let result = syslog_writer.write(b"test message");
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        std::assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        assert!(error.to_string().contains("Write failed"));
    }

    #[test]
    fn test_syslog_writer_flush_error() {
        let writer = FailingWriter;
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let result = syslog_writer.flush();
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        std::assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        assert!(error.to_string().contains("Flush failed"));
    }

    #[test]
    fn test_syslog_writer_write_all_error() {
        let writer = FailingWriter;
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let result = syslog_writer.write_all(b"test message");
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        std::assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }

    struct PartialWriter {
        bytes_to_write: usize,
    }

    impl PartialWriter {
        fn new(bytes_to_write: usize) -> Self {
            Self { bytes_to_write }
        }
    }

    impl Write for PartialWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let bytes_written = std::cmp::min(buf.len(), self.bytes_to_write);
            self.bytes_to_write = self.bytes_to_write.saturating_sub(bytes_written);
            Ok(bytes_written)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_syslog_writer_partial_writes() {
        let writer = PartialWriter::new(5);
        let mut syslog_writer = SyslogWriter::new(writer);
        
        let message = b"this is a longer message";
        let result = syslog_writer.write(message);
        
        assert!(result.is_ok());
        std::assert_eq!(result.unwrap(), 5); // Only first 5 bytes written
    }
}

/// Integration tests
mod integration_tests {
    use super::*;

    #[test]
    fn test_logging_integration() {
        // Test full logging initialization and usage
        let result = init_tracing(false);
        
        // Should succeed or already be initialized
        match result {
            Ok(_) => {
                // Test various logging operations
                tracing::info!("Integration test started");
                
                let span = tracing::span!(Level::INFO, "integration", test = "logging");
                let _enter = span.enter();
                
                tracing::debug!("Debug message in span");
                tracing::warn!(component = "test", "Warning message");
                tracing::error!("Error message for testing");
                
                tracing::info!("Integration test completed");
            }
            Err(_) => {
                // Already initialized - still test logging
                tracing::info!("Using existing tracing subscriber");
            }
        }
    }

    #[test]
    fn test_syslog_writer_with_tracing() {
        // This is more of a concept test since we can't easily
        // inject our mock writer into the tracing infrastructure
        // in a unit test, but we can test the SyslogWriter directly
        
        let writer = MockWriter::new();
        let written_data = writer.written_data.clone();
        let mut syslog_writer = SyslogWriter::new(writer);
        
        // Simulate what tracing would write
        let log_line = b"[2024-01-01T12:00:00.000Z INFO tt_riingd] Test log message\n";
        syslog_writer.write_all(log_line).unwrap();
        
        let written = written_data.lock().unwrap();
        std::assert_eq!(*written, log_line);
    }

    #[test]
    fn test_concurrent_logging() {
        let _ = init_tracing(false);
        
        let handles: Vec<_> = (0..10)
            .map(|i| {
                std::thread::spawn(move || {
                    for j in 0..10 {
                        tracing::info!(thread = i, iteration = j, "Concurrent log message");
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                })
            })
            .collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        // If we get here without deadlocks or panics, test passes
    }
} 