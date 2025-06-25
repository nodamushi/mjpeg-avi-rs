use std::io::SeekFrom;
use crate::{MjpegError, Result};
use crate::common::*;
use crate::writer::Writer;

pub trait MjpegAviWriter {
    fn add_frame(&mut self, jpeg_binary: &[u8]) -> Result<()>;
    fn finish(&mut self) -> Result<()>;
}

pub struct MjpegWriter<W: Writer> {
    writer: W,
    frame_sizes: Vec<u32>,
    jpeg_total_size: u64,
    estimated_file_size: u64,
}

impl<W: Writer> MjpegWriter<W> {
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

impl<W: Writer> MjpegAviWriter for MjpegWriter<W> {
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
        let chunk = create_frame_chunk_header(padded_size_u32);
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

        // ファイルサイズ計算
        let file_sizes = calculate_file_sizes(&self.frame_sizes, self.jpeg_total_size)?;

        // idx1 チャンクヘッダーを書き込み
        let idx_header = create_idx_header(file_sizes.index_size);
        self.writer.write_all(&idx_header)?;

        // インデックステーブルを構築
        let mut offset = 4u32;
        for &size in &self.frame_sizes {
            let entry = create_index_entry(offset, size);
            self.writer.write_all(&entry)?;

            offset = offset.checked_add(8)
                .and_then(|o| o.checked_add(size))
                .ok_or(MjpegError::FileSizeExceeded)?;
        }

        let frame_count_u32 = frame_count as u32; // calculate_file_sizes で確認済み

        // サイズ値を一括で書き込み
        let sizes = [
            (4, file_sizes.total_file_size.to_le_bytes()),    // RIFFファイルサイズ
            (48, frame_count_u32.to_le_bytes()),              // totalframes
            (140, frame_count_u32.to_le_bytes()),             // length
            (240, frame_count_u32.to_le_bytes()),             // odml totalframes
            (248, file_sizes.movi_size.to_le_bytes()),        // moviサイズ
        ];

        for (pos, bytes) in sizes {
            self.writer.seek(SeekFrom::Start(pos))?;
            self.writer.write_all(&bytes)?;
        }

        Ok(())
    }
}

fn create_header_data<W: Writer>(writer: &mut W, fps: u32, width: u32, height: u32) -> Result<()> {
    let header = create_header_template(fps, width, height);
    writer.write_all(&header)?;
    Ok(())
}