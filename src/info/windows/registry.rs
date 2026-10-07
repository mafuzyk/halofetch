//! Read-only registry access. Each call opens the key it needs and closes it again.

use std::mem::size_of;
use std::ptr::{from_mut, null, null_mut};

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, KEY_READ, RRF_RT_REG_DWORD,
    RRF_RT_REG_SZ,
};

use super::{from_wide, to_wide};

/// Upper bound on the subkeys listed from one key. Device class keys hold a few dozen.
const MAX_SUBKEYS: u32 = 512;

/// Registry key names are at most 255 characters, plus the terminator.
const KEY_NAME_CAPACITY: usize = 256;

/// A string value, trimmed. `None` when it is missing, not a string or empty.
pub(super) fn read_string(root: HKEY, subkey: &str, value: &str) -> Option<String> {
    let subkey = to_wide(subkey);
    let value = to_wide(value);
    let mut size: u32 = 0;
    // SAFETY: a null data pointer asks only for the size in bytes, which `size` receives.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    let mut capacity = (buffer.len() * 2) as u32;
    // SAFETY: `buffer` holds `capacity` bytes, and RegGetValueW writes no more than that.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut capacity,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let text = from_wide(&buffer);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// A DWORD value.
pub(super) fn read_u32(root: HKEY, subkey: &str, value: &str) -> Option<u32> {
    let subkey = to_wide(subkey);
    let value = to_wide(value);
    let mut data: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: `data` is a writable u32 and `size` gives RegGetValueW its byte length.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            from_mut(&mut data).cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

/// Names of the direct subkeys of a key. Empty when the key cannot be opened.
pub(super) fn subkeys(root: HKEY, subkey: &str) -> Vec<String> {
    let Some(key) = Key::open(root, subkey) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for index in 0..MAX_SUBKEYS {
        let mut buffer = [0u16; KEY_NAME_CAPACITY];
        let mut length = buffer.len() as u32;
        // SAFETY: `buffer` holds `length` UTF-16 units; the optional out-parameters are null.
        let status = unsafe {
            RegEnumKeyExW(
                key.0,
                index,
                buffer.as_mut_ptr(),
                &mut length,
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            break;
        }
        let length = (length as usize).min(buffer.len());
        names.push(String::from_utf16_lossy(&buffer[..length]));
    }
    names
}

/// An open registry key, closed when dropped.
struct Key(HKEY);

impl Key {
    fn open(root: HKEY, subkey: &str) -> Option<Key> {
        let subkey = to_wide(subkey);
        let mut key: HKEY = null_mut();
        // SAFETY: `subkey` is NUL-terminated and `key` is a valid out-parameter.
        let status = unsafe { RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_READ, &mut key) };
        if status == ERROR_SUCCESS && !key.is_null() {
            Some(Key(key))
        } else {
            None
        }
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful RegOpenKeyExW and is closed only here.
        unsafe { RegCloseKey(self.0) };
    }
}
