use serde_json::Value;

type IsAcceptableResultFn = fn(&Value) -> bool;

pub(crate) fn contains_acceptable_result(findings: &[Value], leads: &[Value]) -> bool {
    let acceptable_result_fns: &[IsAcceptableResultFn] = &[
        is_acceptable_done_result,
        is_acceptable_hello_result,
        is_acceptable_crlf_oracle_result,
    ];

    [findings, leads].into_iter().any(|results| {
        acceptable_result_fns
            .iter()
            .any(|&is_acceptable_result| results.iter().any(is_acceptable_result))
    })
}

// The paper highlighted this `<-done` removal.
fn is_acceptable_done_result(value: &Value) -> bool {
    let removed_code = value
        .get("removed_code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !removed_code.contains("<-done") {
        return false;
    }

    assert_json_string_contains(value, "removed_code", "<-done");
    assert_json_string_contains(value, "removed_location", "smtp_test.go");
    assert_json_string_contains(value, "affected_location", "smtp_test.go");

    let text = json_text(value);
    assert!(
        text.contains("traffic") || text.contains("SMTP") || text.contains("goroutine"),
        "finding did not include SMTP traffic/goroutine details: {value:#}"
    );
    true
}

// Current LLMs may also report `Hello("customhost")` in `TestHello` case 8:
// https://github.com/golang/go/blob/9a0a82445650eebedf5633fdfe6e73b5836dc5c9/src/net/smtp/smtp_test.go#L474-L481
// That is also a valid test-harness finding, though it was not called out in the
// original paper.
//
// The test appears intended to check `Client.Hello`'s documented ordering
// contract: `Hello` must be called before any other method, and the implementation
// rejects late calls once `didHello` is set:
// https://github.com/golang/go/blob/9a0a82445650eebedf5633fdfe6e73b5836dc5c9/src/net/smtp/smtp.go#L96-L105
//
// ```go
// err = c.Verify("test@example.com")
// if err != nil {
//     err = c.Hello("customhost")
//     if err != nil {
//         t.Errorf("Want error, got none")
//     }
// }
// ```
//
// In the fixture, `Verify` succeeds, so the branch is unreachable and the
// `Hello` call is never exercised. Removing it therefore leaves the test passing
// and exposes that the harness would not catch a regression where `Hello`
// incorrectly succeeds after another method. The intended shape is more like:
//
// ```go
// if err := c.Verify("test@example.com"); err != nil {
//     t.Fatalf("Verify failed: %v", err)
// }
// if err := c.Hello("customhost"); err == nil {
//     t.Errorf("Want error, got none")
// }
// ```
fn is_acceptable_hello_result(value: &Value) -> bool {
    if !json_text(value).contains("Hello(\"customhost\")") {
        return false;
    }

    assert_json_string_contains(value, "removed_code", "Hello(\"customhost\")");
    assert_json_string_contains(value, "removed_location", "smtp_test.go");
    assert_json_string_contains(value, "affected_location", "smtp_test.go");

    let text = json_text(value);
    assert!(
        text.contains("Verify") && text.contains("Hello"),
        "finding did not include Verify/Hello details: {value:#}"
    );
    true
}

// Current LLMs may also report the `TestBasic` CR/LF injection checks:
// https://github.com/golang/go/blob/9a0a82445650eebedf5633fdfe6e73b5836dc5c9/src/net/smtp/smtp_test.go#L197-L217
// Those are also valid test-harness findings, though they were not called out in
// the original paper.
//
// The test appears intended to check that `Client.Verify`, `Client.Rcpt`, and
// `Client.Mail` reject CR/LF command injection before sending SMTP commands:
// https://github.com/golang/go/blob/9a0a82445650eebedf5633fdfe6e73b5836dc5c9/src/net/smtp/smtp.go#L185-L188
// https://github.com/golang/go/blob/9a0a82445650eebedf5633fdfe6e73b5836dc5c9/src/net/smtp/smtp.go#L246-L249
// https://github.com/golang/go/blob/9a0a82445650eebedf5633fdfe6e73b5836dc5c9/src/net/smtp/smtp.go#L266-L269
//
// ```go
// if err := c.Verify("user2@gmail.com>\r\nDATA\r\nAnother injected message body\r\n.\r\nQUIT\r\n"); err == nil {
//     t.Fatalf("VRFY should have failed due to a message injection attempt")
// }
// if err := c.Rcpt("golang-nuts@googlegroups.com>\r\nDATA\r\nInjected message body\r\n.\r\nQUIT\r\n"); err == nil {
//     t.Fatalf("RCPT should have failed due to a message injection attempt")
// }
// if err := c.Mail("user@gmail.com>\r\nDATA\r\nAnother injected message body\r\n.\r\nQUIT\r\n"); err == nil {
//     t.Fatalf("MAIL should have failed due to a message injection attempt")
// }
// ```
//
// Necessist can remove the selector-call suffix from those short initializer
// expressions. The remaining initializer is the non-`nil` client `c`, so
// `err == nil` is false, the failure branch is skipped, and the malicious input
// is never passed to `Verify`, `Rcpt`, or `Mail`. The SMTP implementation still
// calls `validateLine`; the defect is that the test oracle can pass without
// exercising those security checks.
fn is_acceptable_crlf_oracle_result(value: &Value) -> bool {
    let removed_code = value
        .get("removed_code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !["Verify", "Rcpt", "Mail"]
        .iter()
        .any(|method| removed_code.contains(&format!(".{method}(\"")))
    {
        return false;
    }

    assert_json_string_contains(value, "removed_code", "\\r\\n");
    assert_json_string_contains(value, "removed_location", "smtp_test.go");
    assert_json_string_contains(value, "affected_location", "smtp_test.go");

    let text = json_text(value);
    assert!(
        (text.contains("CR/LF") || text.contains("injection"))
            && (text.contains("Verify") || text.contains("Rcpt") || text.contains("Mail")),
        "finding did not include CR/LF injection oracle details: {value:#}"
    );
    true
}

fn assert_json_string_contains(value: &Value, key: &str, needle: &str) {
    let haystack = value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("finding missing string field `{key}`"));
    assert!(
        haystack.contains(needle),
        "expected `{key}` to contain `{needle}`, got `{haystack}`"
    );
}

fn json_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(bool) => bool.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(string) => string.clone(),
        Value::Array(array) => array.iter().map(json_text).collect::<Vec<_>>().join("\n"),
        Value::Object(object) => object
            .values()
            .map(json_text)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
