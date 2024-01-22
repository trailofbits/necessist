// smoelius: Based on: https://doc.rust-lang.org/rust-by-example/macros.html

macro_rules! say_hello {
    () => {
        println!("Hello!")
    };
}

struct S {
    field: T,
}

impl S {
    fn method(&self) -> U {
        U
    }
    fn ignored_method(&self) -> U {
        U
    }
}

struct T;

impl T {
    fn method(&self) -> U {
        U
    }
    fn ignored_method(&self) -> U {
        U
    }
}

struct U;

impl U {
    fn baz(&self) {}
}

fn foo() -> S {
    S { field: T }
}

#[test]
fn test() {
    say_hello!();

    let bar = foo();

    foo().method();
    foo().method().baz();
    foo().ignored_method();
    foo().ignored_method().baz();

    bar.method();
    bar.method().baz();
    bar.ignored_method();
    bar.ignored_method().baz();

    bar.field.method();
    bar.field.method().baz();
    bar.field.ignored_method();
    bar.field.ignored_method().baz();
}
