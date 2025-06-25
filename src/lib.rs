//! A Rust library for creating MJPEG AVI files.
//!
//! This library provides a simple interface for creating AVI files from a sequence of JPEG frames.
//! It supports both synchronous and asynchronous writers.
//!
//! # Examples
//!
//! ```no_run
//! use mjpeg_avi_rs::{MjpegAviWriter, MjpegWriter};
//! use std::fs::File;
//!
//! fn main() -> mjpeg_avi_rs::Result<()> {
//!     let mut file = File::create("output.avi")?;
//!     let mut writer = MjpegWriter::new(file, 320, 240, 30)?;
//!
//!     // Add a single frame
//!     let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0]; // Example JPEG data
//!     writer.add_frame(&jpeg_data)?;
//!
//!     // Add a frame from multiple chunks
//!     let jpeg_part1 = vec![0xFF, 0xD8];
//!     let jpeg_part2 = vec![0xFF, 0xE0];
//!     writer.add_frame_vectored(&[&jpeg_part1, &jpeg_part2])?;
//!
//!     writer.finish()?;
//!     Ok(())
//! }
//! ```

use std::fmt;

/// The error type for MJPEG AVI operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MjpegError {
    /// An I/O error occurred.
    Io(String),
    /// The AVI file size limit (2GB) was exceeded.
    FileSizeExceeded,
    /// The frame count limit was exceeded.
    FrameCountExceeded,
    /// A single frame's size exceeds the `u32` limit.
    FrameSizeExceeded,
    /// The provided frame data is invalid (e.g., empty).
    InvalidFrameSize,
}

impl fmt::Display for MjpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MjpegError::Io(msg) => write!(f, "IO error: {}", msg),
            MjpegError::FileSizeExceeded => write!(f, "AVI file size limit exceeded (2GB)"),
            MjpegError::FrameCountExceeded => write!(f, "Frame count limit exceeded"),
            MjpegError::FrameSizeExceeded => write!(f, "Frame size exceeds u32 limit"),
            MjpegError::InvalidFrameSize => write!(f, "Invalid frame size"),
        }
    }
}

impl From<std::io::Error> for MjpegError {
    fn from(err: std::io::Error) -> Self {
        MjpegError::Io(err.to_string())
    }
}

/// A `Result` alias for MJPEG AVI operations.
pub type Result<T> = core::result::Result<T, MjpegError>;

mod common;
mod writer;
mod mjpeg_sync;

#[cfg(any(feature = "async", feature = "tokio"))]
mod mjpeg_async;

// Re-export public API
pub use writer::{Writer};
pub use mjpeg_sync::{MjpegAviWriter, MjpegWriter};

