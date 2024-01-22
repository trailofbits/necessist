// smoelius: Based on: https://doc.rust-lang.org/rust-by-example/macros.html

macro_rules! say_hello {
    () => {
        println!("Hello!")
    };
}

#[test]
fn test() {
    say_hello!();
}
