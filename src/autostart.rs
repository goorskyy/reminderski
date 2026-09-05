//! Whether Windows starts Reminderski at login.
//!
//! One value under the user's own Run key. No service, no scheduled task, nothing that asks for
//! administrator rights, and nothing left behind anywhere the user cannot see it: this is the
//! same list Task Manager shows under Startup, so turning it off from outside the application
//! works and is not fought over.

use std::io;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SAM_FLAGS, REG_SZ, RegCloseKey,
    RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};

use crate::log;
use crate::ui::wide;

/// Created by Windows for every profile, so it is opened rather than created.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Reminderski";

/// Long enough for any path this will meet, and small enough to sit on the stack. A path that
/// does not fit is reported rather than silently written half way.
const PATH_LIMIT: usize = 512;

pub fn is_enabled() -> bool {
    stored_command().is_some()
}

pub fn set(enabled: bool) -> io::Result<()> {
    // Worked out before the key is opened, so a failure here cannot leak the handle.
    let command = if enabled {
        Some(wide(&command_line()?))
    } else {
        None
    };

    let key = open(KEY_SET_VALUE)?;
    let status = match &command {
        Some(command) => unsafe {
            RegSetValueExW(
                key,
                wide(VALUE_NAME).as_ptr(),
                0,
                REG_SZ,
                command.as_ptr() as *const u8,
                (command.len() * 2) as u32,
            )
        },
        None => {
            let status = unsafe { RegDeleteValueW(key, wide(VALUE_NAME).as_ptr()) };
            // Turning off something that was already off is not a failure.
            if status == ERROR_FILE_NOT_FOUND {
                ERROR_SUCCESS
            } else {
                status
            }
        }
    };
    unsafe { RegCloseKey(key) };

    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(())
}

/// Points the stored command back at this executable when it no longer names it.
///
/// Reminderski is one file that gets downloaded again on every release, and nothing stops the
/// new one landing in a different folder. Without this the startup entry would quietly go on
/// naming a file that is not there, which is the exact failure starting with Windows exists to
/// prevent, and it would say nothing about it.
pub fn follow_the_executable() {
    let Some(stored) = stored_command() else {
        return;
    };
    let Ok(current) = command_line() else {
        return;
    };
    // Paths on Windows do not care about case, and this one was written by a person often enough
    // to be worth not rewriting the registry over.
    if stored.eq_ignore_ascii_case(&current) {
        return;
    }
    if let Err(error) = set(true) {
        log::problem(&format!("could not update the startup entry: {error}"));
    }
}

fn open(access: REG_SAM_FLAGS) -> io::Result<HKEY> {
    let mut key: HKEY = ptr::null_mut();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            access,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(key)
}

fn stored_command() -> Option<String> {
    let key = open(KEY_QUERY_VALUE).ok()?;

    let mut buffer = [0u16; PATH_LIMIT];
    let mut bytes = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegQueryValueExW(
            key,
            wide(VALUE_NAME).as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            buffer.as_mut_ptr() as *mut u8,
            &mut bytes,
        )
    };
    unsafe { RegCloseKey(key) };

    if status != ERROR_SUCCESS {
        return None;
    }
    // A registry string is not obliged to be terminated, and one written here is. Either way
    // what matters is the text in front of the first terminator.
    let characters = (bytes as usize / 2).min(buffer.len());
    let stored = String::from_utf16_lossy(&buffer[..characters]);
    Some(stored.trim_end_matches('\0').to_string())
}

/// What Windows should run, quoted, because the Run value is read as a command line and the
/// executable may sit somewhere with a space in the name.
fn command_line() -> io::Result<String> {
    let mut buffer = [0u16; PATH_LIMIT];
    let length =
        unsafe { GetModuleFileNameW(ptr::null_mut(), buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    // Filling the buffer exactly means the path was truncated to fit it.
    if length as usize >= buffer.len() {
        return Err(io::Error::other("the path to the executable is too long"));
    }
    Ok(quoted(&String::from_utf16_lossy(
        &buffer[..length as usize],
    )))
}

fn quoted(path: &str) -> String {
    format!("\"{path}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry itself is left to the human running the application: these tests would have
    /// to write to their startup list to say anything about it.
    #[test]
    fn a_path_is_quoted_whole() {
        assert_eq!(
            quoted(r"C:\Program Files\Reminderski\reminderski.exe"),
            "\"C:\\Program Files\\Reminderski\\reminderski.exe\""
        );
    }
}