#[cfg(any(feature = "async", feature = "tokio"))]
pub use writer::AsyncWriter;
#[cfg(any(feature = "async", feature = "tokio"))]
pub use mjpeg_async::{MjpegAviWriterAsync, MjpegAsyncWriter};


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_test_jpeg(width: u32, height: u32, circle_x: u32) -> Vec<u8> {
        use image::{ImageBuffer, Rgb, RgbImage, DynamicImage, ImageFormat};
        
        let mut img: RgbImage = ImageBuffer::new(width, height);
        
        // First, set all pixels to white
        for pixel in img.pixels_mut() {
            *pixel = Rgb([255, 255, 255]); // White background
        }
        
        // Draw a red circle
        let center_y = height / 2;
        let radius = 20i32;
        
        for y in 0..height {
            for x in 0..width {
                let dx = x as i32 - circle_x as i32;
                let dy = y as i32 - center_y as i32;
                let distance_squared = dx * dx + dy * dy;
                
                if distance_squared <= (radius * radius) {
                    img.put_pixel(x, y, Rgb([255, 0, 0])); // Red color
                }
            }
        }
        
        // Encode as JPEG with low quality (like a camera)
        let mut buffer = Vec::new();
        {
            let dynamic_img = DynamicImage::ImageRgb8(img);
            let mut cursor = Cursor::new(&mut buffer);
            // Encode with lower quality
            dynamic_img.write_to(&mut cursor, ImageFormat::Jpeg).unwrap();
        }
        
        buffer
    }

    #[test]
    fn test_moving_circle_video() {
        let width = 320;
        let height = 240;
        let fps = 30;
        let frame_count = 120;
        
        // Create a test directory
        let temp_dir = std::path::Path::new("target/test_output");
        std::fs::create_dir_all(&temp_dir).unwrap();
        
        let mut output = Vec::new();
        let cursor = Cursor::new(&mut output);
        
        let mut writer = MjpegWriter::new(cursor, width, height, fps).unwrap();
        
        // Create a 120-frame video (red circle moving from left to right)
        for frame in 0..frame_count {
            let circle_x = (frame * (width - 40)) / (frame_count - 1);
            let jpeg_data = create_test_jpeg(width, height, circle_x);
            writer.add_frame(&jpeg_data).unwrap();
        }
        
        writer.finish().unwrap();
        
        // Save the result to a temporary directory
        let output_path = temp_dir.join("test_output.avi");
        std::fs::write(&output_path, &output).unwrap();
        
        // Basic size check
        assert!(output.len() > 1000);
        
        // Check AVI header
        assert_eq!(&output[0..4], b"RIFF");
        assert_eq!(&output[8..12], b"AVI ");
        
        println!("Created test video: {:?} ({} bytes)", output_path, output.len());
        println!("Video: {}x{} @ {}fps, {} frames", width, height, fps, frame_count);
    }
    
    #[test]
    fn test_vectored_write() {
        let width = 320;
        let height = 240;
        let fps = 30;

        let jpeg_data = create_test_jpeg(width, height, 100);
        let (part1, part2) = jpeg_data.split_at(jpeg_data.len() / 2);

        // Vectored write
        let mut output_vectored = Vec::new();
        let cursor_vectored = Cursor::new(&mut output_vectored);
        let mut writer_vectored = MjpegWriter::new(cursor_vectored, width, height, fps).unwrap();
        writer_vectored.add_frame_vectored(&[part1, part2]).unwrap();
        writer_vectored.finish().unwrap();

        // Single write
        let mut output_single = Vec::new();
        let cursor_single = Cursor::new(&mut output_single);
        let mut writer_single = MjpegWriter::new(cursor_single, width, height, fps).unwrap();
        writer_single.add_frame(&jpeg_data).unwrap();
        writer_single.finish().unwrap();

        assert_eq!(output_vectored, output_single);
    }

    #[test]
    fn test_empty_frame_error() {
        let mut output = Vec::new();
        let cursor = Cursor::new(&mut output);
        let mut writer = MjpegWriter::new(cursor, 320, 240, 30).unwrap();
        
        let result = writer.add_frame(&[]);
        assert!(matches!(result, Err(MjpegError::InvalidFrameSize)));
    }
    
    #[test]
    fn test_zero_fps_error() {
        let mut output = Vec::new();
        let cursor = Cursor::new(&mut output);
        let result = MjpegWriter::new(cursor, 320, 240, 0);
        assert!(matches!(result, Err(MjpegError::InvalidFrameSize)));
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_sync_compatibility() {
        use futures_executor::block_on;
        use futures::io::Cursor as AsyncCursor;
        
        let width = 320;
        let height = 240;
        let fps = 30;
        
        // Create test JPEG
        let jpeg_data = create_test_jpeg(width, height, 100);
        
        // Sync version
        let mut sync_output = Vec::new();
        let sync_cursor = Cursor::new(&mut sync_output);
        let mut sync_writer = MjpegWriter::new(sync_cursor, width, height, fps).unwrap();
        sync_writer.add_frame(&jpeg_data).unwrap();
        sync_writer.finish().unwrap();
        
        // Async version
        let async_output = block_on(async {
            let mut output = Vec::new();
            let async_cursor = AsyncCursor::new(&mut output);
            let mut async_writer = MjpegAsyncWriter::new(async_cursor, width, height, fps).await.unwrap();
            async_writer.add_frame(&jpeg_data).await.unwrap();
            async_writer.finish().await.unwrap();
            output
        });
        
        // Save async output to file for inspection
        let temp_dir = std::path::Path::new("target/test_output");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let async_output_path = temp_dir.join("async_compatibility_test.avi");
        std::fs::write(&async_output_path, &async_output).unwrap();
        
        // Verify outputs are identical
        assert_eq!(sync_output, async_output);
        assert!(sync_output.len() > 1000);
        
        // Verify AVI headers are identical
        assert_eq!(&sync_output[0..4], b"RIFF");
        assert_eq!(&async_output[0..4], b"RIFF");
        assert_eq!(&sync_output[8..12], b"AVI ");
        assert_eq!(&async_output[8..12], b"AVI ");
    }
}