use std::io::SeekFrom;
use crate::{MjpegError, Result};

#[cfg(any(feature = "async", feature = "tokio"))]
use std::future::Future;

/// Synchronous writer trait abstraction
pub trait Writer {
    fn write_all(&mut self, buf: &[u8]) -> Result<()>;
    fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
}

/// Asynchronous writer trait abstraction  
#[cfg(any(feature = "async", feature = "tokio"))]
pub trait AsyncWriter: Send {
    fn write_all(&mut self, buf: &[u8]) -> impl Future<Output = Result<()>> + Send;
    fn seek(&mut self, pos: SeekFrom) -> impl Future<Output = Result<u64>> + Send;
}

// Implement Writer for std::io types
impl<W: std::io::Write + std::io::Seek> Writer for W {
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        std::io::Write::write_all(self, buf).map_err(MjpegError::from)
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

    async fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        tokio::io::AsyncSeekExt::seek(self, pos).await.map_err(MjpegError::from)
    }
}