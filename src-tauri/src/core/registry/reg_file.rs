//! A key and its subkeys in regedit's `.reg` format (ADR 0003): the bytes `reg export`
//! writes, so a backup also opens in Windows' own tools, and read back here to restore
//! it without regedit or a shell.

use std::io;

use crate::win::registry::{Hive, Key, Value, View};

const BOM: [u8; 2] = [0xFF, 0xFE];
const HEADER: &str = "Windows Registry Editor Version 5.00\r\n\r\n";
/// What regedit puts between two parts of a wrapped hex line.
const HEX_BREAK: &str = "\\\r\n  ";
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
    let mut bytes = BOM.to_vec();
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    Some(bytes)
}

/// One key read back from a `.reg` file.
pub struct RegKey {
    pub hive: Hive,
    pub path: String,
    pub values: Vec<Value>,
}

/// The keys of a file written by `export`, parents first; `None` for anything else, so a
/// damaged file restores nothing rather than something different.
pub fn parse(bytes: &[u8]) -> Option<Vec<RegKey>> {
    let text = String::from_utf16(&utf16_units(bytes.strip_prefix(&BOM)?)?).ok()?;
    let body = text.strip_prefix(HEADER)?.replace(HEX_BREAK, "");
    let mut keys: Vec<RegKey> = Vec::new();
    for line in body.split("\r\n").filter(|line| !line.is_empty()) {
        if let Some(full) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let (hive, path) = Hive::parse(full)?;
            keys.push(RegKey {
                hive,
                path: path.to_owned(),
                values: Vec::new(),
            });
        } else {
            keys.last_mut()?.values.push(parse_value(line)?);
        }
    }
    Some(keys)
}

/// Creates the keys and writes their values as they were.
pub fn import(keys: &[RegKey], view: View) -> io::Result<()> {
    for reg_key in keys {
        let key = Key::create(reg_key.hive, &reg_key.path, view)?;
        for value in &reg_key.values {
            key.set(value)?;
        }
    }
    Ok(())
}

fn parse_value(line: &str) -> Option<Value> {
    let (name, data) = match line.strip_prefix("@=") {
        Some(data) => (String::new(), data),
        None => {
            let (name, rest) = unquote(line)?;
            (name, rest.strip_prefix('=')?)
        }
    };
    let (kind, data) = if data.starts_with('"') {
        let (text, rest) = unquote(data)?;
        rest.is_empty().then_some(())?;
        (REG_SZ, utf16z(&text))
    } else if let Some(hex) = data.strip_prefix("dword:") {
        (hex.len() == 8).then_some(())?;
        (
            REG_DWORD,
            u32::from_str_radix(hex, 16).ok()?.to_le_bytes().to_vec(),
        )
    } else {
        let (kind, bytes) = match data.strip_prefix("hex:") {
            Some(bytes) => (REG_BINARY, bytes),
            None => {
                let (kind, bytes) = data.strip_prefix("hex(")?.split_once("):")?;
                (u32::from_str_radix(kind, 16).ok()?, bytes)
            }
        };
        let bytes = if bytes.is_empty() {
            Vec::new()
        } else {
            bytes
                .split(',')
                .map(|byte| (byte.len() == 2).then(|| u8::from_str_radix(byte, 16).ok())?)
                .collect::<Option<_>>()?
        };
        (kind, bytes)
    };
    Some(Value { name, kind, data })
}

/// A quoted, escaped string at the start of `text`, and what follows it.
fn unquote(text: &str) -> Option<(String, &str)> {
    let mut chars = text.strip_prefix('"')?.char_indices();
    let mut out = String::new();
    while let Some((index, c)) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?.1),
            '"' => return Some((out, &text[index + 2..])),
            c => out.push(c),
        }
    }
    None
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
    let units = utf16_units(data)?;
    let (&0, text) = units.split_last()? else {
        return None;
    };
    let text = String::from_utf16(text).ok()?;
    (!text.contains(['\0', '\r', '\n'])).then_some(text)
}

fn escape(text: &str) -> String {
    text.replace('\\', r"\\").replace('"', "\\\"")
}

fn utf16_units(bytes: &[u8]) -> Option<Vec<u16>> {
    let (pairs, []) = bytes.as_chunks::<2>() else {
        return None;
    };
    Some(pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect())
}

/// `text` as REG_SZ stores it: UTF-16 with a terminating null.
fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Value, export, import, parse, parse_value, utf16z, write_value};
    use crate::win::registry::{Hive, Key, View, delete_tree, test_key};

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
    fn reads_back_each_line_it_writes() {
        for (name, kind, data) in [
            ("", 1, utf16z("x")),
            (r#"a"b\c"#, 1, utf16z(r#"C:\"q""#)),
            ("d", 4, vec![0x88, 0x13, 0, 0]),
            ("d", 4, vec![1, 2]),
            ("s", 1, vec![]),
            ("m", 7, utf16z("fr-FR\0")),
            ("v", 3, vec![0xab; 60]),
        ] {
            let mut text = line(name, kind, &data).replace("\\\r\n  ", "");
            text.truncate(text.len() - 2);
            let value = parse_value(&text).unwrap();
            assert_eq!(
                (value.name.as_str(), value.kind, value.data),
                (name, kind, data)
            );
        }
        for bad in [
            "x",
            "\"a\"=",
            "\"a\"=dword:1",
            "\"a\"=hex:1,02",
            "\"a\"=\"b",
            "@=\"b\"c",
        ] {
            assert!(parse_value(bad).is_none(), "{bad}");
        }
        assert!(parse(b"[HKEY_CURRENT_USER\\x]").is_none());
    }

    /// Writes only under `HKCU\Software\UninstallerTest` (`test_key`).
    #[test]
    fn restores_a_deleted_key_exactly() {
        let path = test_key::fresh("reg-file");
        let set = |sub: &str, name: &str, kind: u32, data: &[u8]| {
            Key::create(Hive::CurrentUser, &format!(r"{path}{sub}"), View::Native64)
                .unwrap()
                .set(&Value {
                    name: name.to_owned(),
                    kind,
                    data: data.to_vec(),
                })
                .unwrap();
        };
        set("", "", 1, &utf16z("défaut"));
        set("", r#"guillemet " et \"#, 1, &utf16z(r"C:\Program Files\X"));
        set("", "dword", 4, &[1, 0, 0, 0]);
        set("", "qword", 0xb, &[2; 8]);
        set("", "binary", 3, &[0xab; 100]);
        set("", "multi", 7, &utf16z("un\0deux\0"));
        set("", "expand", 2, &utf16z("%ProgramFiles%"));
        set("", "empty", 1, &[]);
        set(r"\Sub\Deeper", "n", 4, &[9, 0, 0, 0]);
        let saved = export(Hive::CurrentUser, &path, View::Native64).unwrap();

        delete_tree(Hive::CurrentUser, &path, View::Native64).unwrap();
        assert!(Key::open(Hive::CurrentUser, &path, View::Native64).is_none());
        import(&parse(&saved).unwrap(), View::Native64).unwrap();
        assert!(export(Hive::CurrentUser, &path, View::Native64).unwrap() == saved);
        test_key::clear(&path);
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
