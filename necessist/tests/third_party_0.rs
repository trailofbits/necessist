mod third_party_common;

const PATH: &str = "tests/third_party_tests/0";

#[cfg_attr(
    dylint_lib = "non_thread_safe_call_in_test",
    allow(non_thread_safe_call_in_test)
)]
#[test]
fn all_tests() {
    third_party_common::all_tests_in(PATH);
}

#[test]
fn stdout_subsequence() {
    third_party_common::stdout_subsequence_in(PATH);
}
