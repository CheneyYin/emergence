/// 展开环境变量占位符 ${VAR_NAME}
pub fn expand_env_vars(value: &str) -> String {
    let re = regex::Regex::new(r"\$\{(\w+)\}").unwrap();
    re.replace_all(value, |caps: &regex::Captures| {
        let var_name = &caps[1];
        std::env::var(var_name).unwrap_or_else(|_| caps[0].to_string())
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that ${VAR_NAME} placeholders are replaced with environment variable values.
    #[test]
    fn test_expand_env_var() {
        std::env::set_var("EMERGENCE_TEST_VAR", "expanded_value");
        let result = expand_env_vars("prefix_${EMERGENCE_TEST_VAR}_suffix");
        assert_eq!(result, "prefix_expanded_value_suffix");
        std::env::remove_var("EMERGENCE_TEST_VAR");
    }

    /// Verifies that undefined variable placeholders remain unchanged rather than being replaced.
    #[test]
    fn test_missing_env_var_keeps_placeholder() {
        let result = expand_env_vars("${NONEXISTENT_VAR_XYZ_12345}");
        assert_eq!(result, "${NONEXISTENT_VAR_XYZ_12345}");
    }
}
