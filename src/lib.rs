use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MjpegError {
    Io(String),
    FileSizeExceeded,
    FrameCountExceeded,
    FrameSizeExceeded,
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
        
        // まず全ピクセルを白にセット
        for pixel in img.pixels_mut() {
            *pixel = Rgb([255, 255, 255]); // 白背景
        }
        
        // 赤い円を描画
        let center_y = height / 2;
        let radius = 20i32;
        
        for y in 0..height {
            for x in 0..width {
                let dx = x as i32 - circle_x as i32;
                let dy = y as i32 - center_y as i32;
                let distance_squared = dx * dx + dy * dy;
                
                if distance_squared <= (radius * radius) {
                    img.put_pixel(x, y, Rgb([255, 0, 0])); // 赤色
                }
            }
        }
        
        // JPEG形式で低品質でエンコード（カメラっぽく）
        let mut buffer = Vec::new();
        {
            let dynamic_img = DynamicImage::ImageRgb8(img);
            let mut cursor = Cursor::new(&mut buffer);
            // より低い品質でエンコード
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
        
        // テスト用ディレクトリを作成
        let temp_dir = std::path::Path::new("target/test_output");
        std::fs::create_dir_all(&temp_dir).unwrap();
        
        let mut output = Vec::new();
        let cursor = Cursor::new(&mut output);
        
        let mut writer = MjpegWriter::new(cursor, width, height, fps).unwrap();
        
        // 120フレームの動画を作成（赤い円が左から右に移動）
        for frame in 0..frame_count {
            let circle_x = (frame * (width - 40)) / (frame_count - 1);
            let jpeg_data = create_test_jpeg(width, height, circle_x);
            writer.add_frame(&jpeg_data).unwrap();
        }
        
        writer.finish().unwrap();
        
        // 結果を一時ディレクトリに保存
        let output_path = temp_dir.join("test_output.avi");
        std::fs::write(&output_path, &output).unwrap();
        
        // 基本的なサイズチェック
        assert!(output.len() > 1000);
        
        // AVIヘッダーの確認
        assert_eq!(&output[0..4], b"RIFF");
        assert_eq!(&output[8..12], b"AVI ");
        
        println!("Created test video: {:?} ({} bytes)", output_path, output.len());
        println!("Video: {}x{} @ {}fps, {} frames", width, height, fps, frame_count);
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