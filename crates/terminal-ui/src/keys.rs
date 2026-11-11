//! Keystroke → PTY byte encoding for the terminal surface.
//!
//! This is pure xterm key translation, and it is the piece most likely to grow
//! (modifier parameters, kitty protocol).

use alacritty_terminal::term::TermMode;
use gpui::Keystroke;

/// Map a GPUI keystroke to PTY bytes (xterm-ish).
///
/// `mode` is the active screen's [`TermMode`]. Only two flags matter for key
/// encoding: `APP_CURSOR` (DECCKM) switches cursor keys from CSI to SS3, and
/// the xterm modifier parameter is derived from the held modifiers.
pub(super) fn encode_keystroke(ks: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    // Page commands (new tab, split, pane focus) are bound to chords that a
    // shell would otherwise see as control bytes — gpui still runs
    // `on_key_down` after a keybinding matches, so they have to be dropped
    // here instead.
    if page_chord(ks) {
        return None;
    }
    let key = ks.key.as_str();
    let mods = &ks.modifiers;
    let alt = mods.alt && !mods.platform;
    let app_cursor = mode.contains(TermMode::APP_CURSOR);

    // Ctrl+<letter> → C0 control. Shift does not change the mapping, so
    // Ctrl+Shift+A is still 0x01.
    if mods.control && !mods.alt && !mods.platform {
        if key.len() == 1 {
            let c = key.chars().next()?.to_ascii_lowercase();
            if c.is_ascii_lowercase() {
                return Some(vec![(c as u8) - b'a' + 1]);
            }
        }
        match key {
            "\\" => return Some(vec![0x1c]),
            "]" => return Some(vec![0x1d]),
            "[" | "escape" => return Some(vec![0x1b]),
            _ => {}
        }
    }

    // xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4).
    let mod_param = 1
        + if mods.shift { 1 } else { 0 }
        + if alt { 2 } else { 0 }
        + if mods.control { 4 } else { 0 };

    // Cursor keys: `ESC [ X` normally, `ESC O X` under DECCKM, and
    // `ESC [ 1 ; m X` when any modifier is held.
    let cursor = |final_byte: char| -> Vec<u8> {
        if mod_param > 1 {
            format!("\x1b[1;{mod_param}{final_byte}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{final_byte}").into_bytes()
        } else {
            format!("\x1b[{final_byte}").into_bytes()
        }
    };
    // `ESC [ n ~` keys carry the modifier as the second parameter.
    let tilde = |n: u8| -> Vec<u8> {
        if mod_param > 1 {
            format!("\x1b[{n};{mod_param}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };

    match key {
        "enter" | "return" => {
            return Some(if alt { b"\x1b\r".to_vec() } else { vec![b'\r'] });
        }
        "tab" => {
            return Some(if mods.shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            });
        }
        "backspace" => {
            return Some(if alt { vec![0x1b, 0x7f] } else { vec![0x7f] });
        }
        "delete" => return Some(tilde(3)),
        "escape" => return Some(vec![0x1b]),
        "up" => return Some(cursor('A')),
        "down" => return Some(cursor('B')),
        "right" => return Some(cursor('C')),
        "left" => return Some(cursor('D')),
        "home" => return Some(cursor('H')),
        "end" => return Some(cursor('F')),
        "pageup" => return Some(tilde(5)),
        "pagedown" => return Some(tilde(6)),
        _ => {}
    }

    if mods.control || mods.platform {
        return None;
    }

    // Option/Alt is the meta prefix for printable keys (⌥B → `ESC b`). The
    // composed char is deliberately ignored here: macOS remaps it under Option
    // (⌥B is "∫"), while the shell wants the base key.
    if alt {
        if key.len() != 1 {
            return None;
        }
        let c = key.chars().next()?;
        if c.is_control() {
            return None;
        }
        let c = if mods.shift {
            c.to_ascii_uppercase()
        } else {
            c
        };
        let mut out = vec![0x1b];
        let mut buf = [0u8; 4];
        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        return Some(out);
    }

    // Composed character — respects the keyboard layout and shift state.
    if let Some(text) = ks.key_char.as_deref()
        && !text.is_empty()
    {
        return Some(text.as_bytes().to_vec());
    }

    // Single printable key without a composed char (fallback).
    if key.len() == 1 {
        let c = key.chars().next()?;
        if !c.is_control() {
            let out = if mods.shift {
                c.to_ascii_uppercase()
            } else {
                c
            };
            return Some(out.to_string().into_bytes());
        }
    }

    None
}

/// Chords the Halo Terminal page binds for itself (see `keymap.rs`).
///
/// Kept in sync with that table: macOS uses the Command modifier, every other
/// platform Ctrl+Shift / Ctrl+Alt. `Ctrl+Tab` is bound on both.
fn page_chord(ks: &Keystroke) -> bool {
    let mods = &ks.modifiers;
    let key = ks.key.as_str();
    let arrow = matches!(key, "left" | "right" | "up" | "down");
    if key == "tab" && mods.control {
        return true;
    }
    if mods.platform {
        return (mods.shift && matches!(key, "t" | "w" | "d" | "e" | "x")) || (mods.alt && arrow);
    }
    mods.control
        && ((mods.shift && matches!(key, "t" | "w" | "d" | "e" | "x")) || (mods.alt && arrow))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_chords_never_reach_the_shell() {
        let mode = TermMode::empty();
        for chord in [
            "ctrl-shift-t",
            "ctrl-shift-w",
            "ctrl-shift-x",
            "ctrl-alt-left",
            "ctrl-tab",
        ] {
            let ks = Keystroke::parse(chord).expect(chord);
            assert_eq!(encode_keystroke(&ks, mode), None, "{chord}");
        }
    }

    #[test]
    fn shell_keys_still_encode() {
        let mode = TermMode::empty();
        let ctrl_c = Keystroke::parse("ctrl-c").expect("ctrl-c");
        assert_eq!(encode_keystroke(&ctrl_c, mode), Some(vec![0x03]));
        let tab = Keystroke::parse("tab").expect("tab");
        assert_eq!(encode_keystroke(&tab, mode), Some(vec![b'\t']));
        let shift_tab = Keystroke::parse("shift-tab").expect("shift-tab");
        assert_eq!(encode_keystroke(&shift_tab, mode), Some(b"\x1b[Z".to_vec()));
    }
}
