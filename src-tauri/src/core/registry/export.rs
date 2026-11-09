//! A key and its subkeys in regedit's `.reg` format (ADR 0003): the bytes `reg export`
//! writes, so a backup also opens in Windows' own tools.

use crate::win::registry::{Hive, Key, Value, View};

const HEADER: &str = "Windows Registry Editor Version 5.00\r\n\r\n";
/// regedit breaks a hex line once it reaches this many characters.
const HEX_WRAP: usize = 77;
/// Value type numbers, as `hex(n)` writes them.
const REG_SZ: u32 = 1;
const REG_BINARY: u32 = 3;
const REG_DWORD: u32 = 4;

/// The key at `path` and everything under it, UTF-16 with a byte order mark. `None` if
/// the key, a subkey or a value cannot be read: a partial backup would restore less
/// than was removed.
pub fn export(hive: Hive, path: &str, view: View) -> Option<Vec<u8>> {
    let key = Key::open(hive, path, view)?;
    let mut text = String::from(HEADER);
    write_key(&key, &format!(r"{}\{path}", hive.name()), &mut text)?;
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    Some(bytes)
}

fn write_key(key: &Key, path: &str, out: &mut String) -> Option<()> {
    out.push_str(&format!("[{path}]\r\n"));
    for value in key.values()? {
        write_value(&value, out);
    }
    out.push_str("\r\n");
    for name in key.all_subkey_names()? {
        write_key(&key.subkey(&name)?, &format!(r"{path}\{name}"), out)?;
    }
    Some(())
}

/// One `name=data` line. A string is quoted only when it reads back to the same bytes;
/// anything else is hex, wrapped as regedit does.
fn write_value(value: &Value, out: &mut String) {
    let name = if value.name.is_empty() {
        "@=".to_owned()
    } else {
        format!("\"{}\"=", escape(&value.name))
    };
    out.push_str(&name);
    if value.kind == REG_DWORD && value.data.len() == 4 {
        let dword =
            u32::from_le_bytes([value.data[0], value.data[1], value.data[2], value.data[3]]);
        out.push_str(&format!("dword:{dword:08x}"));
    } else if value.kind == REG_SZ
        && let Some(text) = plain_string(&value.data)
    {
        out.push_str(&format!("\"{}\"", escape(&text)));
    } else {
        let prefix = if value.kind == REG_BINARY {
            "hex:".to_owned()
        } else {
            format!("hex({:x}):", value.kind)
        };
        out.push_str(&prefix);
        let mut line = name.chars().count() + prefix.len();
        for (index, byte) in value.data.iter().enumerate() {
            out.push_str(&format!("{byte:02x}"));
            if index + 1 == value.data.len() {
                break;
            }
            out.push(',');
            line += 3;
            if line >= HEX_WRAP {
                out.push_str("\\\r\n  ");
                line = 2;
            }
        }
    }
    out.push_str("\r\n");
}

/// UTF-16 text ending in its only null, without line breaks.
fn plain_string(data: &[u8]) -> Option<String> {
    let (pairs, []) = data.as_chunks::<2>() else {
        return None;
    };
    let units: Vec<u16> = pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
    let (&0, text) = units.split_last()? else {
        return None;
    };
    let text = String::from_utf16(text).ok()?;
    (!text.contains(['\0', '\r', '\n'])).then_some(text)
}

fn escape(text: &str) -> String {
    text.replace('\\', r"\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::{Value, export, write_value};
    use crate::win::registry::{Hive, View};

    fn line(name: &str, kind: u32, data: &[u8]) -> String {
        let mut out = String::new();
        let value = Value {
            name: name.to_owned(),
            kind,
            data: data.to_vec(),
        };
        write_value(&value, &mut out);
        out
    }

    fn utf16z(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    #[test]
    fn writes_each_type_so_it_reads_back_exactly() {
        assert_eq!(line("", 1, &utf16z("x")), "@=\"x\"\r\n");
        assert_eq!(
            line(r#"a"b\c"#, 1, &utf16z(r#"C:\"q""#)),
            "\"a\\\"b\\\\c\"=\"C:\\\\\\\"q\\\"\"\r\n"
        );
        assert_eq!(
            line("d", 4, &[0x88, 0x13, 0, 0]),
            "\"d\"=dword:00001388\r\n"
        );
        assert_eq!(line("d", 4, &[1, 2]), "\"d\"=hex(4):01,02\r\n");
        assert_eq!(line("b", 3, &[0x9e, 0x1e]), "\"b\"=hex:9e,1e\r\n");
        assert_eq!(
            line("e", 2, &utf16z("%A%")),
            "\"e\"=hex(2):25,00,41,00,25,00,00,00\r\n"
        );
        // Not one null at the end, or a line break: hex, never a lossy string.
        assert_eq!(line("s", 1, &[]), "\"s\"=hex(1):\r\n");
        assert_eq!(line("s", 1, &[0x41, 0]), "\"s\"=hex(1):41,00\r\n");
        assert_eq!(
            line("s", 1, &utf16z("a\nb")),
            "\"s\"=hex(1):61,00,0a,00,62,00,00,00\r\n"
        );
        assert_eq!(
            line("q", 0xb, &[0; 8]),
            "\"q\"=hex(b):00,00,00,00,00,00,00,00\r\n"
        );
    }

    #[test]
    fn wraps_hex_like_regedit() {
        let text = line("PreviousPreferredUILanguages", 7, &[0; 14]);
        assert_eq!(
            text,
            "\"PreviousPreferredUILanguages\"=hex(7):00,00,00,00,00,00,00,00,00,00,00,00,00,\\\r\n  00\r\n"
        );
        let long = line("v", 3, &[0xab; 60]);
        assert!(long.lines().all(|l| l.len() <= 80));
    }

    /// Byte for byte what `reg export` writes for a key every account has, with
    /// subkeys and string, dword, binary and multi-string values. Only reads the
    /// registry; the comparison file goes to %TEMP%.
    #[test]
    fn matches_reg_export() {
        let path = r"Control Panel\Desktop";
        let file = std::env::temp_dir().join(format!("uninstaller-export-{}.reg", std::process::id()));
        let status = std::process::Command::new("reg")
            .args(["export", &format!(r"HKCU\{path}")])
            .arg(&file)
            .arg("/y")
            .output()
            .unwrap()
            .status;
        assert!(status.success());
        let expected = std::fs::read(&file).unwrap();
        std::fs::remove_file(&file).unwrap();
        let ours = export(Hive::CurrentUser, path, View::Native64).unwrap();
        assert!(ours == expected, "differs from reg export");
    }

    #[test]
    fn missing_key_exports_nothing() {
        assert!(
            export(
                Hive::CurrentUser,
                r"SOFTWARE\Uninstaller\Nope",
                View::Native64
            )
            .is_none()
        );
    }
}
