//! Windows shell integrations: Explorer's file-operation engine, launching
//! programs and consoles, elevation, the Recycle Bin and Explorer's own
//! context menu.

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::{null, null_mut};

use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FO_MOVE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_WANTNUKEWARNING,
    SEE_MASK_INVOKEIDLIST, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, SHEmptyRecycleBinW,
    SHFILEOPSTRUCTW, SHFileOperationW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOW, SW_SHOWNORMAL};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Window handle as a plain integer, so it can cross threads.
pub type Hwnd = isize;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileOp {
    /// To the Recycle Bin. Windows asks before permanently deleting anything
    /// too big for the bin, and handles files in use with its own dialogs.
    Recycle,
    /// Permanently, after DiskLens's own confirmation.
    Delete,
    Move,
}

/// Run an operation through Explorer's engine with its normal progress and
/// conflict dialogs. Returns whether the user or Windows aborted part of it;
/// callers check which paths are actually gone.
pub fn file_operation(hwnd: Hwnd, op: FileOp, paths: &[String], dest: Option<&str>) -> Result<bool, String> {
    if paths.is_empty() {
        return Ok(false);
    }
    // Double-NUL-terminated lists of plain (not \\?\) paths.
    let mut from: Vec<u16> = Vec::new();
    for p in paths {
        from.extend(p.encode_utf16());
        from.push(0);
    }
    from.push(0);
    let to: Vec<u16> = dest.map(|d| d.encode_utf16().chain([0, 0]).collect()).unwrap_or_default();

    let flags = match op {
        FileOp::Recycle => FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_WANTNUKEWARNING,
        FileOp::Delete => FOF_NOCONFIRMATION,
        FileOp::Move => FOF_NOCONFIRMMKDIR | FOF_ALLOWUNDO,
    };
    let mut info = SHFILEOPSTRUCTW {
        hwnd: hwnd as _,
        wFunc: if op == FileOp::Move { FO_MOVE } else { FO_DELETE },
        pFrom: from.as_ptr(),
        pTo: if to.is_empty() { null() } else { to.as_ptr() },
        fFlags: flags as u16,
        fAnyOperationsAborted: 0,
        hNameMappings: null_mut(),
        lpszProgressTitle: null(),
    };
    let result = unsafe { SHFileOperationW(&mut info) };
    // 0x4C7 (ERROR_CANCELLED) means the user cancelled a Windows prompt.
    if result != 0 && result != 0x4C7 {
        return Err(format!("Windows could not complete the operation (code 0x{result:X})"));
    }
    Ok(info.fAnyOperationsAborted != 0 || result == 0x4C7)
}

/// ShellExecuteEx wrapper. `verb` "runas" asks for administrator rights.
pub fn shell_execute(file: &str, params: Option<&str>, verb: Option<&str>, dir: Option<&str>, show: bool) -> Result<(), String> {
    let file_w = wide(file);
    let params_w = params.map(wide);
    let verb_w = verb.map(wide);
    let dir_w = dir.map(wide);
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpFile = file_w.as_ptr();
    info.lpParameters = params_w.as_ref().map_or(null(), |p| p.as_ptr());
    info.lpVerb = verb_w.as_ref().map_or(null(), |v| v.as_ptr());
    info.lpDirectory = dir_w.as_ref().map_or(null(), |d| d.as_ptr());
    info.nShow = if show { SW_SHOWNORMAL } else { SW_HIDE };
    if unsafe { ShellExecuteExW(&mut info) } != 0 {
        if !info.hProcess.is_null() {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(info.hProcess) };
        }
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(1223) {
            Err("Cancelled".into())
        } else {
            Err(err.to_string())
        }
    }
}

