# CLAUDE.md
必ず日本語で回答してください。
This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust library for creating MJPEG AVI files, with a Python reference implementation. The project is based on [mjpeg-avi](https://github.com/Ricardicus/mjpeg-avi/blob/master/src/avi.c) and aims to port the functionality to Rust.

## ゴミファイルについて

作業中に一時的に作られるファイルは、 tmp ディレクトリを作成し、その下に格納してください。 Git を無駄に汚さないこと。

## Development Commands

- `cargo build` - Build the project
- `cargo test` - Run tests
- `cargo check` - Quick syntax/type checking
- `cargo clippy` - Run linter
- `cargo fmt` - Format code

## Architecture

The codebase consists of:

- **src/lib.rs**: Main Rust library with traits and structures for MJPEG AVI writing
  - `MjpegAviWriter` trait for synchronous writing
  - `MjpegAviWriterAsync` trait for asynchronous writing
  - `NoName` struct (placeholder name) that implements the writer functionality
- **mjpeg.py**: Reference Python implementation that works and should guide the Rust implementation
  - Contains complete AVI header creation logic
  - Shows the binary format structure needed for AVI files
  - Implements frame writing and index generation

## Key Implementation Details

The Python reference shows the AVI file structure requires:
- RIFF/AVI headers with specific byte layouts
- MJPEG frame data prefixed with '00dc' chunk headers
- Index table ('idx1') for frame offsets
- Multiple size placeholders that need patching after writing all frames

The Rust implementation should follow the same binary format patterns shown in the Python code, particularly the header creation logic in `create_header_data()` and frame writing in `add_frame()`.

## Current Status

The Rust code has trait definitions and basic structure but lacks the actual AVI format implementation. The Python code serves as the working reference for the binary format requirements.