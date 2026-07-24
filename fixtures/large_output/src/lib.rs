#[cfg(test)]
mod tests {
    use std::io::{Write, stderr, stdout};

    #[test]
    fn large_stdout() {
        let mut n = 0;
        n += 1;
        let output = vec![b'x'; 1_000_000];
        let stdout_result = stdout().write_all(&output);
        assert!(stdout_result.is_ok());
        assert_eq!(n, 1);
    }

    #[test]
    fn large_stderr() {
        let mut n = 0;
        n += 2;
        let output = vec![b'x'; 1_000_000];
        let stderr_result = stderr().write_all(&output);
        assert!(stderr_result.is_ok());
        assert_eq!(n, 2);
    }
}
