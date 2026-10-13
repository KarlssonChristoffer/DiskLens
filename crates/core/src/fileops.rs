//! File maintenance operations from WinDirStat's Clean Up menu that work on
//! paths directly: hashing, NTFS/WOF compression, VHD compaction, the
//! hibernation file, plus small queries (elevation, owner).
//! Ports of WinDirStat's HelpersTasks.cpp.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_HASH_HANDLE, BCryptCreateHash, BCryptDestroyHash, BCryptFinishHash, BCryptHashData,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, LookupAccountSidW, OWNER_SECURITY_INFORMATION, PSID, SID_NAME_USE,
    TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_SEQUENTIAL_SCAN, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::Storage::Vhd::{
    COMPACT_VIRTUAL_DISK_FLAG_NONE, COMPACT_VIRTUAL_DISK_PARAMETERS, COMPACT_VIRTUAL_DISK_VERSION_1,
    CompactVirtualDisk, OPEN_VIRTUAL_DISK_FLAG_NONE, OPEN_VIRTUAL_DISK_PARAMETERS, OPEN_VIRTUAL_DISK_VERSION_1,
    OpenVirtualDisk, VIRTUAL_DISK_ACCESS_ALL, VIRTUAL_STORAGE_TYPE, VIRTUAL_STORAGE_TYPE_DEVICE_UNKNOWN,
    VIRTUAL_STORAGE_TYPE_VENDOR_MICROSOFT,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Power::{CallNtPowerInformation, SystemReserveHiberFile};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::scan::normalize;

fn extended(path: &str) -> OsString {
    OsString::from_wide(&normalize(path).1)
}

fn wide_z(path: &str) -> Vec<u16> {
    normalize(path).1.into_iter().chain([0]).collect()
}

fn last_error() -> String {
    std::io::Error::last_os_error().to_string()
}

// ------------------------------------------------------------------ hashing

#[derive(Clone, Debug, Serialize)]
pub struct Hashes {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
    pub sha384: String,
    pub sha512: String,
}

/// MD5, SHA-1, SHA-256, SHA-384 and SHA-512 of a file in one pass, through
/// Windows CNG as WinDirStat's Compute Hash does.
pub fn compute_hashes(path: &str) -> Result<Hashes, String> {
    // CNG pseudo-handles (Windows 10+): MD5, SHA1, SHA256, SHA384, SHA512.
    const ALGORITHMS: [(usize, usize); 5] = [(0x21, 16), (0x31, 20), (0x41, 32), (0x51, 48), (0x61, 64)];

    struct Hash(BCRYPT_HASH_HANDLE);
    impl Drop for Hash {
        fn drop(&mut self) {
            unsafe { BCryptDestroyHash(self.0) };
        }
    }

    let mut hashes = Vec::new();
    for (alg, _) in ALGORITHMS {
        let mut h: BCRYPT_HASH_HANDLE = null_mut();
        let status = unsafe { BCryptCreateHash(alg as _, &mut h, null_mut(), 0, null(), 0, 0) };
        if status < 0 {
            return Err(format!("Windows could not start hashing (0x{status:08x})"));
        }
        hashes.push(Hash(h));
    }

    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_SEQUENTIAL_SCAN)
        .open(extended(path))
        .map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        for h in &hashes {
            unsafe { BCryptHashData(h.0, buffer.as_ptr(), read as u32, 0) };
        }
    }

    let mut hex = Vec::new();
    for (h, (_, len)) in hashes.iter().zip(ALGORITHMS) {
        let mut out = vec![0u8; len];
        unsafe { BCryptFinishHash(h.0, out.as_mut_ptr(), len as u32, 0) };
        hex.push(out.iter().map(|b| format!("{b:02x}")).collect::<String>());
    }
    let mut hex = hex.into_iter();
    Ok(Hashes {
        md5: hex.next().unwrap(),
        sha1: hex.next().unwrap(),
        sha256: hex.next().unwrap(),
        sha384: hex.next().unwrap(),
        sha512: hex.next().unwrap(),
    })
}

// --------------------------------------------------------------- compression

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Compression {
    /// Remove NTFS and WOF compression.
    None,
    /// Classic NTFS compression.
    Lznt1,
    /// Windows Overlay Filter compression (Windows 10+, like `compact /exe`).
    Xpress4k,
    Xpress8k,
    Xpress16k,
    Lzx,
}

const FSCTL_SET_COMPRESSION: u32 = 0x0009_C040;
const FSCTL_SET_EXTERNAL_BACKING: u32 = 0x0009_030C;
const FSCTL_DELETE_EXTERNAL_BACKING: u32 = 0x0009_0314;
const COMPRESSION_FORMAT_NONE: u16 = 0;
const COMPRESSION_FORMAT_LZNT1: u16 = 2;

#[repr(C)]
struct WofExternalInfo {
    version: u32,
    provider: u32,
}

#[repr(C)]
struct FileProviderExternalInfoV1 {
    version: u32,
    algorithm: u32,
    flags: u32,
}