/// Open Explorer's Properties dialog for a file or folder.
pub fn show_properties(path: &str) -> Result<(), String> {
    let verb = wide("properties");
    let file = wide(path);
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_INVOKEIDLIST;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.nShow = SW_SHOW;
    if unsafe { ShellExecuteExW(&mut info) } != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A Command Prompt or PowerShell window in `dir`.
pub fn open_console(dir: &str, powershell: bool) -> Result<(), String> {
    let mut cmd = if powershell {
        let exe = if which("pwsh.exe") { "pwsh.exe" } else { "powershell.exe" };
        let mut c = Command::new(exe);
        c.arg("-NoExit");
        c
    } else {
        let mut c = Command::new(std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()));
        c.arg("/K");
        c
    };
    cmd.current_dir(dir).creation_flags(CREATE_NEW_CONSOLE).spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn which(exe: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
        .unwrap_or(false)
}

/// Run `command` through `cmd /C` in `dir` (user-defined cleanups).
pub fn run_cmd(command: &str, dir: &str, show_console: bool, wait: bool) -> Result<(), String> {
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
    let mut cmd = Command::new(comspec);
    // raw_arg keeps the user's quoting intact.
    cmd.raw_arg(format!("/C {command}"))
        .current_dir(dir)
        .creation_flags(if show_console { CREATE_NEW_CONSOLE } else { CREATE_NO_WINDOW });
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if wait {
        child.wait().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// An elevated console that runs `command` and waits for a key, so the user
/// sees the output (DISM, shadow copies, profile removal).
pub fn run_elevated_console(title: &str, command: &str) -> Result<(), String> {
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
    let params = format!("/C \"TITLE {title} & {command} & PAUSE\"");
    shell_execute(&comspec, Some(&params), Some("runas"), None, true)
}

/// Empty the Recycle Bin of every drive (DiskLens confirms beforehand).
pub fn empty_recycle_bin(hwnd: Hwnd) -> Result<(), String> {
    const SHERB_NOCONFIRMATION: u32 = 1;
    let hr = unsafe { SHEmptyRecycleBinW(hwnd as _, null(), SHERB_NOCONFIRMATION) };
    // S_OK, or E_UNEXPECTED when the bin was already empty.
    if hr >= 0 || hr as u32 == 0x8000_FFFF { Ok(()) } else { Err(format!("Windows could not empty the Recycle Bin (0x{:08X})", hr as u32)) }
}

/// Start another instance with administrator rights.
pub fn relaunch_elevated() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    shell_execute(&exe.to_string_lossy(), None, Some("runas"), None, true)
}

/// The user's Windows accent colour as `#rrggbb`, from the DWM settings.
pub fn accent_color() -> Option<String> {
    let key = wide(r"Software\Microsoft\Windows\DWM");
    let value = wide("AccentColor");
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    // Stored as 0xAABBGGRR.
    (status == 0).then(|| format!("#{:02x}{:02x}{:02x}", data & 0xff, (data >> 8) & 0xff, (data >> 16) & 0xff))
}

/// Show Explorer's context menu for `paths` (sharing one parent folder) at
/// screen position `x`, `y`, and run the chosen command. Must run on the UI
/// thread. Returns whether a command was invoked.
pub fn explorer_menu(hwnd: Hwnd, paths: &[String], x: i32, y: i32) -> Result<bool, String> {
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
        IContextMenu, ILFindLastID, IShellFolder, SHBindToParent, SHParseDisplayName,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreatePopupMenu, DestroyMenu, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx,
    };
    use windows::core::{PCSTR, PCWSTR};
    // Not exported by the windows crate (shobjidl_core.h).
    const CMIC_MASK_UNICODE: u32 = 0x0000_4000;

    if paths.is_empty() {
        return Ok(false);
    }
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let hwnd = HWND(hwnd as *mut _);

        let mut pidls: Vec<*mut ITEMIDLIST> = Vec::new();
        let free = |pidls: &[*mut ITEMIDLIST]| {
            for &p in pidls {
                CoTaskMemFree(Some(p as *const _));
            }
        };
        for p in paths {
            let w = wide(p);
            let mut pidl: *mut ITEMIDLIST = null_mut();
            if let Err(e) = SHParseDisplayName(PCWSTR(w.as_ptr()), None, &mut pidl, 0, None) {
                free(&pidls);
                return Err(e.message().to_string());
            }
            pidls.push(pidl);
        }

        let result = (|| -> windows::core::Result<bool> {
            let folder: IShellFolder = SHBindToParent(pidls[0], None)?;
            let children: Vec<*const ITEMIDLIST> = pidls.iter().map(|&p| ILFindLastID(p) as *const _).collect();
            let menu: IContextMenu = folder.GetUIObjectOf(hwnd, &children, None)?;
            let hmenu = CreatePopupMenu()?;
            menu.QueryContextMenu(hmenu, 0, 1, 0x7FFF, CMF_NORMAL).ok()?;
            let _ = SetForegroundWindow(hwnd);
            let chosen = TrackPopupMenuEx(hmenu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, x, y, hwnd, None);
            let mut invoked = false;
            if chosen.0 > 0 {
                let offset = (chosen.0 - 1) as usize;
                let info = CMINVOKECOMMANDINFOEX {
                    cbSize: size_of::<CMINVOKECOMMANDINFOEX>() as u32,
                    fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE,
                    hwnd,
                    lpVerb: PCSTR(offset as *const u8),
                    lpVerbW: PCWSTR(offset as *const u16),
                    nShow: SW_SHOWNORMAL,
                    ptInvoke: POINT { x, y },
                    ..Default::default()
                };
                menu.InvokeCommand(&info as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO)?;
                invoked = true;
            }
            let _ = DestroyMenu(hmenu);
            Ok(invoked)
        })();
        free(&pidls);
        result.map_err(|e| e.message().to_string())
    }
}
