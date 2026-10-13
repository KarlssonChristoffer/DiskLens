//! Drives available for scanning.

use serde::Serialize;
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
};
use windows_sys::Win32::System::WindowsProgramming::{
    DRIVE_CDROM, DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOTE, DRIVE_REMOVABLE,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    /// Root path such as `C:\`.
    pub root: String,
    pub label: String,
    pub file_system: String,
    pub kind: &'static str,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Total and free bytes of the volume holding `path`.
pub fn space(path: &str) -> Option<(u64, u64)> {
    let z: Vec<u16> = path.encode_utf16().chain([0]).collect();
    let (mut free, mut total) = (0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(z.as_ptr(), std::ptr::null_mut(), &mut total, &mut free) };
    (ok != 0).then_some((total, free))
}

pub fn list() -> Vec<Volume> {
    let mut buffer = vec![0u16; 512];
    let len = unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) } as usize;
    buffer.truncate(len.min(buffer.len()));
    buffer
        .split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .filter_map(volume)
        .collect()
}

fn volume(root: &[u16]) -> Option<Volume> {
    let mut z = root.to_vec();
    z.push(0);
    let kind = match unsafe { GetDriveTypeW(z.as_ptr()) } {
        DRIVE_FIXED => "fixed",
        DRIVE_REMOVABLE => "removable",
        DRIVE_REMOTE => "network",
        DRIVE_CDROM => "optical",
        DRIVE_RAMDISK => "ram",
        _ => return None,
    };

    let (mut free, mut total) = (0u64, 0u64);
    if unsafe { GetDiskFreeSpaceExW(z.as_ptr(), std::ptr::null_mut(), &mut total, &mut free) } == 0 {
        // Empty card readers and optical drives without media.
        return None;
    }

    let mut label = vec![0u16; 261];
    let mut fs = vec![0u16; 261];
    let ok = unsafe {
        GetVolumeInformationW(
            z.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    let text = |v: &[u16]| {
        let end = v.iter().position(|&c| c == 0).unwrap_or(v.len());
        String::from_utf16_lossy(&v[..end])
    };
    Some(Volume {
        root: String::from_utf16_lossy(root),
        label: if ok != 0 { text(&label) } else { String::new() },
        file_system: if ok != 0 { text(&fs) } else { String::new() },
        kind,
        total_bytes: total,
        free_bytes: free,
    })
}
