mod end_to_end_common;

const PATH: &str = "tests/end_to_end_tests/0";

#[cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]
#[test]
fn all_tests() {
    end_to_end_common::all_tests_in(PATH);
}

#[test]
fn stdout_files_are_sanitary() {
    end_to_end_common::stdout_files_are_sanitary_in(PATH);
}

#[test]
fn stdout_subsequence() {
    end_to_end_common::stdout_subsequence_in(PATH);
}
