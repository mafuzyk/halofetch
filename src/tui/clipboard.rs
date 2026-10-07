//! Text from the Windows clipboard. Legacy consoles do not deliver a paste as an
//! `Event::Paste`, so the logo editor reads the clipboard directly.

use std::ptr;

use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

/// The clipboard text, when the clipboard holds some.
pub fn read_text() -> Option<String> {
    let format = u32::from(CF_UNICODETEXT);
    // SAFETY: the clipboard is closed on every path after a successful OpenClipboard.
    // The memory from GetClipboardData is read only between GlobalLock and GlobalUnlock,
    // and never past the size GlobalSize reports for it.
    unsafe {
        if IsClipboardFormatAvailable(format) == 0 || OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(format);
        let text = if handle.is_null() {
            None
        } else {
            let units = GlobalLock(handle).cast::<u16>();
            let text = (!units.is_null()).then(|| {
                let capacity = GlobalSize(handle) / 2;
                let len = (0..capacity)
                    .find(|&index| *units.add(index) == 0)
                    .unwrap_or(capacity);
                String::from_utf16_lossy(std::slice::from_raw_parts(units, len))
            });
            GlobalUnlock(handle);
            text
        };
        CloseClipboard();
        text
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reading_the_clipboard_does_not_panic() {
        let _ = super::read_text();
    }
}
