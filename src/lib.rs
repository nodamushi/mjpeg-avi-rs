use std::io::{Write, Seek, SeekFrom};
use std::fmt;

const MAX_AVI_FILE_SIZE: u64 = 2_147_483_648 - 1; // 2GB - 1 (AVI RIFF limit)
const MAX_FRAME_COUNT: u32 = 1_000_000; // 実用的な上限

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

pub trait MjpegAviWriter {
    fn add_frame(&mut self, jpeg_binary: &[u8]) -> Result<()>;
    fn finish(&mut self) -> Result<()>;
}

pub struct MjpegWriter<W: Write + Seek> {
    writer: W,
    frame_sizes: Vec<u32>,
    jpeg_total_size: u64,
    estimated_file_size: u64,
}

impl<W: Write + Seek> MjpegWriter<W> {
    pub fn new(mut writer: W, width: u32, height: u32, fps: u32) -> Result<Self> {
        if fps == 0 {
            return Err(MjpegError::InvalidFrameSize);
        }
        
        create_header_data(&mut writer, fps, width, height)?;
        
        Ok(MjpegWriter {
            writer,
            frame_sizes: Vec::new(),
            jpeg_total_size: 0,
            estimated_file_size: 256, // ヘッダーサイズ
        })
    }
    
    fn check_limits(&self, frame_size: usize) -> Result<()> {
        // フレーム数制限チェック
        if self.frame_sizes.len() >= MAX_FRAME_COUNT as usize {
            return Err(MjpegError::FrameCountExceeded);
        }
        
        // フレームサイズがu32に収まるかチェック
        if frame_size > u32::MAX as usize {
            return Err(MjpegError::FrameSizeExceeded);
        }
        
        // 推定ファイルサイズチェック
        let padded_size = if frame_size % 2 == 1 { frame_size + 1 } else { frame_size };
        let new_frame_data_size = 8 + padded_size; // chunk header + data
        let new_index_size = 16; // index entry size
        let estimated_new_size = self.estimated_file_size + new_frame_data_size as u64 + new_index_size;
        
        if estimated_new_size > MAX_AVI_FILE_SIZE {
            return Err(MjpegError::FileSizeExceeded);
        }
        
        Ok(())
    }
}

