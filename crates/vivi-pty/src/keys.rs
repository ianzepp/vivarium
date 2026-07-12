use crate::protocol::KeyModifier;

pub fn encode_key(key: &str, modifiers: &[KeyModifier]) -> Result<Vec<u8>, String> {
    let modifiers = KeyModifiers::parse(modifiers)?;
    if let Some(ch) = single_character(key) {
        return encode_character(ch, modifiers);
    }

    let normalized = key.to_ascii_lowercase();
    let bytes = match normalized.as_str() {
        "enter" | "return" => encode_named(b"\r", modifiers, false)?,
        "escape" | "esc" => encode_named(b"\x1b", modifiers, false)?,
        "tab" => {
            if modifiers.control || modifiers.alt {
                return Err("tab supports only the shift modifier".into());
            }
            if modifiers.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }
        }
        "backspace" | "back" => encode_named(b"\x7f", modifiers, false)?,
        "space" => encode_character(' ', modifiers)?,
        "up" => encode_csi_key('A', modifiers),
        "down" => encode_csi_key('B', modifiers),
        "right" => encode_csi_key('C', modifiers),
        "left" => encode_csi_key('D', modifiers),
        "home" => encode_csi_key('H', modifiers),
        "end" => encode_csi_key('F', modifiers),
        "insert" => encode_tilde_key(2, modifiers),
        "delete" | "del" => encode_tilde_key(3, modifiers),
        "pageup" | "page-up" => encode_tilde_key(5, modifiers),
        "pagedown" | "page-down" => encode_tilde_key(6, modifiers),
        _ if normalized.starts_with('f') => encode_function_key(&normalized, modifiers)?,
        _ => return Err(format!("unsupported terminal key: {key}")),
    };
    Ok(bytes)
}

#[derive(Clone, Copy)]
struct KeyModifiers {
    control: bool,
    alt: bool,
    shift: bool,
}

impl KeyModifiers {
    fn parse(modifiers: &[KeyModifier]) -> Result<Self, String> {
        let mut parsed = Self {
            control: false,
            alt: false,
            shift: false,
        };
        for modifier in modifiers {
            let slot = match modifier {
                KeyModifier::Control => &mut parsed.control,
                KeyModifier::Alt => &mut parsed.alt,
                KeyModifier::Shift => &mut parsed.shift,
            };
            if *slot {
                return Err("duplicate terminal key modifier".into());
            }
            *slot = true;
        }
        Ok(parsed)
    }

    fn parameter(self) -> Option<u8> {
        let value = 1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.control);
        (value != 1).then_some(value)
    }
}

fn single_character(key: &str) -> Option<char> {
    let mut chars = key.chars();
    let ch = chars.next()?;
    chars.next().is_none().then_some(ch)
}

fn encode_character(ch: char, modifiers: KeyModifiers) -> Result<Vec<u8>, String> {
    let ch = if modifiers.shift && ch.is_ascii_lowercase() {
        ch.to_ascii_uppercase()
    } else {
        ch
    };
    let byte = if modifiers.control {
        control_code(ch)?
    } else {
        let mut encoded = [0; 4];
        let bytes = ch.encode_utf8(&mut encoded).as_bytes().to_vec();
        return Ok(prefix_alt(bytes, modifiers.alt));
    };
    Ok(prefix_alt(vec![byte], modifiers.alt))
}

fn control_code(ch: char) -> Result<u8, String> {
    match ch {
        '@' | ' ' => Ok(0),
        '[' => Ok(27),
        '\\' => Ok(28),
        ']' => Ok(29),
        '^' => Ok(30),
        '_' => Ok(31),
        '?' => Ok(127),
        ch if ch.is_ascii_alphabetic() => Ok((ch.to_ascii_uppercase() as u8) & 0x1f),
        _ => Err(format!("control modifier is unsupported for key {ch:?}")),
    }
}

fn prefix_alt(mut bytes: Vec<u8>, alt: bool) -> Vec<u8> {
    if alt {
        bytes.insert(0, 0x1b);
    }
    bytes
}

fn encode_named(
    bytes: &[u8],
    modifiers: KeyModifiers,
    allow_control: bool,
) -> Result<Vec<u8>, String> {
    if (modifiers.shift || modifiers.control) && !allow_control {
        return Err("modifier is unsupported for this terminal key".into());
    }
    Ok(prefix_alt(bytes.to_vec(), modifiers.alt))
}

fn encode_csi_key(final_byte: char, modifiers: KeyModifiers) -> Vec<u8> {
    match modifiers.parameter() {
        None => format!("\x1b[{final_byte}").into_bytes(),
        Some(parameter) => format!("\x1b[1;{parameter}{final_byte}").into_bytes(),
    }
}

fn encode_tilde_key(number: u8, modifiers: KeyModifiers) -> Vec<u8> {
    match modifiers.parameter() {
        None => format!("\x1b[{number}~").into_bytes(),
        Some(parameter) => format!("\x1b[{number};{parameter}~").into_bytes(),
    }
}

fn encode_function_key(key: &str, modifiers: KeyModifiers) -> Result<Vec<u8>, String> {
    let number = key[1..]
        .parse::<u8>()
        .map_err(|_| format!("unsupported terminal key: {key}"))?;
    if !(1..=12).contains(&number) {
        return Err(format!("unsupported terminal key: {key}"));
    }
    let plain = match number {
        1 => "\x1bOP",
        2 => "\x1bOQ",
        3 => "\x1bOR",
        4 => "\x1bOS",
        5 => "\x1b[15~",
        6 => "\x1b[17~",
        7 => "\x1b[18~",
        8 => "\x1b[19~",
        9 => "\x1b[20~",
        10 => "\x1b[21~",
        11 => "\x1b[23~",
        12 => "\x1b[24~",
        _ => return Err(format!("unsupported terminal key: {key}")),
    };
    let Some(parameter) = modifiers.parameter() else {
        return Ok(plain.as_bytes().to_vec());
    };
    let suffix = if number <= 4 {
        char::from(b'P' + number - 1)
    } else {
        '~'
    };
    let base = match number {
        1..=4 => 1,
        5 => 15,
        6 => 17,
        7 => 18,
        8 => 19,
        9 => 20,
        10 => 21,
        11 => 23,
        12 => 24,
        _ => return Err(format!("unsupported terminal key: {key}")),
    };
    Ok(format!("\x1b[{base};{parameter}{suffix}").into_bytes())
}
