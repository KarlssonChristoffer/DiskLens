//! Directory enumeration through `NtQueryDirectoryFile`.
//!
//! This is the same route WinDirStat's basic finder takes: one call returns a
//! large batch of entries with both the end-of-file size and the allocation
//! size, so no per-file metadata call is needed.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::{offset_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Wdk::Storage::FileSystem::{
    FILE_FULL_DIR_INFORMATION, FileFullDirectoryInformation, NtQueryDirectoryFile,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE, NO_ERROR, NTSTATUS,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, GetCompressedFileSizeW, GetDiskFreeSpaceW,
    GetFileAttributesW, INVALID_FILE_SIZE, OPEN_EXISTING, SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

const STATUS_NO_MORE_FILES: NTSTATUS = 0x8000_0006_u32 as i32;

/// 1 MiB per thread; large directories then need only a few calls.
const BUFFER_WORDS: usize = 128 * 1024;

thread_local! {
    static BUFFER: RefCell<Vec<u64>> = RefCell::new(vec![0; BUFFER_WORDS]);
}

/// One directory entry as reported by the file system.
pub struct RawEntry<'a> {
    pub name: &'a [u16],
    pub attributes: u32,
    /// Reparse tag when `attributes` has FILE_ATTRIBUTE_REPARSE_POINT.
    pub reparse_tag: u32,
    pub end_of_file: u64,
    pub allocation_size: u64,
    pub last_write: i64,
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn wide_z(path: &[u16]) -> Vec<u16> {
    let mut z = Vec::with_capacity(path.len() + 1);
    z.extend_from_slice(path);
    z.push(0);
    z
}

/// List one directory. `path` is an extended-length (`\\?\`) path without a
/// terminating NUL. Returns the Win32 error code when the directory cannot be
/// opened or read.
pub fn read_dir(path: &[u16], mut on_entry: impl FnMut(RawEntry<'_>)) -> Result<(), u32> {
    let z = wide_z(path);
    let handle = unsafe {
        CreateFileW(
            z.as_ptr(),
            FILE_LIST_DIRECTORY | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(unsafe { GetLastError() });
    }
    let handle = Handle(handle);

    BUFFER.with_borrow_mut(|buffer| {
        let mut restart = true;
        loop {
            let mut iosb: IO_STATUS_BLOCK = unsafe { zeroed() };
            let status = unsafe {
                NtQueryDirectoryFile(
                    handle.0,
                    null_mut(),
                    None,
                    null(),
                    &mut iosb,
                    buffer.as_mut_ptr().cast::<c_void>(),
                    (buffer.len() * size_of::<u64>()) as u32,
                    FileFullDirectoryInformation,
                    false,
                    null(),
                    restart,
                )
            };
            restart = false;
            if status == STATUS_NO_MORE_FILES {
                return Ok(());
            }
            if status < 0 {
                // Any other failure status ends the listing; report it as a
                // generic read failure (ERROR_READ_FAULT).
                return Err(30);
            }
            if iosb.Information == 0 {
                return Ok(());
            }

            let base = buffer.as_ptr().cast::<u8>();
            let mut offset = 0usize;
            loop {
                // SAFETY: the kernel wrote a chain of FILE_FULL_DIR_INFORMATION
                // records into `buffer`; every record is 8-byte aligned and
                // NextEntryOffset stays within the bytes it reported.
                let (info, name) = unsafe {
                    let record = base.add(offset);
                    let info = &*record.cast::<FILE_FULL_DIR_INFORMATION>();
                    let name_ptr = record
                        .add(offset_of!(FILE_FULL_DIR_INFORMATION, FileName))
                        .cast::<u16>();
                    let mut len = info.FileNameLength as usize / 2;
                    let name = std::slice::from_raw_parts(name_ptr, len);
                    // Some file systems append a NUL (WinDirStat handles this too).
                    if len > 1 && name[len - 1] == 0 {
                        len -= 1;
                    }
                    (info, &name[..len])
                };

                let is_dot = name == [b'.' as u16] || name == [b'.' as u16, b'.' as u16];
                if !is_dot {
                    on_entry(RawEntry {
                        name,
                        attributes: info.FileAttributes,
                        // For reparse points EaSize carries the reparse tag.
                        reparse_tag: info.EaSize,
                        end_of_file: info.EndOfFile.max(0) as u64,
                        allocation_size: info.AllocationSize.max(0) as u64,
                        last_write: info.LastWriteTime,
                    });
                }

                if info.NextEntryOffset == 0 {
                    break;
                }
                offset += info.NextEntryOffset as usize;
            }
        }
    })
}

/// On-disk size of a compressed, sparse or cloud file whose directory entry
/// reports an allocation size of zero.
pub fn compressed_size(path: &[u16]) -> Option<u64> {
    let z = wide_z(path);
    let mut high = 0u32;
    let low = unsafe { GetCompressedFileSizeW(z.as_ptr(), &mut high) };
    if low == INVALID_FILE_SIZE && unsafe { GetLastError() } != NO_ERROR {
        return None;
    }
    Some(((high as u64) << 32) | low as u64)
}

pub fn cluster_size(volume_root: &[u16]) -> u64 {
    let z = wide_z(volume_root);
    let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    let ok = unsafe { GetDiskFreeSpaceW(z.as_ptr(), &mut spc, &mut bps, &mut free, &mut total) };
    if ok != 0 { spc as u64 * bps as u64 } else { 4096 }
}

pub fn attributes(path: &[u16]) -> Option<u32> {
    let z = wide_z(path);
    let a = unsafe { GetFileAttributesW(z.as_ptr()) };
    (a != u32::MAX).then_some(a)
}
