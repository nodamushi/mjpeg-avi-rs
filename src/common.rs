use std::mem::MaybeUninit;
use crate::{MjpegError, Result};

pub(crate) const MAX_AVI_FILE_SIZE: u64 = 2_147_483_648 - 1; // 2GB - 1 (AVI RIFF limit)
pub(crate) const MAX_FRAME_COUNT: u32 = 1_000_000; // 実用的な上限

/// File size calculation results
#[derive(Debug)]
pub(crate) struct FileSizes {
    pub(crate) total_file_size: u32,
    pub(crate) movi_size: u32,
    pub(crate) index_size: u32,
}

/// Creates a frame chunk header (8 bytes: "00dc" + size)
pub(crate) fn create_frame_chunk_header(size: u32) -> [u8; 8] {
    let mut chunk = MaybeUninit::<[u8; 8]>::uninit();
    let chunk_ptr = chunk.as_mut_ptr() as *mut u8;
    
    unsafe {
        chunk_ptr.copy_from_nonoverlapping(b"00dc".as_ptr(), 4);
        chunk_ptr.add(4).copy_from_nonoverlapping(size.to_le_bytes().as_ptr(), 4);
        chunk.assume_init()
    }
}

/// Creates an index entry (16 bytes: fourcc + flags + offset + size)
pub(crate) fn create_index_entry(offset: u32, size: u32) -> [u8; 16] {
    let mut entry = MaybeUninit::<[u8; 16]>::uninit();
    let entry_ptr = entry.as_mut_ptr() as *mut u8;
    
    unsafe {
        entry_ptr.copy_from_nonoverlapping(b"00dc".as_ptr(), 4);
        entry_ptr.add(4).copy_from_nonoverlapping(0x10u32.to_le_bytes().as_ptr(), 4);
        entry_ptr.add(8).copy_from_nonoverlapping(offset.to_le_bytes().as_ptr(), 4);
        entry_ptr.add(12).copy_from_nonoverlapping(size.to_le_bytes().as_ptr(), 4);
        entry.assume_init()
    }
}

/// Creates idx1 chunk header (8 bytes: "idx1" + size)
pub(crate) fn create_idx_header(index_size: u32) -> [u8; 8] {
    let mut header = MaybeUninit::<[u8; 8]>::uninit();
    let header_ptr = header.as_mut_ptr() as *mut u8;
    
    unsafe {
        header_ptr.copy_from_nonoverlapping(b"idx1".as_ptr(), 4);
        header_ptr.add(4).copy_from_nonoverlapping(index_size.to_le_bytes().as_ptr(), 4);
        header.assume_init()
    }
}

/// Calculates final file sizes for AVI format
pub(crate) fn calculate_file_sizes(frame_sizes: &[u32], jpeg_total_size: u64) -> Result<FileSizes> {
    let frame_count = frame_sizes.len();
    
    // フレーム数がu32に収まることを確認
    if frame_count > u32::MAX as usize {
        return Err(MjpegError::FrameCountExceeded);
    }
    let frame_count_u32 = frame_count as u32;
    
    // インデックスサイズがオーバーフローしないかチェック
    let index_size = frame_count_u32.checked_mul(16)
        .ok_or(MjpegError::FileSizeExceeded)?;

    // ファイルサイズ計算（Python版と同じ計算方法）
    let header_size = 256u64;
    let total_file_size = header_size
        .checked_add(jpeg_total_size)
        .and_then(|s| s.checked_add(frame_count as u64 * (8 + 16))) // frame chunks + index entries
        .ok_or(MjpegError::FileSizeExceeded)?;
        
    if total_file_size > u32::MAX as u64 {
        return Err(MjpegError::FileSizeExceeded);
    }
    
    let movi_size = 4u64
        .checked_add(jpeg_total_size)
        .and_then(|s| s.checked_add(frame_count as u64 * 8))
        .ok_or(MjpegError::FileSizeExceeded)?;
        
    if movi_size > u32::MAX as u64 {
        return Err(MjpegError::FileSizeExceeded);
    }

    Ok(FileSizes {
        total_file_size: total_file_size as u32,
        movi_size: movi_size as u32,
        index_size,
    })
}

