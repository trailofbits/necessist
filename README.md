# Necessist

Runs tests with statements and method calls removed to help identify broken tests

```
cargo install necessist
```

## Usage

By default, Necessist outputs to the console. Passing `--sqlite` causes Necessist to instead output to a sqlite database. A tool like [sqlitebrowser](https://sqlitebrowser.org/) can then be used to filter/sort the results.

Generally speaking, Necessist will not attempt to remove a statement if it is one the following:

- A local declaration (e.g., `let` binding)
- A `break` or `continue`

Also, for some frameworks, certain statements and methods are whitelisted (see [below](#supported-framework-specifics)).

## Output

By default, Necessist outputs only when tests pass. Passing `--verbose` causes Necessist to instead output all of the removal outcomes below.

| Outcome                                      | Meaning (With the statement removed...) |
| -------------------------------------------- | --------------------------------------- |
| <span style="color:red">passed</span>        | The test(s) built and passed.           |
| <span style="color:yellow">timed-out</span>  | The test(s) built but timed-out.        |
| <span style="color:green">failed</span>      | The test(s) built but failed.           |
| <span style="color:blue">nonbuildable</span> | The test(s) did not build.              |

## Supported frameworks

- [Hardhat TS](#hardhat-ts)
- [Rust](#rust)

## Supported framework specifics

### Hardhat TS

TBD

### Rust

#### Whitelisted macros

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

#### Whitelisted methods

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
- `success` (e.g. [`assert_cmd::assert::Assert::success`](https://docs.rs/assert_cmd/latest/assert_cmd/assert/struct.Assert.html#method.success))
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

## Goals

- If a project uses a [supported framework](#supported-frameworks), then `cd`ing into its directory and typing `necessist` (with no arguments) ought to produce meaningful output.

## References

- Groce, A., Ahmed, I., Jensen, C., McKenney, P.E., Holmes, J.: How verified (or tested) is my code? Falsification-driven verification and testing. Autom. Softw. Eng. **25**, 917–960 (2018). A preprint is available [here](https://agroce.github.io/asej18.pdf). See Section 2.3.
