//! Wildcard pattern matching for permission rules.
//!
//! Mirrors `src/util/wildcard.ts` from the original OpenCode.
//! Supports `*` (match any sequence) and `?` (match single char).

/// Check if a string matches a wildcard pattern.
///
/// - `*` matches any number of characters (including none)
/// - `?` matches exactly one character
///
/// # Examples
/// ```
/// use opencoder_core::util::wildcard::matches;
/// assert!(matches("*.ts", "foo.ts"));
/// assert!(matches("src/**/*.rs", "src/foo/bar.rs"));
/// assert!(!matches("*.ts", "foo.rs"));
/// ```
pub fn matches(pattern: &str, text: &str) -> bool {
    // Convert to globset for robust matching
    // But for simple cases, use a fast path
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == text;
    }

    // DP matching
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    let m = pat.len();
    let n = txt.len();

    // dp[i][j] = pattern[0..i] matches text[0..j]
    let mut dp = vec![vec![false; n + 1]; m + 1];
    dp[0][0] = true;

    // Handle leading *
    for i in 1..=m {
        if pat[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        } else {
            break;
        }
    }

    for i in 1..=m {
        for j in 1..=n {
            if pat[i - 1] == '*' {
                // * matches zero chars (dp[i-1][j]) or one more char (dp[i][j-1])
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if pat[i - 1] == '?' || pat[i - 1] == txt[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[m][n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_wildcards() {
        assert!(matches("*", "anything"));
        assert!(matches("*.ts", "foo.ts"));
        assert!(matches("*.ts", ".ts"));
        assert!(!matches("*.ts", "foo.rs"));
        assert!(matches("foo*", "foobar"));
        assert!(matches("foo*bar", "fooXXXbar"));
        assert!(!matches("foo*bar", "fooXXXbaz"));
    }

    #[test]
    fn question_mark() {
        assert!(matches("f?o", "foo"));
        assert!(matches("f?o", "fXo"));
        assert!(!matches("f?o", "fo"));
        assert!(!matches("f?o", "fXXo"));
    }

    #[test]
    fn exact_match() {
        assert!(matches("exact", "exact"));
        assert!(!matches("exact", "other"));
    }

    #[test]
    fn complex_patterns() {
        assert!(matches("*.env", "prod.env"));
        assert!(matches("*.env.*", ".env.local"));
        assert!(matches("*.env.example", "app.env.example"));
    }
}
