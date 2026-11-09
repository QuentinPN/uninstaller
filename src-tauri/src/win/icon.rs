//! Program icons: one icon of an .exe, .dll or .ico, as straight RGBA pixels.

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, HBITMAP,
    ReleaseDC,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetIconInfo, HICON, ICONINFO, PrivateExtractIconsW,
};
use windows::core::PCWSTR;

/// Expands `%SystemRoot%`-style variables; the input unchanged if that fails.
pub fn expand_env(raw: &str) -> String {
    let source = wide(raw);
    let mut buffer = vec![0u16; 1024];
    // SAFETY: `source` is NUL-terminated and outlives the call; the slice length is
    // the buffer capacity the API may write to.
    let written =
        unsafe { ExpandEnvironmentStringsW(PCWSTR(source.as_ptr()), Some(&mut buffer)) } as usize;
    if written == 0 || written > buffer.len() {
        return raw.to_owned();
    }
    // `written` counts the terminating NUL.
    String::from_utf16_lossy(&buffer[..written - 1])
}

/// The icon at `index` (negative: resource id) of `file`, `size` px square, as RGBA.
pub fn extract(file: &str, index: i32, size: u32) -> Option<Vec<u8>> {
    extract_from(file, index, size).or_else(|| {
        // Since Windows 10 1903, system files keep their icons in a `.mun` beside
        // the other system resources, which this API does not follow.
        let name = std::path::Path::new(file).file_name()?.to_str()?;
        let mun = expand_env(&format!(r"%SystemRoot%\SystemResources\{name}.mun"));
        extract_from(&mun, index, size)
    })
}

fn extract_from(file: &str, index: i32, size: u32) -> Option<Vec<u8>> {
    // The API takes a MAX_PATH buffer; longer paths have no icon.
    let mut path = [0u16; 260];
    let name: Vec<u16> = file.encode_utf16().collect();
    path.get_mut(..name.len())?.copy_from_slice(&name);
    if name.len() >= path.len() {
        return None;
    }
    let mut icons = [HICON::default()];
    // SAFETY: `path` is NUL-terminated; `icons` has room for the one icon asked for.
    let count = unsafe {
        PrivateExtractIconsW(
            &path,
            index,
            size as i32,
            size as i32,
            Some(&mut icons),
            None,
            0,
        )
    };
    let icon = icons[0];
    // 0: no icon; u32::MAX: file not found.
    if count == 0 || count == u32::MAX || icon.is_invalid() {
        return None;
    }
    let pixels = rgba(icon, size);
    // SAFETY: `icon` was created by PrivateExtractIconsW above and is destroyed once.
    let _ = unsafe { DestroyIcon(icon) };
    pixels
}

fn rgba(icon: HICON, size: u32) -> Option<Vec<u8>> {
    let mut info = ICONINFO::default();
    // SAFETY: `icon` is a live icon; `info` is a valid out-pointer. The two bitmaps it
    // returns are ours to delete, which happens below on every path.
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    let color = (!info.hbmColor.is_invalid()).then(|| bits(info.hbmColor, size));
    let mask = bits(info.hbmMask, size);
    // SAFETY: both bitmaps came from GetIconInfo and are deleted exactly once.
    unsafe {
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
    }
    let mut pixels = color.flatten()?;
    let mask = mask?;
    // Old icons have no alpha channel: the AND mask says what is transparent.
    let has_alpha = pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
    for (p, m) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(mask.as_chunks::<4>().0)
    {
        p.swap(0, 2); // BGRA to RGBA
        if !has_alpha {
            p[3] = if m[0] == 0 { 255 } else { 0 };
        }
    }
    Some(pixels)
}

/// A bitmap's pixels as top-down 32-bit BGRA, `size` x `size`.
fn bits(bitmap: HBITMAP, size: u32) -> Option<Vec<u8>> {
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    // SAFETY: the screen DC is released below; `pixels` holds exactly `size` rows of
    // `size` 32-bit pixels, the format `header` asks for.
    let lines = unsafe {
        let dc = GetDC(None);
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            size,
            Some(pixels.as_mut_ptr().cast()),
            &mut header,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        lines
    };
    (lines == size as i32).then_some(pixels)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_variables() {
        let windir = std::env::var("SystemRoot").unwrap();
        assert_eq!(expand_env(r"%SystemRoot%\x"), format!(r"{windir}\x"));
        assert_eq!(expand_env("plain"), "plain");
    }

    /// Real system file, read-only: Notepad has an icon with transparent corners.
    #[test]
    fn extracts_notepad_with_alpha() {
        let pixels = extract(&expand_env(r"%SystemRoot%\notepad.exe"), 0, 48).unwrap();
        assert_eq!(pixels.len(), 48 * 48 * 4);
        assert_eq!(pixels[3], 0, "top-left corner is transparent");
        assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[3] == 255));
    }

    #[test]
    fn missing_file_is_none() {
        assert!(extract(r"C:\nope\missing.exe", 0, 48).is_none());
    }
}
