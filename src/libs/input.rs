//! Terminal key decoding.
//!
//! russh hands us raw bytes from the client's pty and a single keystroke is
//! *not* always one byte (`\x1b[A` for arrows, 3–6 bytes, and they can be split
//! across two `data()` callbacks). `parse_keys` therefore decodes every
//! complete key it can find in the buffer and leaves a trailing partial escape
//! sequence in place for the next chunk.

/// A decoded key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Esc,
    Tab,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    CtrlC,
    /// A key we recognise as "something happened" but do not bind.
    Unknown,
}

const ESC: u8 = 0x1b;
const DEL: u8 = 0x7f;
const BS: u8 = 0x08;
const CTRL_C: u8 = 0x03;

/// Consumes every complete key from `buf`, keeping any incomplete trailing
/// escape sequence (or partial UTF-8 code point) buffered for the next read.
pub fn parse_keys(buf: &mut Vec<u8>) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut i = 0usize;

    while i < buf.len() {
        match buf[i] {
            ESC => {
                if i + 1 >= buf.len() {
                    // A trailing lone `ESC` is the Escape key: real terminals send
                    // a full sequence in one write, so waiting for a continuation
                    // byte here would make Escape unusable.
                    keys.push(Key::Esc);
                    i += 1;
                    continue;
                }
                match buf[i + 1] {
                    b'[' | b'O' => {
                        // CSI/SS3 sequence: scan for the final byte (0x40..=0x7e).
                        let end = buf[i + 2..]
                            .iter()
                            .position(|b| (0x40..=0x7e).contains(b))
                            .map(|p| i + 2 + p);
                        match end {
                            Some(end) => {
                                keys.push(decode_csi(&buf[i..=end]));
                                i = end + 1;
                            }
                            // Sequence not complete yet — wait for more bytes.
                            None => break,
                        }
                    }
                    // `ESC <key>` (alt-key) or a lone ESC.
                    _ => {
                        keys.push(Key::Esc);
                        i += 1;
                    }
                }
            }
            b'\r' | b'\n' => {
                keys.push(Key::Enter);
                i += 1;
            }
            DEL | BS => {
                keys.push(Key::Backspace);
                i += 1;
            }
            CTRL_C => {
                keys.push(Key::CtrlC);
                i += 1;
            }
            b'\t' => {
                keys.push(Key::Tab);
                i += 1;
            }
            0x20..=0x7e => {
                keys.push(Key::Char(buf[i] as char));
                i += 1;
            }
            _ => {
                // Multi-byte UTF-8 (or a control byte we do not bind).
                match std::str::from_utf8(&buf[i..]) {
                    Ok(text) => {
                        keys.extend(text.chars().map(Key::Char));
                        i = buf.len();
                    }
                    // Incomplete code point at the end of the buffer: keep it.
                    Err(err) if err.error_len().is_none() => break,
                    Err(err) => {
                        i += err.valid_up_to().max(1);
                        keys.push(Key::Unknown);
                    }
                }
            }
        }
    }

    buf.drain(..i);
    keys
}

/// Maps a complete `ESC [ ...` / `ESC O ...` sequence to a key.
fn decode_csi(seq: &[u8]) -> Key {
    // Mouse reports (`ESC [ < ... M/m`) and bracketed paste markers are not keys.
    if seq.get(2) == Some(&b'<') || seq.get(2) == Some(&b'M') {
        return Key::Unknown;
    }
    match seq.get(2) {
        Some(b'A') => Key::Up,
        Some(b'B') => Key::Down,
        Some(b'C') => Key::Right,
        Some(b'D') => Key::Left,
        Some(b'H') => Key::Home,
        Some(b'F') => Key::End,
        Some(b'Z') => Key::Unknown, // back-tab
        _ => match seq.get(3) {
            // `ESC [ <digit> ~` family.
            Some(b'~') => match seq[2] {
                b'1' | b'7' => Key::Home,
                b'2' => Key::Unknown, // insert
                b'3' => Key::Delete,
                b'4' | b'8' => Key::End,
                b'5' => Key::PageUp,
                b'6' => Key::PageDown,
                _ => Key::Unknown, // focus events, modifyOtherKeys, ...
            },
            _ => Key::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(bytes: &[u8]) -> Vec<Key> {
        let mut buf = bytes.to_vec();
        let parsed = parse_keys(&mut buf);
        assert!(buf.is_empty(), "buffer should be drained: {buf:?}");
        parsed
    }

    #[test]
    fn plain_characters() {
        assert_eq!(keys(b"jk"), vec![Key::Char('j'), Key::Char('k')]);
    }

    #[test]
    fn enter_accepts_cr_and_lf() {
        assert_eq!(keys(b"\r"), vec![Key::Enter]);
        assert_eq!(keys(b"\n"), vec![Key::Enter]);
    }

    #[test]
    fn backspace_variants() {
        assert_eq!(keys(&[DEL]), vec![Key::Backspace]);
        assert_eq!(keys(&[BS]), vec![Key::Backspace]);
    }

    #[test]
    fn arrows_and_navigation() {
        assert_eq!(keys(b"\x1b[A"), vec![Key::Up]);
        assert_eq!(
            keys(b"\x1b[B\x1b[C\x1b[D"),
            vec![Key::Down, Key::Right, Key::Left]
        );
        assert_eq!(keys(b"\x1b[5~\x1b[6~"), vec![Key::PageUp, Key::PageDown]);
        assert_eq!(keys(b"\x1b[H\x1b[F"), vec![Key::Home, Key::End]);
        assert_eq!(keys(b"\x1bOA"), vec![Key::Up]);
        assert_eq!(keys(b"\x1b[3~"), vec![Key::Delete]);
    }

    #[test]
    fn utf8_and_control_bytes() {
        assert_eq!(keys("é".as_bytes()), vec![Key::Char('é')]);
        assert_eq!(keys(&[CTRL_C]), vec![Key::CtrlC]);
        assert_eq!(keys(b"\x1b"), vec![Key::Esc]);
    }

    #[test]
    fn split_escape_sequence_is_buffered_until_complete() {
        let mut buf = vec![ESC, b'['];
        assert!(parse_keys(&mut buf).is_empty());
        assert_eq!(buf, vec![ESC, b'[']);
        buf.push(b'A');
        assert_eq!(parse_keys(&mut buf), vec![Key::Up]);
        assert!(buf.is_empty());
    }

    #[test]
    fn split_utf8_is_buffered_until_complete() {
        let bytes = "é".as_bytes().to_vec();
        let mut buf = vec![bytes[0]];
        assert!(parse_keys(&mut buf).is_empty());
        buf.extend_from_slice(&bytes[1..]);
        assert_eq!(parse_keys(&mut buf), vec![Key::Char('é')]);
    }

    #[test]
    fn mouse_reports_are_ignored() {
        assert_eq!(keys(b"\x1b[<0;12;5M"), vec![Key::Unknown]);
    }
}
