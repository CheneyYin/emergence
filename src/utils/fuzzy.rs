/// 计算两个字符串的 Levenshtein 编辑距离
pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();

    if len1 == 0 { return len2; }
    if len2 == 0 { return len1; }

    let chars1: Vec<char> = s1.chars().collect();
    let chars2: Vec<char> = s2.chars().collect();

    let mut prev_row: Vec<usize> = (0..=len2).collect();
    let mut curr_row = vec![0usize; len2 + 1];

    for i in 1..=len1 {
        curr_row[0] = i;
        for j in 1..=len2 {
            let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };
            curr_row[j] = (prev_row[j] + 1)
                .min(curr_row[j - 1] + 1)
                .min(prev_row[j - 1] + cost);
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[len2]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that levenshtein distance is 0 for identical strings.
    #[test]
    fn test_same_string() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    /// Verifies that levenshtein distance is 1 for a single character difference.
    #[test]
    fn test_one_edit() {
        assert_eq!(levenshtein_distance("compac", "compact"), 1);
    }

    /// Verifies that levenshtein distance equals length when strings are entirely different.
    #[test]
    fn test_completely_different() {
        assert_eq!(levenshtein_distance("abc", "xyz"), 3);
    }
}
