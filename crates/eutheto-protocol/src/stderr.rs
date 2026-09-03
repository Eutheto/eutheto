use std::fmt;

use crate::{ProtocolFault, StateFault};

pub const STDERR_TRUNCATION_MARKER: &str = "[stderr truncated]";

#[derive(Clone, PartialEq, Eq)]
pub struct BoundedStderr {
    retained: String,
    max_bytes: usize,
    truncated: bool,
}

impl BoundedStderr {
    /// Creates a bounded, sanitized stderr accumulator.
    ///
    /// # Errors
    ///
    /// Returns a stderr-limit fault when the ceiling cannot contain the
    /// truncation marker.
    pub fn new(max_bytes: usize) -> Result<Self, ProtocolFault> {
        if max_bytes < STDERR_TRUNCATION_MARKER.len() {
            return Err(StateFault::StderrLimit.into());
        }
        Ok(Self {
            retained: String::new(),
            max_bytes,
            truncated: false,
        })
    }

    pub fn push(&mut self, bytes: &[u8]) {
        if self.truncated {
            return;
        }
        for chunk in bytes.utf8_chunks() {
            for character in chunk.valid().chars() {
                if !self.push_character(character) {
                    return;
                }
            }
            if !chunk.invalid().is_empty() && !self.push_character('\u{fffd}') {
                return;
            }
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.retained
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.retained.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.retained.is_empty()
    }

    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.retained
    }

    fn push_character(&mut self, character: char) -> bool {
        if is_unsafe_control(character) && !matches!(character, '\n' | '\t') {
            return true;
        }
        if self.retained.len().saturating_add(character.len_utf8()) > self.max_bytes {
            self.mark_truncated();
            return false;
        }
        self.retained.push(character);
        true
    }

    fn mark_truncated(&mut self) {
        let prefix_cap = self.max_bytes - STDERR_TRUNCATION_MARKER.len();
        let mut boundary = prefix_cap.min(self.retained.len());
        while !self.retained.is_char_boundary(boundary) {
            boundary -= 1;
        }
        self.retained.truncate(boundary);
        self.retained.push_str(STDERR_TRUNCATION_MARKER);
        self.truncated = true;
    }
}

impl fmt::Debug for BoundedStderr {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundedStderr")
            .field("retained_bytes", &self.retained.len())
            .field("max_bytes", &self.max_bytes)
            .field("truncated", &self.truncated)
            .finish()
    }
}

pub(crate) fn is_unsafe_control(character: char) -> bool {
    character.is_control()
        || matches!(
            character,

            '\u{061c}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{feff}'
        )
}

#[cfg(test)]
mod tests {
    use super::{BoundedStderr, STDERR_TRUNCATION_MARKER};
    use crate::{ProtocolFault, checked_in_policy};
    #[test]
    fn debug_reports_only_bounded_metadata() -> Result<(), ProtocolFault> {
        let mut stderr = BoundedStderr::new(128)?;
        stderr.push(b"representative-secret-stderr");
        let debug = format!("{stderr:?}");
        assert!(!debug.contains("representative-secret-stderr"));
        assert!(debug.contains("retained_bytes"));
        assert!(debug.contains("max_bytes"));
        Ok(())
    }

    #[test]
    fn strips_terminal_controls_and_tolerates_invalid_utf8() -> Result<(), ProtocolFault> {
        let mut stderr = BoundedStderr::new(128)?;
        stderr.push(b"safe\x1b[31m\x00bad\xff\n");
        stderr.push("hidden\u{061c}\u{2028}\u{2029}\u{202e}direction".as_bytes());
        assert!(!stderr.as_str().contains('\u{1b}'));
        assert!(!stderr.as_str().contains('\0'));
        for unsafe_character in ['\u{061c}', '\u{2028}', '\u{2029}', '\u{202e}'] {
            assert!(!stderr.as_str().contains(unsafe_character));
        }
        assert!(stderr.as_str().contains('\u{fffd}'));
        assert!(stderr.as_str().contains('\n'));
        Ok(())
    }

    #[test]
    fn split_invalid_sequences_are_replaced_without_panicking() -> Result<(), ProtocolFault> {
        let mut stderr = BoundedStderr::new(128)?;
        stderr.push(&[0xf0, 0x9f]);
        stderr.push(&[0x92, 0xa9]);
        assert!(
            stderr
                .as_str()
                .chars()
                .all(|character| character == '\u{fffd}')
        );
        assert!(stderr.as_str().chars().count() >= 2);
        Ok(())
    }

    #[test]
    fn emits_one_marker_without_exceeding_byte_ceiling() -> Result<(), ProtocolFault> {
        let ceiling = STDERR_TRUNCATION_MARKER.len() + 5;
        let mut stderr = BoundedStderr::new(ceiling)?;
        stderr.push("αβγ overflowing".as_bytes());
        stderr.push(b" ignored again");
        assert!(stderr.is_truncated());
        assert!(stderr.len() <= ceiling);
        assert_eq!(stderr.as_str().matches(STDERR_TRUNCATION_MARKER).count(), 1);
        assert!(std::str::from_utf8(stderr.as_str().as_bytes()).is_ok());
        Ok(())
    }

    #[test]
    fn exact_ceiling_is_not_truncated() -> Result<(), ProtocolFault> {
        let value = "abcdefghijklmnopqr";
        let mut stderr = BoundedStderr::new(value.len())?;
        stderr.push(value.as_bytes());
        assert_eq!(stderr.as_str(), value);
        assert!(!stderr.is_truncated());
        Ok(())
    }

    #[test]
    fn checked_policy_stderr_cap_is_inclusive_and_bounded() -> Result<(), ProtocolFault> {
        let ceiling = checked_in_policy()?.max_stderr_bytes();
        let mut stderr = BoundedStderr::new(ceiling)?;
        stderr.push(&vec![b'x'; ceiling]);
        assert_eq!(stderr.len(), ceiling);
        assert!(!stderr.is_truncated());
        stderr.push(b"x");
        assert_eq!(stderr.len(), ceiling);
        assert!(stderr.is_truncated());
        assert!(stderr.as_str().ends_with(STDERR_TRUNCATION_MARKER));
        Ok(())
    }
}
