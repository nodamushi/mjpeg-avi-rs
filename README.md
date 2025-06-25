# mjpeg-avi-rs

A simple, dependency-free Rust library for creating MJPEG AVI files from a sequence of JPEG frames. It's designed for ease of use and efficiency, supporting both synchronous and asynchronous I/O.

## Features

*   **Simple API:** Create AVI files with just a few lines of code.
*   **Synchronous & Asynchronous:** Supports both blocking and non-blocking I/O via feature flags.
*   **Efficient Writes:** Uses vectored writes (`add_frame_vectored`) to minimize system calls when frame data is in multiple chunks.
*   **No required dependencies:** The core synchronous functionality only uses the standard library.
*   **Async Support:** Integrates with `tokio` and `futures` for async operations.

## Installation

To use this library in another local project, add it to your `Cargo.toml` using a path dependency:

```toml
[dependencies]
mjpeg-avi-rs = { path = "/path/to/mjpeg-avi-rs" }
```

For asynchronous support, enable the corresponding feature flag:

```toml
[dependencies]
mjpeg-avi-rs = { path = "/path/to/mjpeg-avi-rs", features = ["tokio"] }
```

## Usage

### Synchronous Example

```rust,no_run
use mjpeg_avi_rs::{MjpegAviWriter, MjpegWriter};
use std::fs::File;

fn create_sync_video() -> mjpeg_avi_rs::Result<()> {
    let mut file = File::create("output_sync.avi")?;
    let mut writer = MjpegWriter::new(file, 320, 240, 30)?;

    // In a real application, you would get JPEG data from a camera or other source.
    let jpeg_frame_1 = std::fs::read("frame1.jpg").expect("frame1.jpg not found");
    let jpeg_frame_2 = std::fs::read("frame2.jpg").expect("frame2.jpg not found");

    writer.add_frame(&jpeg_frame_1)?;
    writer.add_frame(&jpeg_frame_2)?;

    // You can also use vectored writes for efficiency if your data is in chunks
    let part1 = &jpeg_frame_2[..jpeg_frame_2.len() / 2];
    let part2 = &jpeg_frame_2[jpeg_frame_2.len() / 2..];
    writer.add_frame_vectored(&[part1, part2])?;

    writer.finish()?;
    Ok(())
}
```

### Asynchronous Example (with Tokio)

Make sure to enable the `tokio` feature in your `Cargo.toml`.

```rust,no_run
use mjpeg_avi_rs::{MjpegAviWriterAsync, MjpegAsyncWriter};
use tokio::fs::File;

async fn create_async_video() -> mjpeg_avi_rs::Result<()> {
    let file = File::create("output_async.avi").await?;
    let mut writer = MjpegAsyncWriter::new(file, 320, 240, 30).await?;

    let jpeg_frame_1 = tokio::fs::read("frame1.jpg").await.expect("frame1.jpg not found");
    let jpeg_frame_2 = tokio::fs::read("frame2.jpg").await.expect("frame2.jpg not found");

    writer.add_frame(&jpeg_frame_1).await?;
    writer.add_frame(&jpeg_frame_2).await?;

    writer.finish().await?;
    Ok(())
}
```

## Feature Flags

-   `default`: No features are enabled by default, providing only the synchronous API.
-   `async`: Enables the `futures`-based asynchronous API (`MjpegAsyncWriter`).
-   `tokio`: Enables `tokio`-specific integrations for the asynchronous API.