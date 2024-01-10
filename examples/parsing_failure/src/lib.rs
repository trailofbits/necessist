// smoelius: Based on:
// https://github.com/rust-lang/rust-clippy/blob/b6882f6107193696e14ab21fb0ee9921a4e0b842/tests/ui/needless_return.rs#L286-L288

#[test]
fn issue9947() -> Result<(), String> {
    do yeet "hello";
}
