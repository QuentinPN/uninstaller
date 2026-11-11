//! An executable's version resource: the product name it carries.

use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::core::HSTRING;

/// The `ProductName` of `file`'s version resource, in its first language; `None` when
/// it has none or it is blank.
pub fn product_name(file: &Path) -> Option<String> {
    let path = HSTRING::from(file.as_os_str());
    // SAFETY: `path` is NUL-terminated and outlives the call; no handle is asked for.
    let size = unsafe { GetFileVersionInfoSizeW(&path, None) };
    if size == 0 {
        return None;
    }
    let mut block = vec![0u8; size as usize];
    // SAFETY: `block` holds the `size` bytes the API was told it may write.
    unsafe { GetFileVersionInfoW(&path, None, size, block.as_mut_ptr().cast()) }.ok()?;
    let translation = query(&block, r"\VarFileInfo\Translation")?;
    let [lang_low, lang_high, page_low, page_high, ..] = translation[..] else {
        return None;
    };
    let lang = u16::from_le_bytes([lang_low, lang_high]);
    let page = u16::from_le_bytes([page_low, page_high]);
    let name = query(
        &block,
        &format!(r"\StringFileInfo\{lang:04x}{page:04x}\ProductName"),
    )?;
    let units: Vec<u16> = name
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .take_while(|&unit| unit != 0)
        .collect();
    let name = String::from_utf16_lossy(&units).trim().to_owned();
    (!name.is_empty()).then_some(name)
}

/// The bytes `VerQueryValueW` finds at `sub` in `block`. A string's length comes in
/// characters, read here as two bytes each; a binary value's comes in bytes.
fn query<'a>(block: &'a [u8], sub: &str) -> Option<&'a [u8]> {
    let mut value: *mut c_void = std::ptr::null_mut();
    let mut length = 0u32;
    // SAFETY: `block` is a version resource filled by `GetFileVersionInfoW`; the API
    // points `value` inside it and sets `length`.
    let found = unsafe {
        VerQueryValueW(
            block.as_ptr().cast(),
            &HSTRING::from(sub),
            &raw mut value,
            &raw mut length,
        )
    };
    if !found.as_bool() || value.is_null() {
        return None;
    }
    let start = (value as usize).checked_sub(block.as_ptr() as usize)?;
    let bytes = if sub.starts_with(r"\StringFileInfo") {
        length as usize * 2
    } else {
        length as usize
    };
    block.get(start..start.checked_add(bytes)?.min(block.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_system_executable_product_name() {
        let notepad =
            Path::new(&std::env::var("SystemRoot").unwrap()).join(r"System32\notepad.exe");
        let name = product_name(&notepad).unwrap();
        assert!(name.contains("Windows"), "{name}");
        assert!(product_name(Path::new(r"C:\nowhere\none.exe")).is_none());
    }
}
