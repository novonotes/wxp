mod blocking_variable;
mod capsule;
mod future_completer;

pub(crate) use blocking_variable::BlockingVariable;
#[cfg(test)]
pub(crate) use capsule::Capsule;
pub(crate) use future_completer::FutureCompleter;

use std::time::{SystemTime, UNIX_EPOCH};

/// A short, mostly-unique suffix (base-36 of the current millisecond) for
/// per-process-instance native names.
///
/// Used to give each load of this library its own Win32 window class /
/// CFRunLoop mode name. Several plugin DLLs — or repeated unload/reload of the
/// same one — can coexist in one host process; a timestamp (rather than the
/// module identity) guarantees a reloaded copy starts with a fresh name instead
/// of colliding with a registration left by the previous load.
pub fn get_timestamp_suffix() -> String {
    const BASE36_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    if timestamp == 0 {
        return "0".to_string();
    }

    let mut result = Vec::new();
    let mut num = timestamp;

    while num > 0 {
        let digit = (num % 36) as usize;
        result.push(BASE36_CHARS[digit]);
        num /= 36;
    }

    result.reverse();
    String::from_utf8(result).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::debug;

    #[test]
    fn test_get_timestamp_suffix() {
        // Should return a non-empty string
        let suffix = get_timestamp_suffix();
        debug!("suffix: {}", suffix);
        assert!(!suffix.is_empty());

        // Should consist only of uppercase alphanumeric characters
        assert!(
            suffix.chars().all(
                |c| c.is_ascii_alphanumeric() && (c.is_ascii_digit() || c.is_ascii_uppercase())
            )
        );

        // Consecutive calls should return different values (millisecond precision, so sleep)
        let suffix1 = get_timestamp_suffix();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let suffix2 = get_timestamp_suffix();
        assert_ne!(suffix1, suffix2);
    }
}