const AVI_HEADER_TEMPLATE: [u8; 256] = [
    // RIFF header
    b'R', b'I', b'F', b'F',
    0, 0, 0, 0,  // file size placeholder (4-7)
    b'A', b'V', b'I', b' ',
    
    // hdrl LIST
    b'L', b'I', b'S', b'T',
    224, 0, 0, 0,  // hdrl list size
    b'h', b'd', b'r', b'l',
    
    // avih chunk
    b'a', b'v', b'i', b'h',
    56, 0, 0, 0,   // avih size
    0, 0, 0, 0,    // microsec/frame placeholder (32-35)
    88, 27, 0, 0,  // maxbytespersec (7000)
    0, 0, 0, 0,    // paddinggranularity
    16, 0, 0, 0,   // flags (0x10)
    0, 0, 0, 0,    // totalframes placeholder (48-51)
    0, 0, 0, 0,    // initialframes
    1, 0, 0, 0,    // streams
    0, 0, 0, 0,    // suggestedBufferSize
    0, 0, 0, 0,    // width placeholder (64-67)
    0, 0, 0, 0,    // height placeholder (68-71)
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,  // reserved
    
    // strl LIST
    b'L', b'I', b'S', b'T',
    148, 0, 0, 0,  // strl list size
    b's', b't', b'r', b'l',
    
    // strh chunk
    b's', b't', b'r', b'h',
    64, 0, 0, 0,   // strh size
    b'v', b'i', b'd', b's',
    b'M', b'J', b'P', b'G',
    0, 0, 0, 0,    // flags
    0, 0,          // priority
    0, 0,          // language
    0, 0, 0, 0,    // initialframes
    1, 0, 0, 0,    // scale
    0, 0, 0, 0,    // rate placeholder (128-131)
    0, 0, 0, 0,    // start
    0, 0, 0, 0,    // length placeholder (140-143)
    0, 0, 0, 0,    // suggestedBufferSize
    0, 0, 0, 0,    // quality
    0, 0, 0, 0,    // sampleSize
    0, 0, 0, 0,    // left
    0, 0, 0, 0,    // top
    0, 0, 0, 0,    // width placeholder (164-167)
    0, 0, 0, 0,    // height placeholder (168-171)
    
    // strf chunk (bitmap info)
    b's', b't', b'r', b'f',
    40, 0, 0, 0,   // strf size
    40, 0, 0, 0,   // biSize
    0, 0, 0, 0,    // biWidth placeholder (184-187)
    0, 0, 0, 0,    // biHeight placeholder (188-191)
    1, 0,          // biPlanes
    24, 0,         // biBitCount
    b'M', b'J', b'P', b'G',
    0, 0, 0, 0,    // biSizeImage placeholder (200-203)
    0, 0, 0, 0,    // biXPelsPerMeter
    0, 0, 0, 0,    // biYPelsPerMeter
    0, 0, 0, 0,    // biClrUsed
    0, 0, 0, 0,    // biClrImportant

    // odml LIST
    b'L', b'I', b'S', b'T',
    16, 0, 0, 0,   // odml list size
    b'o', b'd', b'm', b'l',
    b'd', b'm', b'l', b'h',
    4, 0, 0, 0,    // dmlh size
    0, 0, 0, 0,    // totalframes placeholder (240-243)

    // movi LIST
    b'L', b'I', b'S', b'T',
    0, 0, 0, 0,    // movi size placeholder (248-251)
    b'm', b'o', b'v', b'i',
];

/// Creates AVI header with dynamic values filled in
pub(crate) fn create_header_template(fps: u32, width: u32, height: u32) -> [u8; 256] {
    let microsec = 1_000_000 / fps;
    let bi_size_image = ((width * 24 / 8 + 3) & 0xFFFFFFFC) * height;
    
    let mut header = AVI_HEADER_TEMPLATE;
    
    // 動的な値のみ更新
    header[32..36].copy_from_slice(&microsec.to_le_bytes());
    header[64..68].copy_from_slice(&width.to_le_bytes());
    header[68..72].copy_from_slice(&height.to_le_bytes());
    header[128..132].copy_from_slice(&fps.to_le_bytes());
    header[164..168].copy_from_slice(&width.to_le_bytes());
    header[168..172].copy_from_slice(&height.to_le_bytes());
    header[184..188].copy_from_slice(&width.to_le_bytes());
    header[188..192].copy_from_slice(&height.to_le_bytes());
    header[200..204].copy_from_slice(&bi_size_image.to_le_bytes());
    
    header
}