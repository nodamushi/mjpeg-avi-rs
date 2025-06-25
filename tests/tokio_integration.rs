#[cfg(any(feature = "async", feature = "tokio"))]
mod tokio_tests {
    use mjpeg_avi_rs::{MjpegAsyncWriter, MjpegAviWriterAsync};
    
    #[cfg(feature = "async")]
    use futures::io::Cursor;
    
    // No more TokioFileWriter import needed!

    fn create_test_jpeg(width: u32, height: u32, circle_x: u32) -> Vec<u8> {
        use image::{ImageBuffer, Rgb, RgbImage, DynamicImage, ImageFormat};
        
        let mut img: RgbImage = ImageBuffer::new(width, height);
        
        // 白背景
        for pixel in img.pixels_mut() {
            *pixel = Rgb([255, 255, 255]);
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
                    img.put_pixel(x, y, Rgb([255, 0, 0]));
                }
            }
        }
        
        let mut buffer = Vec::new();
        {
            let dynamic_img = DynamicImage::ImageRgb8(img);
            let mut cursor = std::io::Cursor::new(&mut buffer);
            dynamic_img.write_to(&mut cursor, ImageFormat::Jpeg).unwrap();
        }
        
        buffer
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_tokio_single_frame() {
        let width = 320;
        let height = 240;
        let fps = 30;
        
        let jpeg_data = create_test_jpeg(width, height, 100);
        
        let mut output = Vec::new();
        let cursor = Cursor::new(&mut output);
        
        let mut writer = MjpegAsyncWriter::new(cursor, width, height, fps).await.unwrap();
        writer.add_frame(&jpeg_data).await.unwrap();
        writer.finish().await.unwrap();
        
        // 基本的なAVIチェック
        assert!(output.len() > 1000);
        assert_eq!(&output[0..4], b"RIFF");
        assert_eq!(&output[8..12], b"AVI ");
        
        println!("Tokio single frame test: {} bytes", output.len());
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_tokio_multiple_frames() {
        let width = 320;
        let height = 240;
        let fps = 30;
        let frame_count = 10;
        
        let mut output = Vec::new();
        let cursor = Cursor::new(&mut output);
        
        let mut writer = MjpegAsyncWriter::new(cursor, width, height, fps).await.unwrap();
        
        for frame in 0..frame_count {
            let circle_x = (frame * (width - 40)) / (frame_count - 1);
            let jpeg_data = create_test_jpeg(width, height, circle_x);
            writer.add_frame(&jpeg_data).await.unwrap();
        }
        
        writer.finish().await.unwrap();
        
        assert!(output.len() > 5000); // 複数フレームでより大きなファイル
        assert_eq!(&output[0..4], b"RIFF");
        assert_eq!(&output[8..12], b"AVI ");
        
        println!("Tokio multi-frame test: {} frames, {} bytes", frame_count, output.len());
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_tokio_concurrent_writers() {
        let width = 160;
        let height = 120;
        let fps = 15;
        
        let jpeg_data = create_test_jpeg(width, height, 50);
        
        // 3つの並行Writer
        let tasks = (0..3).map(|i| {
            let data = jpeg_data.clone();
            tokio::spawn(async move {
                let mut output = Vec::new();
                let cursor = Cursor::new(&mut output);
                
                let mut writer = MjpegAsyncWriter::new(cursor, width, height, fps).await.unwrap();
                writer.add_frame(&data).await.unwrap();
                writer.finish().await.unwrap();
                
                (i, output.len())
            })
        });
        
        let results = futures::future::join_all(tasks).await;
        
        for result in results {
            let (id, size) = result.unwrap();
            assert!(size > 500);
            println!("Concurrent writer {}: {} bytes", id, size);
        }
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_tokio_error_handling() {
        let mut output = Vec::new();
        let cursor = Cursor::new(&mut output);
        
        let mut writer = MjpegAsyncWriter::new(cursor, 320, 240, 30).await.unwrap();
        
        // 空フレームでエラーテスト
        let result = writer.add_frame(&[]).await;
        assert!(result.is_err());
        
        println!("Tokio error handling test passed");
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_tokio_fs_file() {
        use tokio::fs::File;
        
        let width = 320;
        let height = 240;
        let fps = 30;
        let frame_count = 5;
        
        // 一時ファイルパス
        let temp_path = std::path::Path::new("target/test_output/tokio_file_test.avi");
        std::fs::create_dir_all(temp_path.parent().unwrap()).unwrap();
        
        // ファイルが存在する場合は削除
        if temp_path.exists() {
            std::fs::remove_file(temp_path).unwrap();
        }
        
        {
            let file = File::create(temp_path).await.unwrap();
            let mut writer = MjpegAsyncWriter::new(file, width, height, fps).await.unwrap();
            
            for frame in 0..frame_count {
                let circle_x = (frame * (width - 40)) / (frame_count - 1);
                let jpeg_data = create_test_jpeg(width, height, circle_x);
                writer.add_frame(&jpeg_data).await.unwrap();
            }
            
            writer.finish().await.unwrap();
        }
        
        // ファイルが作成されたか確認
        assert!(temp_path.exists());
        
        let file_size = std::fs::metadata(temp_path).unwrap().len();
        assert!(file_size > 5000); // 複数フレームで十分なサイズ
        
        // AVIヘッダーの確認
        let contents = std::fs::read(temp_path).unwrap();
        assert_eq!(&contents[0..4], b"RIFF");
        assert_eq!(&contents[8..12], b"AVI ");
        
        println!("Tokio fs::File test: {} frames, {} bytes written to {:?}", 
                 frame_count, file_size, temp_path);
        
        // Keep file for manual inspection - don't delete it
    }
}