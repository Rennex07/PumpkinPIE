//! Scanning and substituting `%placeholder%` tokens.
//!
//! Matches Java PlaceholderAPI: a token is a percent sign, an identifier, and
//! another percent sign. A token may carry one argument after a colon, so
//! `%player_has_permission:some.node%` is one token. Anything that does not fit
//! that shape is left alone, and there is no escape for a literal percent sign.

/// A placeholder occurrence found in a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The identifier between the percent signs, as written.
    pub id: String,
    /// The argument after the first colon, if the token carried one.
    pub argument: Option<String>,
}

impl Token {
    /// The token as it appears in the source text, used when a token cannot be
    /// resolved and has to be put back.
    #[must_use]
    pub fn source(&self) -> String {
        match &self.argument {
            Some(argument) => format!("%{}:{}%", self.id, argument),
            None => format!("%{}%", self.id),
        }
    }
}

const MARKER: u8 = b'%';

fn is_id_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

fn is_id_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_arg_char(byte: u8) -> bool {
    is_id_char(byte) || matches!(byte, b'.' | b'-' | b':' | b'/')
}

/// Reads the token starting at `start`, returning it and the index after it.
///
/// Scanning stops on any non-ASCII byte, and no byte of a multi-byte character
/// is ASCII, so every index this returns lands on a character boundary.
fn scan(bytes: &[u8], start: usize) -> Option<(Token, usize)> {
    let mut index = start + 1;
    if index >= bytes.len() || !is_id_start(bytes[index]) {
        return None;
    }
    let id_start = index;
    while index < bytes.len() && is_id_char(bytes[index]) {
        index += 1;
    }
    let id = String::from_utf8_lossy(&bytes[id_start..index]).into_owned();

    let mut argument = None;
    if index < bytes.len() && bytes[index] == b':' {
        index += 1;
        let arg_start = index;
        while index < bytes.len() && is_arg_char(bytes[index]) {
            index += 1;
        }
        argument = Some(String::from_utf8_lossy(&bytes[arg_start..index]).into_owned());
    }

    if index >= bytes.len() || bytes[index] != MARKER {
        return None;
    }
    Some((Token { id, argument }, index + 1))
}

/// Returns every placeholder in `text`, in order and without duplicates.
#[must_use]
pub fn find(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut found: Vec<Token> = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != MARKER {
            index += 1;
            continue;
        }
        let Some((token, next)) = scan(bytes, index) else {
            index += 1;
            continue;
        };
        if !found.contains(&token) {
            found.push(token);
        }
        index = next;
    }

    found
}

/// The result of [`substitute`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Substituted {
    /// The text with every resolvable placeholder replaced.
    pub text: String,
    /// Ids that could not be resolved, in the order they first appear.
    pub unresolved: Vec<String>,
}

/// Replaces every placeholder in `text` with what `resolve` returns for it.
///
/// A resolver returning `None` means the placeholder is unknown, and the token
/// is put back exactly as it was written. The same placeholder is only offered
/// to the resolver once, so a line that repeats it still costs one lookup.
pub fn substitute(
    text: &str,
    mut resolve: impl FnMut(&str, Option<&str>) -> Option<String>,
) -> Substituted {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut unresolved: Vec<String> = Vec::new();
    // A placeholder, and what it resolved to, so a repeat costs one lookup.
    type Lookup = (String, Option<String>);

    let mut memo: Vec<(Lookup, Option<String>)> = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != MARKER {
            let start = index;
            while index < bytes.len() && bytes[index] != MARKER {
                index += 1;
            }
            out.push_str(&text[start..index]);
            continue;
        }
        let Some((token, next)) = scan(bytes, index) else {
            out.push('%');
            index += 1;
            continue;
        };
        index = next;

        let key = (token.id.clone(), token.argument.clone());
        let value = match memo.iter().find(|(seen, _)| *seen == key) {
            Some((_, value)) => value.clone(),
            None => {
                let value = resolve(&token.id, token.argument.as_deref());
                memo.push((key, value.clone()));
                value
            }
        };

        match value {
            Some(value) => out.push_str(&value),
            None => {
                out.push_str(&token.source());
                if !unresolved.contains(&token.id) {
                    unresolved.push(token.id);
                }
            }
        }
    }

    Substituted { text: out, unresolved }
}