impl<W: Write + Seek> MjpegAviWriter for MjpegWriter<W> {
    fn add_frame(&mut self, jpeg_binary: &[u8]) -> Result<()> {
        if jpeg_binary.is_empty() {
            return Err(MjpegError::InvalidFrameSize);
        }
        
        self.check_limits(jpeg_binary.len())?;
        
        let frame_size = jpeg_binary.len();
        let odd = frame_size % 2 == 1;
        let padded_size = if odd { frame_size + 1 } else { frame_size };
        let padded_size_u32 = padded_size as u32; // check_limits で確認済み
        
        // チャンクヘッダー + データを一括書き込み
        let mut chunk = [0u8; 8];
        chunk[0..4].copy_from_slice(b"00dc");
        chunk[4..8].copy_from_slice(&padded_size_u32.to_le_bytes());
        
        self.writer.write_all(&chunk)?;
        self.writer.write_all(jpeg_binary)?;
        
        if odd {
            self.writer.write_all(&[0u8])?;
        }

        self.frame_sizes.push(padded_size_u32);
        self.jpeg_total_size += padded_size as u64;
        self.estimated_file_size += 8 + padded_size as u64 + 16; // chunk + index entry
        
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        let frame_count = self.frame_sizes.len();
        
        // フレーム数がu32に収まることを確認
        if frame_count > u32::MAX as usize {
            return Err(MjpegError::FrameCountExceeded);
        }
        let frame_count_u32 = frame_count as u32;
        
        // インデックスサイズがオーバーフローしないかチェック
        let index_size = frame_count_u32.checked_mul(16)
            .ok_or(MjpegError::FileSizeExceeded)?;
        
        // idx1 チャンクヘッダーを書き込み
        let mut idx_header = [0u8; 8];
        idx_header[0..4].copy_from_slice(b"idx1");
        idx_header[4..8].copy_from_slice(&index_size.to_le_bytes());
        self.writer.write_all(&idx_header)?;
        
        // インデックステーブルを構築
        let mut offset = 4u32;
        for &size in &self.frame_sizes {
            let mut entry = [0u8; 16];
            entry[0..4].copy_from_slice(b"00dc");
            entry[4..8].copy_from_slice(&0x10u32.to_le_bytes()); // flags
            entry[8..12].copy_from_slice(&offset.to_le_bytes());
            entry[12..16].copy_from_slice(&size.to_le_bytes());
            
            self.writer.write_all(&entry)?;
            
            offset = offset.checked_add(8)
                .and_then(|o| o.checked_add(size))
                .ok_or(MjpegError::FileSizeExceeded)?;
        }

        // ファイルサイズ計算（Python版と同じ計算方法）
        let header_size = 256u64;
        let total_file_size = header_size
            .checked_add(self.jpeg_total_size)
            .and_then(|s| s.checked_add(frame_count as u64 * (8 + 16))) // frame chunks + index entries
            .ok_or(MjpegError::FileSizeExceeded)?;
            
        if total_file_size > u32::MAX as u64 {
            return Err(MjpegError::FileSizeExceeded);
        }
        
        let movi_size = 4u64
            .checked_add(self.jpeg_total_size)
            .and_then(|s| s.checked_add(frame_count as u64 * 8))
            .ok_or(MjpegError::FileSizeExceeded)?;
            
        if movi_size > u32::MAX as u64 {
            return Err(MjpegError::FileSizeExceeded);
        }
        
        // サイズ値を一括で書き込み
        let sizes = [
            (4, (total_file_size as u32).to_le_bytes()),      // RIFFファイルサイズ
            (48, frame_count_u32.to_le_bytes()),              // totalframes
            (140, frame_count_u32.to_le_bytes()),             // length
            (240, frame_count_u32.to_le_bytes()),             // odml totalframes  
            (248, (movi_size as u32).to_le_bytes()),          // moviサイズ
        ];
        
        for (pos, bytes) in sizes {
            self.writer.seek(SeekFrom::Start(pos))?;
            self.writer.write_all(&bytes)?;
        }
        
        Ok(())
    }
}

