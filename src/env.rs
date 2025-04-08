#[macro_export]
macro_rules! env {
    ($key:expr) => {{
        match std::env::var($key) {
            Ok(val) => val,
            Err(_) => match std::fs::read_to_string(".env") {
                Ok(content) => {
                    let mut result = String::new();
                    for line in content.lines() {
                        if line.starts_with(&format!("{}=", $key)) {
                            if let Some(val) = line.splitn(2, '=').nth(1) {
                                result = val.to_string();
                                break;
                            }
                        }
                    }
                    if result.is_empty() {
                        panic!("environment variable '{}' not found in .env file", $key);
                    }
                    result
                }
                Err(_) => panic!(
                    "failed to read .env file and environment variable '{}' not set",
                    $key
                ),
            },
        }
    }};
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_env_macro() {
        assert_eq!(env!("TEST_VAR"), "test_value");
    }
}
