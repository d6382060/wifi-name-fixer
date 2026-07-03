//! 权限检测与提权。
//! 原则：启动时绝不弹 UAC；只有用户明确点击需要管理员的操作时才请求提权。

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub fn is_admin() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elev = TOKEN_ELEVATION::default();
        let mut ret = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elev as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elev.TokenIsElevated != 0
    }
}

/// 以管理员身份重新启动本程序。
/// 新实例带 --elevated 参数，启动后会自动开始一轮检测，减少用户操作。
pub fn relaunch_as_admin() -> Result<(), String> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_h = HSTRING::from(exe.as_os_str());
    let r = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(exe_h.as_ptr()),
            w!("--elevated"),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW 返回值 > 32 表示成功；用户在 UAC 弹窗点"否"会返回 SE_ERR_ACCESSDENIED(5)
    if r.0 as isize > 32 {
        Ok(())
    } else {
        Err("您取消了系统的管理员授权弹窗（UAC），操作已放弃。".into())
    }
}
