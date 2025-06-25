use std::io::{IoSlice, SeekFrom};
use crate::{MjpegError, Result};

#[cfg(any(feature = "async", feature = "tokio"))]
use std::future::Future;

/// A trait for synchronous writers that support `Write` and `Seek` operations.
///
/// This trait is an abstraction over `std::io::Write` and `std::io::Seek`,
/// allowing the AVI writer to work with different output types like files or in-memory buffers.
pub trait Writer {
    /// Writes a buffer into this writer, returning how many bytes were written.
    fn write_all(&mut self, buf: &[u8]) -> Result<()>;

    /// Like `write_all`, but writes from a slice of buffers.
    fn write_all_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<()>;

    /// Seeks to an offset, in bytes, in a stream.
    fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
}

/// A trait for asynchronous writers that support `AsyncWrite` and `AsyncSeek` operations.
///
/// This trait is an abstraction over asynchronous I/O operations, supporting runtimes
/// like `tokio` and `futures`.
#[cfg(any(feature = "async", feature = "tokio"))]
pub trait AsyncWriter: Send {
    /// Asynchronously writes a buffer into this writer.
    fn write_all(&mut self, buf: &[u8]) -> impl Future<Output = Result<()>> + Send;

    /// Asynchronously writes a slice of buffers into this writer.
    fn write_all_vectored<'a, 'b>(&'a mut self, bufs: &'b [IoSlice<'b>]) -> impl Future<Output = Result<()>> + Send;

    /// Asynchronously seeks to an offset, in bytes, in a stream.
    fn seek(&mut self, pos: SeekFrom) -> impl Future<Output = Result<u64>> + Send;
}

// Implement Writer for std::io types
impl<W: std::io::Write + std::io::Seek> Writer for W {
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        std::io::Write::write_all(self, buf).map_err(MjpegError::from)
    }

    fn write_all_vectored(&mut self, bufs: &[IoSlice<'_>]) -> Result<()> {
        std::io::Write::write_vectored(self, bufs)
            .map(|_| ()) // Convert usize to ()
            .map_err(MjpegError::from)
    }

    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        std::io::Seek::seek(self, pos).map_err(MjpegError::from)
    }
}

// Implement AsyncWriter for futures types
#[cfg(feature = "async")]
impl<W: futures::io::AsyncWrite + futures::io::AsyncSeek + Unpin + Send> AsyncWriter for W {
    async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        futures::io::AsyncWriteExt::write_all(self, buf).await.map_err(MjpegError::from)
    }

    async fn write_all_vectored<'a, 'b>(&'a mut self, bufs: &'b [IoSlice<'b>]) -> Result<()> {
        for buf in bufs {
            futures::io::AsyncWriteExt::write_all(self, buf).await?;
        }
        Ok(())
    }

    async fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        futures::io::AsyncSeekExt::seek(self, pos).await.map_err(MjpegError::from)
    }
}

// Direct implementation for tokio::fs::File
#[cfg(feature = "tokio")]
impl AsyncWriter for tokio::fs::File {
    async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        tokio::io::AsyncWriteExt::write_all(self, buf).await.map_err(MjpegError::from)
    }

    async fn write_all_vectored<'a, 'b>(&'a mut self, bufs: &'b [IoSlice<'b>]) -> Result<()> {
        tokio::io::AsyncWriteExt::write_vectored(self, bufs).await.map(|_| ()).map_err(MjpegError::from)
    }

    async fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        tokio::io::AsyncSeekExt::seek(self, pos).await.map_err(MjpegError::from)
    }
}