# Necessist

Run tests with statements and method calls removed to help identify broken tests

```
cargo install necessist
```

## Overview

The following hypothetical test verifies that a login mechanism works. Suppose the test would pass if `session.send_password(...)` were removed. This could indicate that the wrong condition is checked thereafter. Or worse, it could indicate a bug in the login mechanism.

```rust
#[test]
fn login_works() {
    let session = Session::new(URL);
    session.send_username(USERNAME).unwrap();
    session.send_password(PASSWORD).unwrap(); // <-- Test passes without this
    assert!(session.read().unwrap().contains(WELCOME));
}
```

Necessist iteratively removes statements and method calls from tests and then runs them to help identify such cases.

Generally speaking, Necessist will not attempt to remove a statement if it is one the following:

- A statement containing other statements (e.g., a `for` loop)
- A declaration (e.g., a local or `let` binding)
- A `break`, `continue`, or `return`

Also, for some frameworks, certain statements and methods are ignored (see [below](#supported-framework-specifics)).

## Usage

```
necessist 0.1.0-beta.1

USAGE:
    necessist [OPTIONS] [TEST_FILES]...

ARGS:
    <TEST_FILES>...    Test files to mutilate (optional)

OPTIONS:
        --allow <WARNING>          Silence <WARNING>; `--allow all` silences all warnings
        --default-config           Create a default necessist.toml file in the project's root
                                   directory (experimental)
        --deny <WARNING>           Treat <WARNING> as an error; `--deny all` treats all warnings as
                                   errors
        --dump                     Dump sqlite database contents to the console
        --framework <FRAMEWORK>    Assume testing framework is <FRAMEWORK> [possible values: auto,
                                   hardhat-ts, rust]
    -h, --help                     Print help information
        --no-dry-run               Do not perform dry runs
        --no-sqlite                Do not output to an sqlite database
        --quiet                    Do not output to the console
        --reset                    Discard sqlite database contents
        --resume                   Resume from the sqlite database
        --root <ROOT>              Root directory of the project under test
        --timeout <TIMEOUT>        Maximum number of seconds to run any test; 60 is the default, 0
                                   means no timeout
    -V, --version                  Print version information
        --verbose                  Show test outcomes besides `passed`
```

By default, Necessist outputs to both the console and to an sqlite database. For the latter, a tool like [sqlitebrowser](https://sqlitebrowser.org/) can be used to filter/sort the results.

## Output

By default, Necessist outputs only when tests pass. Passing `--verbose` causes Necessist to instead output all of the removal outcomes below.

| Outcome                                      | Meaning (With the statement/method call removed...) |
| -------------------------------------------- | --------------------------------------------------- |
| <span style="color:red">passed</span>        | The test(s) built and passed.                       |
| <span style="color:yellow">timed-out</span>  | The test(s) built but timed-out.                    |
| <span style="color:green">failed</span>      | The test(s) built but failed.                       |
| <span style="color:blue">nonbuildable</span> | The test(s) did not build.                          |

## Supported frameworks

- [Hardhat TS](#hardhat-ts)
- [Rust](#rust)

## Supported framework specifics

### Hardhat TS

#### Ignored functions

- Anything beginning with `assert` (e.g., `assert.equal`)
- `expect`

#### Ignored methods

- Anything beginning with `should` (e.g., `should.equal`)
- Anything beginning with `to` (e.g., `to.equal`)
- `toNumber`
- `toString`

### Rust

#### Ignored macros

- `assert`
- `assert_eq`
- `assert_ne`
- `eprint`
- `eprintln`
- `panic`
- `print`
- `println`
- `unimplemented`
- `unreachable`

#### Ignored methods

- `as_bytes`
- `as_bytes_mut`
- `as_mut`
- `as_mut_ptr`
- `as_os_str`
- `as_path`
- `as_ptr`
- `as_ref`
- `as_slice`
- `as_str`
- `borrow`
- `borrow_mut`
- `clone`
- `cloned`
- `copied`
- `deref`
- `into`
- `into_os_string`
- `into_owned`
- `into_path_buf`
- `into_string`
- `into_vec`
- `success` (e.g. [`assert_cmd::assert::Assert::success`])
- `to_os_string`
- `to_owned`
- `to_path_buf`
- `to_str`
- `to_string`
- `to_string_lossy`
- `to_vec`
- `try_into`
- `unwrap`
- `unwrap_err`
- `unwrap_or_default`

## Configuration files (experimental)

**Configuration files are experimental and their behavior could change at any time.**

A configuration file allows one to tailor Necessist's behavior with respect to a project. The file must be named `necessist.toml`, appear in the project's root directory, and be [toml] encoded. The file may contain one more of the options listed below.

### Hardhat TS configuration options

- `ignored_functions`: A list of strings. Functions whose names appear in the list are ignored.

### Rust configuration options

- `ignored_macros`: A list of strings. Macros whose names appear in the list are ignored.

## Goals

- If a project uses a [supported framework](#supported-frameworks), then `cd`ing into the project's directory and typing `necessist` (with no arguments) should produce meaningful output.

## References

- Groce, A., Ahmed, I., Jensen, C., McKenney, P.E., Holmes, J.: How verified (or tested) is my code? Falsification-driven verification and testing. Autom. Softw. Eng. **25**, 917–960 (2018). A [preprint] is available. See Section 2.3.

[`assert_cmd::assert::assert::success`]: https://docs.rs/assert_cmd/latest/assert_cmd/assert/struct.Assert.html#method.success
[preprint]: https://agroce.github.io/asej18.pdf
[toml]: https://toml.io/en/