/// Compress one file. Folders are compressed by compressing their files.
pub fn compress_file(path: &str, compression: Compression) -> Result<(), String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(extended(path))
        .map_err(|e| e.to_string())?;
    let handle = file.as_raw_handle() as HANDLE;
    let ioctl = |code: u32, input: *const u8, len: usize| -> bool {
        let mut returned = 0u32;
        unsafe { DeviceIoControl(handle, code, input.cast(), len as u32, null_mut(), 0, &mut returned, null_mut()) != 0 }
    };

    let wof_algorithm = match compression {
        Compression::Xpress4k => Some(0),
        Compression::Lzx => Some(1),
        Compression::Xpress8k => Some(2),
        Compression::Xpress16k => Some(3),
        _ => None,
    };
    let ok = if let Some(algorithm) = wof_algorithm {
        #[repr(C)]
        struct Request {
            wof: WofExternalInfo,
            file: FileProviderExternalInfoV1,
        }
        let request = Request {
            wof: WofExternalInfo { version: 1, provider: 2 },
            file: FileProviderExternalInfoV1 { version: 1, algorithm, flags: 0 },
        };
        ioctl(FSCTL_SET_EXTERNAL_BACKING, (&request as *const Request).cast(), size_of::<Request>())
    } else if compression == Compression::Lznt1 {
        let format = COMPRESSION_FORMAT_LZNT1;
        ioctl(FSCTL_SET_COMPRESSION, (&format as *const u16).cast(), 2)
    } else {
        let format = COMPRESSION_FORMAT_NONE;
        let a = ioctl(FSCTL_SET_COMPRESSION, (&format as *const u16).cast(), 2);
        let b = ioctl(FSCTL_DELETE_EXTERNAL_BACKING, null(), 0);
        a || b
    };
    // WOF reports ERROR_COMPRESSION_NOT_BENEFICIAL (344) for incompressible data.
    if ok || unsafe { GetLastError() } == 344 { Ok(()) } else { Err(last_error()) }
}

// ----------------------------------------------------------------- VHD files

/// Compact a dynamically expanding VHD/VHDX file (it must not be attached).
pub fn optimize_vhd(path: &str) -> Result<(), String> {
    let storage = VIRTUAL_STORAGE_TYPE {
        DeviceId: VIRTUAL_STORAGE_TYPE_DEVICE_UNKNOWN,
        VendorId: VIRTUAL_STORAGE_TYPE_VENDOR_MICROSOFT,
    };
    let open = OPEN_VIRTUAL_DISK_PARAMETERS { Version: OPEN_VIRTUAL_DISK_VERSION_1, ..Default::default() };
    let z: Vec<u16> = path.encode_utf16().chain([0]).collect();
    let mut handle: HANDLE = null_mut();
    let err = unsafe {
        OpenVirtualDisk(&storage, z.as_ptr(), VIRTUAL_DISK_ACCESS_ALL, OPEN_VIRTUAL_DISK_FLAG_NONE, &open, &mut handle)
    };
    if err != 0 {
        return Err(std::io::Error::from_raw_os_error(err as i32).to_string());
    }
    let compact = COMPACT_VIRTUAL_DISK_PARAMETERS { Version: COMPACT_VIRTUAL_DISK_VERSION_1, ..Default::default() };
    let err = unsafe { CompactVirtualDisk(handle, COMPACT_VIRTUAL_DISK_FLAG_NONE, &compact, null()) };
    unsafe { CloseHandle(handle) };
    if err == 0 { Ok(()) } else { Err(std::io::Error::from_raw_os_error(err as i32).to_string()) }
}

// ----------------------------------------------------------------- system

/// Turn off hibernation and remove hiberfil.sys (needs administrator rights).
pub fn disable_hibernation() -> Result<(), String> {
    let enabled: u8 = 0;
    let status = unsafe {
        CallNtPowerInformation(SystemReserveHiberFile, (&enabled as *const u8).cast(), 1, null_mut(), 0)
    };
    // The call may leave the file behind; WinDirStat deletes it too.
    if let Ok(drive) = std::env::var("SystemDrive") {
        let _ = std::fs::remove_file(format!(r"{drive}\hiberfil.sys"));
    }
    if status >= 0 { Ok(()) } else { Err(format!("Windows refused (0x{status:08x}); administrator rights are required")) }
}

pub fn hibernation_file_exists() -> bool {
    std::env::var("SystemDrive")
        .map(|d| std::path::Path::new(&format!(r"{d}\hiberfil.sys")).exists())
        .unwrap_or(false)
}

/// Whether this process runs with administrator rights.
pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// The owner of a file or folder as `DOMAIN\name`.
pub fn owner(path: &str) -> Option<String> {
    let z = wide_z(path);
    unsafe {
        let mut sid: PSID = null_mut();
        let mut descriptor = null_mut();
        let err = GetNamedSecurityInfoW(
            z.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut sid,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        );
        if err != 0 {
            return None;
        }
        let mut name = [0u16; 256];
        let mut domain = [0u16; 256];
        let (mut name_len, mut domain_len) = (name.len() as u32, domain.len() as u32);
        let mut use_: SID_NAME_USE = 0;
        let ok = LookupAccountSidW(
            null(),
            sid,
            name.as_mut_ptr(),
            &mut name_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut use_,
        );
        LocalFree(descriptor);
        if ok == 0 {
            return None;
        }
        let name = String::from_utf16_lossy(&name[..name_len as usize]);
        let domain = String::from_utf16_lossy(&domain[..domain_len as usize]);
        Some(if domain.is_empty() { name } else { format!("{domain}\\{name}") })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_known_values() {
        let path = std::env::temp_dir().join(format!("disklens-hash-{}.txt", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let h = compute_hashes(path.to_str().unwrap()).unwrap();
        assert_eq!(h.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(h.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(h.sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn compression_round_trip_on_ntfs() {
        let path = std::env::temp_dir().join(format!("disklens-compress-{}.txt", std::process::id()));
        std::fs::write(&path, vec![b'a'; 256 * 1024]).unwrap();
        let p = path.to_str().unwrap();
        compress_file(p, Compression::Lznt1).unwrap();
        compress_file(p, Compression::Xpress4k).unwrap();
        compress_file(p, Compression::None).unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn owner_of_temp_file_is_known() {
        let path = std::env::temp_dir();
        assert!(owner(path.to_str().unwrap()).is_some());
    }
}