fn create_header_data<W: Write>(writer: &mut W, fps: u32, width: u32, height: u32) -> Result<()> {
    let microsec = 1_000_000 / fps;
    let bi_size_image = ((width * 24 / 8 + 3) & 0xFFFFFFFC) * height;
    
    let mut header = [0u8; 256];
    let mut pos = 0;
    
    // RIFF header
    header[pos..pos+4].copy_from_slice(b"RIFF"); pos += 4;
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // file size placeholder
    header[pos..pos+4].copy_from_slice(b"AVI "); pos += 4;
    
    // hdrl LIST
    header[pos..pos+4].copy_from_slice(b"LIST"); pos += 4;
    header[pos..pos+4].copy_from_slice(&224u32.to_le_bytes()); pos += 4; // hdrl list size
    header[pos..pos+4].copy_from_slice(b"hdrl"); pos += 4;
    
    // avih chunk
    header[pos..pos+4].copy_from_slice(b"avih"); pos += 4;
    header[pos..pos+4].copy_from_slice(&56u32.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(&microsec.to_le_bytes()); pos += 4; // microsec/frame
    header[pos..pos+4].copy_from_slice(&7000u32.to_le_bytes()); pos += 4; // maxbytespersec
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // paddinggranularity
    header[pos..pos+4].copy_from_slice(&0x10u32.to_le_bytes()); pos += 4; // flags
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // totalframes placeholder
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // initialframes
    header[pos..pos+4].copy_from_slice(&1u32.to_le_bytes()); pos += 4; // streams
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // suggestedBufferSize
    header[pos..pos+4].copy_from_slice(&width.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(&height.to_le_bytes()); pos += 4;
    header[pos..pos+16].fill(0); pos += 16; // reserved
    
    // strl LIST
    header[pos..pos+4].copy_from_slice(b"LIST"); pos += 4;
    header[pos..pos+4].copy_from_slice(&148u32.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(b"strl"); pos += 4;
    
    // strh chunk
    header[pos..pos+4].copy_from_slice(b"strh"); pos += 4;
    header[pos..pos+4].copy_from_slice(&64u32.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(b"vids"); pos += 4;
    header[pos..pos+4].copy_from_slice(b"MJPG"); pos += 4;
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // flags
    header[pos..pos+2].copy_from_slice(&0u16.to_le_bytes()); pos += 2; // priority
    header[pos..pos+2].copy_from_slice(&0u16.to_le_bytes()); pos += 2; // language
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // initialframes
    header[pos..pos+4].copy_from_slice(&1u32.to_le_bytes()); pos += 4; // scale
    header[pos..pos+4].copy_from_slice(&fps.to_le_bytes()); pos += 4; // rate
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // start
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // length placeholder
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // suggestedBufferSize
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // quality
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // sampleSize
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // left
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // top
    header[pos..pos+4].copy_from_slice(&width.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(&height.to_le_bytes()); pos += 4;
    
    // strf chunk (bitmap info)
    header[pos..pos+4].copy_from_slice(b"strf"); pos += 4;
    header[pos..pos+4].copy_from_slice(&40u32.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(&40u32.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(&width.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(&height.to_le_bytes()); pos += 4;
    header[pos..pos+2].copy_from_slice(&1u16.to_le_bytes()); pos += 2;
    header[pos..pos+2].copy_from_slice(&24u16.to_le_bytes()); pos += 2;
    header[pos..pos+4].copy_from_slice(b"MJPG"); pos += 4;
    header[pos..pos+4].copy_from_slice(&bi_size_image.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // biXPelsPerMeter
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // biYPelsPerMeter
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // biClrUsed
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // biClrImportant

    // odml LIST
    header[pos..pos+4].copy_from_slice(b"LIST"); pos += 4;
    header[pos..pos+4].copy_from_slice(&0x10u32.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(b"odml"); pos += 4;
    header[pos..pos+4].copy_from_slice(b"dmlh"); pos += 4;
    header[pos..pos+4].copy_from_slice(&4u32.to_le_bytes()); pos += 4;
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4;

    // movi LIST
    header[pos..pos+4].copy_from_slice(b"LIST"); pos += 4;
    header[pos..pos+4].copy_from_slice(&0u32.to_le_bytes()); pos += 4; // movi size placeholder
    header[pos..pos+4].copy_from_slice(b"movi");
    
    writer.write_all(&header)?;
    Ok(())
}

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
        
        // デバッグ用：最初のフレームの詳細をチェック
        if circle_x == 0 {
            println!("First frame: {}x{}, circle at x={}", width, height, circle_x);
            // 背景色をチェック
            let bg_pixel = img.get_pixel(10, 10);
            println!("Background pixel at (10,10): {:?}", bg_pixel);
            // 円の位置をチェック
            if circle_x < width {
                let circle_pixel = img.get_pixel(circle_x, center_y);
                println!("Circle pixel at ({},{}): {:?}", circle_x, center_y, circle_pixel);
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
        
        // デバッグ用：JPEGファイルの確認
        if circle_x == 0 {
            println!("JPEG buffer size: {}", buffer.len());
            
            // JPEGヘッダーの詳細確認
            println!("JPEG header: {:02x?}", &buffer[0..std::cmp::min(20, buffer.len())]);
            if buffer.len() >= 4 {
                if &buffer[0..2] == &[0xFF, 0xD8] {
                    println!("Valid JPEG SOI marker found");
                } else {
                    println!("WARNING: Invalid JPEG SOI marker: {:02x?}", &buffer[0..2]);
                }
            }
            if buffer.len() >= 2 {
                let end = &buffer[buffer.len()-2..];
                if end == &[0xFF, 0xD9] {
                    println!("Valid JPEG EOI marker found");
                } else {
                    println!("WARNING: Invalid JPEG EOI marker: {:02x?}", end);
                }
            }
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
            
            // 最初と最後の数フレームをデバッグ用に保存
            if frame < 3 || frame >= frame_count - 3 {
                let frame_filename = temp_dir.join(format!("frame_{:03}.jpg", frame));
                std::fs::write(&frame_filename, &jpeg_data).unwrap();
                println!("Saved debug frame: {:?}", frame_filename);
            }
            
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
        println!("Debug frames and video saved in: {:?}", temp_dir);
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
    
}


