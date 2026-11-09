//! Program icons for the list, served as PNG through the `icon` URI scheme.

use crate::core::inventory;
use crate::win::icon;

/// Pixel size of the extracted icon: sharp for the 28 px list icon up to 150 % scaling.
const ICON_PX: u32 = 48;

/// PNG of a program's `DisplayIcon`, looked up by its opaque id (never a path from the UI).
pub fn png(id: &str) -> Option<Vec<u8>> {
    let raw = inventory::find(id)?.display_icon?;
    let (file, index) = parse_display_icon(&raw)?;
    let pixels = icon::extract(&icon::expand_env(&file), index, ICON_PX)?;
    encode(&pixels, ICON_PX)
}

/// `DisplayIcon` is `path`, `path,index` or `"path",index` (negative index: resource id).
fn parse_display_icon(raw: &str) -> Option<(String, i32)> {
    let raw = raw.trim();
    let (path, index) = match raw.strip_prefix('"') {
        Some(quoted) => {
            let (path, rest) = quoted.split_once('"')?;
            (path, rest.trim().strip_prefix(',').map(str::trim))
        }
        None => match raw.rsplit_once(',') {
            Some((path, index)) if index.trim().parse::<i32>().is_ok() => {
                (path, Some(index.trim()))
            }
            _ => (raw, None),
        },
    };
    let index = index.and_then(|i| i.parse().ok()).unwrap_or(0);
    let path = path.trim();
    (!path.is_empty()).then(|| (path.to_owned(), index))
}

fn encode(rgba: &[u8], size: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().ok()?.write_image_data(rgba).ok()?;
    Some(out)
}

/// Decodes `%XX` escapes of a URI path segment; `None` if malformed or not UTF-8.
pub fn percent_decode(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = segment.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_display_icon_forms() {
        let p = parse_display_icon;
        assert_eq!(p(r"C:\a\app.exe"), Some((r"C:\a\app.exe".into(), 0)));
        assert_eq!(p(r"C:\a\app.exe,2"), Some((r"C:\a\app.exe".into(), 2)));
        assert_eq!(
            p(r#""C:\a b\app.exe",-101"#),
            Some((r"C:\a b\app.exe".into(), -101))
        );
        assert_eq!(
            p(r#""C:\a b\app.ico""#),
            Some((r"C:\a b\app.ico".into(), 0))
        );
        assert_eq!(p(r"C:\a,b\app.exe"), Some((r"C:\a,b\app.exe".into(), 0)));
        assert_eq!(p("  "), None);
        assert_eq!(p(r#""unterminated"#), None);
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(
            percent_decode("hklm64%5C%7BAB%7D%20x").as_deref(),
            Some(r"hklm64\{AB} x")
        );
        assert_eq!(percent_decode("%E2%9C%93").as_deref(), Some("✓"));
        assert_eq!(percent_decode("%5"), None);
        assert_eq!(percent_decode("%FF"), None);
    }

    #[test]
    fn encodes_a_png() {
        let png = encode(&[0u8; 4 * 4 * 4], 4).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
