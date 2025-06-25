use std::io::{IoSlice, SeekFrom};
use crate::{MjpegError, Result};
use crate::common::*;
use crate::writer::AsyncWriter;

#[cfg(any(feature = "async", feature = "tokio"))]
use std::future::Future;

/// A trait for asynchronously writing MJPEG AVI files.
#[cfg(any(feature = "async", feature = "tokio"))]
pub trait MjpegAviWriterAsync {
    /// Asynchronously adds a single JPEG frame to the AVI file.
    ///
    /// The `jpeg_binary` should be a complete JPEG file binary.
    fn add_frame(&mut self, jpeg_binary: &[u8]) -> impl Future<Output = Result<()>> + Send;

    /// Asynchronously adds a single JPEG frame to the AVI file from a slice of buffers.
    ///
    /// This method is more efficient than `add_frame` when the JPEG data is already
    /// in multiple chunks, as it avoids copying them into a single buffer.
    fn add_frame_vectored<'a, 'b>(&'a mut self, bufs: &'b [&'b [u8]]) -> impl Future<Output = Result<()>> + Send;

    /// Asynchronously finalizes the AVI file.
    ///
    /// This method writes the index chunk and updates the AVI header with the final
    /// file size and frame count. It must be called after all frames have been added.
    fn finish(&mut self) -> impl Future<Output = Result<()>> + Send;
}

/// An asynchronous writer for creating MJPEG AVI files.
///
/// This struct implements the `MjpegAviWriterAsync` trait and provides a high-level
/// interface for creating AVI files asynchronously.
#[must_use = "The writer must be finalized using .finish() to produce a valid AVI file"]
#[cfg(any(feature = "async", feature = "tokio"))]
pub struct MjpegAsyncWriter<W: AsyncWriter> {
    writer: W,
    frame_sizes: Vec<u32>,
    jpeg_total_size: u64,
    estimated_file_size: u64,
}

#[cfg(any(feature = "async", feature = "tokio"))]
impl<W: AsyncWriter> MjpegAsyncWriter<W> {
    /// Creates a new `MjpegAsyncWriter`.
    ///
    /// It asynchronously writes the AVI header to the provided writer.
    ///
    /// # Arguments
    ///
    /// * `writer` - The asynchronous writer to write the AVI file to.
    /// * `width` - The width of the video frames.
    /// * `height` - The height of the video frames.
    /// * `fps` - The frames per second of the video.
    pub async fn new(mut writer: W, width: u32, height: u32, fps: u32) -> Result<Self> {
        if fps == 0 {
            return Err(MjpegError::InvalidFrameSize);
        }

        let header = create_header_template(fps, width, height);
        writer.write_all(&header).await?;

        Ok(MjpegAsyncWriter {
            writer,
            frame_sizes: Vec::new(),
            jpeg_total_size: 0,
            estimated_file_size: 256,
        })
    }
}

// No need for new_tokio - regular new() works directly with tokio::fs::File!

#[cfg(feature = "async")]
impl MjpegAsyncWriter<futures::io::Cursor<Vec<u8>>> {
    /// Creates a new `MjpegAsyncWriter` with an in-memory cursor.
    pub async fn new_cursor(width: u32, height: u32, fps: u32) -> Result<Self> {
        let cursor = futures::io::Cursor::new(Vec::new());
        Self::new(cursor, width, height, fps).await
    }
}

#[cfg(any(feature = "async", feature = "tokio"))]
impl<W: AsyncWriter> MjpegAsyncWriter<W> {    
    fn check_limits(&self, frame_size: usize) -> Result<()> {
        if self.frame_sizes.len() >= MAX_FRAME_COUNT as usize {
            return Err(MjpegError::FrameCountExceeded);
        }
        
        if frame_size > u32::MAX as usize {
            return Err(MjpegError::FrameSizeExceeded);
        }
        
        let padded_size = if frame_size % 2 == 1 { frame_size + 1 } else { frame_size };
        let new_frame_data_size = 8 + padded_size;
        let new_index_size = 16;
        let estimated_new_size = self.estimated_file_size + new_frame_data_size as u64 + new_index_size;
        
        if estimated_new_size > MAX_AVI_FILE_SIZE {
            return Err(MjpegError::FileSizeExceeded);
        }
        
        Ok(())
    }
}

#[cfg(any(feature = "async", feature = "tokio"))]
impl<W: AsyncWriter> MjpegAviWriterAsync for MjpegAsyncWriter<W> {
    async fn add_frame(&mut self, jpeg_binary: &[u8]) -> Result<()> {
        self.add_frame_vectored(&[jpeg_binary]).await
    }

    async fn add_frame_vectored<'a, 'b>(&'a mut self, bufs: &'b [&'b [u8]]) -> Result<()> {
        let frame_size: usize = bufs.iter().map(|s| s.len()).sum();
        if frame_size == 0 {
            return Err(MjpegError::InvalidFrameSize);
        }

        self.check_limits(frame_size)?;

        let odd = frame_size % 2 == 1;
        let padded_size = if odd { frame_size + 1 } else { frame_size };
        let padded_size_u32 = padded_size as u32;

        let chunk_header = create_frame_chunk_header(padded_size_u32);

        let mut bufs_to_write = Vec::with_capacity(bufs.len() + 2);
        bufs_to_write.push(IoSlice::new(&chunk_header));
        for buf in bufs {
            bufs_to_write.push(IoSlice::new(buf));
        }
        let padding_byte = [0u8];
        if odd {
            bufs_to_write.push(IoSlice::new(&padding_byte));
        }

        self.writer.write_all_vectored(&bufs_to_write).await?;

        self.frame_sizes.push(padded_size_u32);
        self.jpeg_total_size += padded_size as u64;
        self.estimated_file_size += 8 + padded_size as u64 + 16;

        Ok(())
    }

    async fn finish(&mut self) -> Result<()> {
        let frame_count = self.frame_sizes.len();
        
        let file_sizes = calculate_file_sizes(&self.frame_sizes, self.jpeg_total_size)?;
        
        let idx_header = create_idx_header(file_sizes.index_size);
        self.writer.write_all(&idx_header).await?;
        
        let mut offset = 4u32;
        for &size in &self.frame_sizes {
            let entry = create_index_entry(offset, size);
            self.writer.write_all(&entry).await?;
            
            offset = offset.checked_add(8)
                .and_then(|o| o.checked_add(size))
                .ok_or(MjpegError::FileSizeExceeded)?;
        }

        let frame_count_u32 = frame_count as u32;
        
        let sizes = [
            (4, file_sizes.total_file_size.to_le_bytes()),
            (48, frame_count_u32.to_le_bytes()),
            (140, frame_count_u32.to_le_bytes()),
            (240, frame_count_u32.to_le_bytes()),
            (248, file_sizes.movi_size.to_le_bytes()),
        ];
        
        for (pos, bytes) in sizes {
            self.writer.seek(SeekFrom::Start(pos)).await?;
            self.writer.write_all(&bytes).await?;
        }
        
        Ok(())
    }
}