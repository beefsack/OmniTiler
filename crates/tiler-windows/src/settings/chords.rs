#![forbid(unsafe_code)]
/// Parsed canonical chord: Win mandatory, optional Shift/Alt/Ctrl, one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub vk: u32,
    pub key_name: &'static str,
}

/// Virtual-key constants shared with the classifier (letters/digits/F-keys
/// derive arithmetically; arrows/tab mirror `snapkey`).
pub const VK_LEFT: u32 = 37;
pub const VK_UP: u32 = 38;
pub const VK_RIGHT: u32 = 39;
pub const VK_DOWN: u32 = 40;
/// Digit base for the item 2 numbered-stay canonical slots (shared with the
/// numbered follow sends on the same digit).
pub const VK_0: u32 = 0x30;
/// Tab chord key for the item 1 previous-view toggle (Win+Ctrl+Tab).
pub const VK_TAB: u32 = 0x09;
/// Float/sticky shared virtual key: the only canonical target two rebound
/// rows may share (separate arm slots), every other shared canonical target
/// would collide in one slot and refuses in validation.
pub const VK_G: u32 = 0x47;

/// Map a canonical key name to its virtual key. Letters A-Z, digits 0-9,
/// F1-F24, the four arrows, and Tab. Anything else is malformed: chords may
/// only name known keys.
#[must_use]
pub fn vk_for_key_name(name: &str) -> Option<(u32, &'static str)> {
    if name.eq_ignore_ascii_case("tab") {
        return Some((VK_TAB, "Tab"));
    }
    const ARROWS: [(&str, u32); 4] = [
        ("Left", VK_LEFT),
        ("Up", VK_UP),
        ("Right", VK_RIGHT),
        ("Down", VK_DOWN),
    ];
    if name.len() == 1 {
        let byte = name.as_bytes()[0];
        if byte.is_ascii_alphabetic() {
            let upper = byte.to_ascii_uppercase();
            // Leak-free static names for A-Z.
            const LETTERS: [&str; 26] = [
                "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
                "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
            ];
            let index = usize::from(upper - b'A');
            return Some((u32::from(upper), LETTERS[index]));
        }
        if byte.is_ascii_digit() {
            const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
            let index = usize::from(byte - b'0');
            return Some((u32::from(byte), DIGITS[index]));
        }
        return None;
    }
    for (text, vk) in ARROWS {
        if name.eq_ignore_ascii_case(text) {
            return Some((vk, text));
        }
    }
    if name.len() >= 2
        && name.len() <= 3
        && (name.as_bytes()[0] == b'F' || name.as_bytes()[0] == b'f')
        && name[1..].bytes().all(|b| b.is_ascii_digit())
    {
        let number: u32 = name[1..].parse().ok()?;
        if (1..=24).contains(&number) {
            // Canonical F-key names live behind one static table.
            const FNAMES: [&str; 24] = [
                "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13",
                "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
            ];
            return Some((0x6F + number, FNAMES[number as usize - 1]));
        }
    }
    None
}

/// Parse one canonical textual chord (`Win[+Shift][+Alt][+Ctrl]+Key`,
/// case-insensitive, any token order). `Win` is mandatory and exactly one
/// known key is required; duplicate tokens refuse.
pub fn parse_chord(text: &str) -> Result<Chord, String> {
    let usage = "refuse: chord needs Win[+Shift][+Alt][+Ctrl]+Key".to_owned();
    let mut win = false;
    let mut shift = false;
    let mut alt = false;
    let mut ctrl = false;
    let mut key: Option<(u32, &'static str)> = None;
    let mut parts = 0u32;
    for raw in text.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            return Err(usage);
        }
        parts += 1;
        if token.eq_ignore_ascii_case("win") {
            if win {
                return Err(usage);
            }
            win = true;
        } else if token.eq_ignore_ascii_case("shift") {
            if shift {
                return Err(usage);
            }
            shift = true;
        } else if token.eq_ignore_ascii_case("alt") {
            if alt {
                return Err(usage);
            }
            alt = true;
        } else if token.eq_ignore_ascii_case("ctrl") {
            if ctrl {
                return Err(usage);
            }
            ctrl = true;
        } else if key.is_none() && vk_for_key_name(token).is_some() {
            key = vk_for_key_name(token);
        } else {
            return Err(usage);
        }
    }
    if !win || parts < 2 {
        return Err(usage);
    }
    let Some((vk, key_name)) = key else {
        return Err(usage);
    };
    Ok(Chord {
        shift,
        alt,
        ctrl,
        vk,
        key_name,
    })
}

/// Render a parsed chord back to canonical order.
#[must_use]
pub fn render_chord(chord: &Chord) -> String {
    let mut out = String::from("Win");
    if chord.shift {
        out.push_str("+Shift");
    }
    if chord.alt {
        out.push_str("+Alt");
    }
    if chord.ctrl {
        out.push_str("+Ctrl");
    }
    out.push('+');
    out.push_str(chord.key_name);
    out
}

/// True for the physical OS lock chord: unshifted Win+L without Alt/Ctrl.
/// The low-level hook cannot reliably intercept it, so it always passes
/// through unless explicitly opted in, and no remap may ever target it
/// (no insecure bypass via remap).
#[must_use]
pub const fn is_lock_chord(chord: &Chord) -> bool {
    chord.vk == 0x4C && !chord.shift && !chord.alt && !chord.ctrl
}
