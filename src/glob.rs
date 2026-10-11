//! Small glob matcher (no extra dependency): supports * ? ** and [a-z] classes.
//!
//! Both the pattern and the candidate path use / as separator and are relative
//! to the module directory.
//!   *      any characters except /
//!   **     any characters, crossing directory boundaries
//!   **/    zero or more directory levels
//!   ?      a single character other than /
//!   [a-z]  character class; [!a-z] or [^a-z] negates it

/// Normalize separators so Windows paths match / patterns.
pub fn normalize_slashes(input: &str) -> String {
    input.replace('\\', "/")
}

pub fn matches(pattern: &str, path: &str) -> bool {
    let pat = normalize_slashes(pattern);
    let target = normalize_slashes(path);
    match_bytes(pat.as_bytes(), target.as_bytes())
}

fn match_bytes(p: &[u8], s: &[u8]) -> bool {
    if p.is_empty() {
        return s.is_empty();
    }
    if p.starts_with(b"**/") {
        let rest = &p[3..];
        if match_bytes(rest, s) {
            return true;
        }
        for i in 0..s.len() {
            if s[i] == b'/' && match_bytes(rest, &s[i + 1..]) {
                return true;
            }
        }
        return false;
    }
    match p[0] {
        b'*' => {
            if p.len() >= 2 && p[1] == b'*' {
                let rest = &p[2..];
                (0..=s.len()).any(|i| match_bytes(rest, &s[i..]))
            } else {
                let rest = &p[1..];
                for i in 0..=s.len() {
                    if match_bytes(rest, &s[i..]) {
                        return true;
                    }
                    if i < s.len() && s[i] == b'/' {
                        break;
                    }
                }
                false
            }
        }
        b'?' => !s.is_empty() && s[0] != b'/' && match_bytes(&p[1..], &s[1..]),
        b'[' => {
            if s.is_empty() || s[0] == b'/' {
                return false;
            }
            match parse_class(&p[1..]) {
                Some((content, rest)) => class_match(content, s[0]) && match_bytes(rest, &s[1..]),
                None => s[0] == b'[' && match_bytes(&p[1..], &s[1..]),
            }
        }
        c => !s.is_empty() && s[0] == c && match_bytes(&p[1..], &s[1..]),
    }
}

fn parse_class(p: &[u8]) -> Option<(&[u8], &[u8])> {
    let end = p.iter().position(|&c| c == b']')?;
    if end == 0 {
        return None;
    }
    Some((&p[..end], &p[end + 1..]))
}

fn class_match(content: &[u8], ch: u8) -> bool {
    let (negated, content) = match content.first() {
        Some(&b'!') | Some(&b'^') => (true, &content[1..]),
        _ => (false, content),
    };
    let mut hit = false;
    let mut i = 0;
    while i < content.len() {
        if i + 2 < content.len() && content[i + 1] == b'-' {
            if content[i] <= ch && ch <= content[i + 2] {
                hit = true;
            }
            i += 3;
        } else {
            if content[i] == ch {
                hit = true;
            }
            i += 1;
        }
    }
    hit != negated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_does_not_cross_slash() {
        assert!(matches("src/*.c", "src/a.c"));
        assert!(!matches("src/*.c", "src/sub/a.c"));
    }

    #[test]
    fn double_star_crosses_directories() {
        assert!(matches("src/**/*.c", "src/a.c"));
        assert!(matches("src/**/*.c", "src/x/y/a.c"));
        assert!(!matches("src/**/*.c", "src/a.h"));
        assert!(matches("src/**", "src/x/y/z.txt"));
    }

    #[test]
    fn question_mark_and_class() {
        assert!(matches("a?.c", "ab.c"));
        assert!(!matches("a?.c", "abc.c"));
        assert!(matches("a[0-9].c", "a7.c"));
        assert!(!matches("a[0-9].c", "ax.c"));
        assert!(matches("a[!0-9].c", "ax.c"));
    }

    #[test]
    fn backslashes_are_normalized() {
        assert!(matches("src/**/*.c", "src\\sub\\a.c"));
    }

    #[test]
    fn plain_pattern_matches_exact_path_only() {
        assert!(matches("main.c", "main.c"));
        assert!(!matches("main.c", "src/main.c"));
    }
}
