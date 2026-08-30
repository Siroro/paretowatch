//! One-time Windows app-identity registration for toast notifications.
//!
//! Windows only displays toast banners for a registered AUMID (app user
//! model id). An unpackaged app registers one per-user, without elevation,
//! in two places: a registry key under `HKCU\Software\Classes\AppUserModelId`
//! (sufficient on current Windows builds) and a Start Menu shortcut carrying
//! the `System.AppUserModel.ID` property (required on older builds). Both are
//! written idempotently in a background thread at startup, so an installed
//! copy and a directly-run exe get identical, correctly attributed toasts.

use std::error::Error;
use std::path::PathBuf;

use windows::Win32::Foundation::{ERROR_SUCCESS, PROPERTYKEY};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Interface, PCWSTR};

pub(crate) const AUMID: &str = "io.github.siroro.ParetoWatch";
const DISPLAY_NAME: &str = "ParetoWatch";

/// `PKEY_AppUserModel_ID` from propsys.dll ({9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}, pid 5).
const PKEY_APPUSERMODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    pid: 5,
};

/// Register the AUMID if it is not already registered. Errors are reported
/// to stderr and otherwise swallowed: on failure toasts fall back to the
/// borrowed PowerShell identity exactly as before.
pub(crate) fn ensure_registered() {
    if let Err(err) = register() {
        eprintln!("ParetoWatch: toast identity registration failed: {err}");
    }
}

fn register() -> Result<(), Box<dyn Error>> {
    write_registry_identity()?;
    create_start_menu_shortcut()?;
    Ok(())
}

fn write_registry_identity() -> Result<(), Box<dyn Error>> {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegSetValueExW,
    };

    unsafe {
        let subkey = wide(&format!(r"Software\Classes\AppUserModelId\{AUMID}"));
        let mut hkey = HKEY::default();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );
        if status != ERROR_SUCCESS {
            return Err(format!("RegCreateKeyExW failed with {status:?}").into());
        }
        let display_name = wide(DISPLAY_NAME);
        let bytes: &[u8] = std::slice::from_raw_parts(
            display_name.as_ptr().cast(),
            display_name.len() * std::mem::size_of::<u16>(),
        );
        let status = RegSetValueExW(
            hkey,
            PCWSTR(wide("DisplayName").as_ptr()),
            None,
            REG_SZ,
            Some(bytes),
        );
        if status != ERROR_SUCCESS {
            let _ = RegCloseKey(hkey);
            return Err(format!("RegSetValueExW failed with {status:?}").into());
        }
        let _ = RegCloseKey(hkey);
    }
    Ok(())
}

fn create_start_menu_shortcut() -> Result<(), Box<dyn Error>> {
    let exe = std::env::current_exe()?;
    let lnk = shortcut_path()?;
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&windows::core::HSTRING::from(exe.as_os_str()))?;
        if let Some(dir) = exe.parent() {
            link.SetWorkingDirectory(&windows::core::HSTRING::from(dir.as_os_str()))?;
        }
        let store: IPropertyStore = link.cast()?;
        store.SetValue(&PKEY_APPUSERMODEL_ID, &PROPVARIANT::from(AUMID))?;
        store.Commit()?;
        let persist: IPersistFile = link.cast()?;
        persist.Save(&windows::core::HSTRING::from(lnk.as_os_str()), true)?;
    }
    Ok(())
}

fn shortcut_path() -> Result<PathBuf, Box<dyn Error>> {
    let base = std::env::var_os("APPDATA").ok_or("APPDATA environment variable not set")?;
    Ok(PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs\ParetoWatch.lnk"))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
