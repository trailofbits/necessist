# Necessist

Run tests with statements and method calls removed to help identify broken tests

Necessist currently supports Foundry, Go, Hardhat TS, and Rust.

**Contents**

- [Installation](#installation)
- [Overview](#overview)
- [Usage](#usage)
- [Details](#details)
- [Configuration files](#configuration-files)
- [Goals](#goals)
- [Limitations](#limitations)
- [License](#license)

## Installation

#### System requirements:

Install `pkg-config` and `sqlite3` development files on your system, e.g., on Ubuntu:

```sh
sudo apt install pkg-config libsqlite3-dev
```

#### Install Necessist from [crates.io]:

```sh
cargo install necessist
```

#### Install Necessist from [github.com]:

```sh
cargo install --git https://github.com/trailofbits/necessist --branch release
```

## Overview

Necessist iteratively removes statements and method calls from tests and then runs them. If a test passes with a statement or method call removed, it could indicate a problem in the test. Or worse, it could indicate a problem in the code being tested.

### Example

This example is from [`rust-openssl`]. The `verify_untrusted_callback_override_ok` test checks that a failed certificate validation can be overridden by a callback. But if the callback were never called (e.g., because of a failed connection), the test would still pass. Necessist reveals this fact by showing that the test passes without the call to `set_verify_callback`:

```rust
#[test]
fn verify_untrusted_callback_override_ok() {
    let server = Server::builder().build();

    let mut client = server.client();
    client
        .ctx()
        .set_verify_callback(SslVerifyMode::PEER, |_, x509| { //
            assert!(x509.current_cert().is_some());           // Test passes without this call
            true                                              // to `set_verify_callback`.
        });                                                   //

    client.connect();
}
```

Following this discovery, a flag was [added to the test] to record whether the callback is called. The flag must be set for the test to succeed:

```rust
#[test]
fn verify_untrusted_callback_override_ok() {
    static CALLED_BACK: AtomicBool = AtomicBool::new(false);  // Added

    let server = Server::builder().build();

    let mut client = server.client();
    client
        .ctx()
        .set_verify_callback(SslVerifyMode::PEER, |_, x509| {
            CALLED_BACK.store(true, Ordering::SeqCst);        // Added
            assert!(x509.current_cert().is_some());
            true
        });

    client.connect();
    assert!(CALLED_BACK.load(Ordering::SeqCst));              // Added
}
```

### Comparison to conventional mutation testing

Conventional mutation testing tries to identify _gaps in test coverage_, whereas Necessist tries to identify _bugs in existing tests_.

Conventional mutation testing tools (such a [`universalmutator`]) randomly inject faults into source code, and see whether the code's tests still pass. If they do, it could mean the code's tests are inadequate.

Notably, conventional mutation testing is about finding deficiencies in the set of tests as a whole, not in individual tests. That is, for any given test, randomly injecting faults into the code is not especially likely to reveal bugs in that test. This is unfortunate since some tests are more important than others, e.g., because ensuring the correctness of some parts of the code is more important than others.

By comparison, Necessist's approach of iteratively removing statements and method calls does target individual tests, and thus can reveal bugs in individual tests.

Of course, there is overlap is the sets of problems the two approaches can uncover, e.g., a failure to find an injected fault could indicate a bug in a test. Nonetheless, for the reasons just given, we see the two approaches as complementary, not competing.

## Usage

```
Usage: necessist [OPTIONS] [TEST_FILES]... [-- <ARGS>...]

Arguments:
  [TEST_FILES]...  Test files to mutilate (optional)
  [ARGS]...        Additional arguments to pass to each test command

Options:
      --allow <WARNING>        Silence <WARNING>; `--allow all` silences all warnings
      --default-config         Create a default necessist.toml file in the project's root directory
      --deny <WARNING>         Treat <WARNING> as an error; `--deny all` treats all warnings as errors
      --dump                   Dump sqlite database contents to the console
      --dump-candidates        Dump removal candidates and exit (for debugging)
      --framework <FRAMEWORK>  Assume testing framework is <FRAMEWORK> [possible values: auto, foundry, go, hardhat-ts, rust]
      --no-dry-run             Do not perform dry runs
      --no-sqlite              Do not output to an sqlite database
      --quiet                  Do not output to the console
      --reset                  Discard sqlite database contents
      --resume                 Resume from the sqlite database
      --root <ROOT>            Root directory of the project under test
      --timeout <TIMEOUT>      Maximum number of seconds to run any test; 60 is the default, 0 means no timeout
      --verbose                Show test outcomes besides `passed`
  -h, --help                   Print help
  -V, --version                Print version
```

### Output

By default, Necessist outputs to the console only when tests pass. Passing `--verbose` causes Necessist to instead output all of the removal outcomes below.

| Outcome                                      | Meaning (With the statement/method call removed...) |
| -------------------------------------------- | --------------------------------------------------- |
| <span style="color:red">passed</span>        | The test(s) built and passed.                       |
| <span style="color:yellow">timed-out</span>  | The test(s) built but timed-out.                    |
| <span style="color:green">failed</span>      | The test(s) built but failed.                       |
| <span style="color:blue">nonbuildable</span> | The test(s) did not build.                          |

By default, Necessist outputs to both the console and to an sqlite database. For the latter, a tool like [sqlitebrowser] can be used to filter/sort the results.

## Details

Generally speaking, Necessist will not attempt to remove a statement if it is one the following:

- a statement containing other statements (e.g., a `for` loop)
- a declaration (e.g., a local or `let` binding)
- a `break`, `continue`, or `return`
- the last statement in a test

Similarly, Necessist will not attempt to remove a method call if:

- It is the primary effect of an enclosing statement (e.g., `x.foo();`).
- It appears in the argument list of an ignored function, method, or macro ([see below](#configuration-files)).

Also, for some frameworks, certain statements and methods are ignored. Click on a framework to see its specifics.

<details>
<summary>Foundry</summary>

In addition to the below, the Foundry framework ignores:

- a statement immediately following a use of any form of `vm.expect` (e.g., `vm.expectRevert`)
- an `emit` statement

#### Ignored functions

- Anything beginning with `assert` (e.g., `assertEq`)
- Anything beginning with `vm.expect` (e.g., `vm.expectCall`)
- `vm.getLabel`
- `vm.label`

</details>

<details>
<summary>Go</summary>

In addition to the below, the Go framework ignores:

- Anything beginning with `assert.` (e.g., `assert.Equal`)
- Anything beginning with `require.` (e.g., `require.Equal`)
- `defer` statements

#### Ignored methods\*

- `Close`
- `Error`
- `Errorf`
- `Fail`
- `FailNow`
- `Fatal`
- `Fatalf`
- `Log`
- `Logf`
- `Parallel`

\* This list is based primarily on [`testing.T`]'s methods. However, some methods with commonplace names are omitted to avoid colliding with other types' methods.

</details>

<details>
<summary>Hardhat TS</summary>

#### Ignored functions

- `assert`
- Anything beginning with `assert.` (e.g., `assert.equal`)
- `expect`

#### Ignored methods

- `toNumber`
- `toString`

</details>

<details>
<summary>Rust</summary>

#### Ignored macros

- `assert`
- `assert_eq`
- `assert_matches`
- `assert_ne`
- `eprint`
- `eprintln`
- `panic`
- `print`
- `println`
- `unimplemented`
- `unreachable`

#### Ignored methods\*

- `as_bytes`
- `as_mut`
- `as_mut_os_str`
- `as_mut_os_string`
- `as_mut_slice`
- `as_mut_str`
- `as_os_str`
- `as_os_str_bytes`
- `as_path`
- `as_ref`
- `as_slice`
- `as_str`
- `borrow`
- `borrow_mut`
- `clone`
- `cloned`
- `copied`
- `deref`
- `deref_mut`
- `expect`
- `expect_err`
- `into_boxed_bytes`
- `into_boxed_os_str`
- `into_boxed_path`
- `into_boxed_slice`
- `into_boxed_str`
- `into_bytes`
- `into_os_string`
- `into_owned`
- `into_path_buf`
- `into_string`
- `into_vec`
- `iter`
- `iter_mut`
- `success`
- `to_os_string`
- `to_owned`
- `to_path_buf`
- `to_string`
- `to_vec`
- `unwrap`
- `unwrap_err`

\* This list is essentially the watched trait and inherent methods of Dylint's [`unnecessary_conversion_for_trait`] lint, with the following additions:

- `clone` (e.g. [`std::clone::Clone::clone`])
- `cloned` (e.g. [`std::iter::Iterator::cloned`])
- `copied` (e.g. [`std::iter::Iterator::copied`])
- `expect` (e.g. [`std::option::Option::expect`])
- `expect_err` (e.g. [`std::result::Result::expect_err`])
- `into_owned` (e.g. [`std::borrow::Cow::into_owned`])
- `success` (e.g. [`assert_cmd::assert::Assert::success`])
- `unwrap` (e.g. [`std::option::Option::unwrap`])
- `unwrap_err` (e.g. [`std::result::Result::unwrap_err`])

</details>

<p></p>

## Configuration files

A configuration file allows one to tailor Necessist's behavior with respect to a project. The file must be named `necessist.toml`, appear in the project's root directory, and be [toml] encoded. The file may contain one more of the options listed below.

- `ignored_functions`, `ignored_methods`, `ignored_macros`: A list of strings interpreted as [patterns]. A function, method, or macro (respectively) whose [path] matches a pattern in the list is ignored. Note that `ignored_macros` is used only by the Rust framework currently.

- `ignored_path_disambiguation`: One of the strings `Either`, `Function`, or `Method`. For a [path] that could refer to a function or method ([see below](#paths)), this option influences whether the function or method is ignored.

  - `Either` (default): Ignore if the path matches either an `ignored_functions` or `ignored_macros` pattern.

  - `Function`: Ignore only if the path matches an `ignored_functions` pattern.

  - `Method`: Ignore only if the path matches an `ignored_methods` pattern.

### Patterns

A pattern is a string composed of letters, numbers, `.`, `_`, or `*`. Each character, other than `*`, is treated literally and matches itself only. A `*` matches any string, including the empty string.

The following are examples of patterns:

- `assert`: matches itself only
- `assert_eq`: matches itself only
- `assertEqual`: matches itself only
- `assert.Equal`: matches itself only
- `assert.*`: matches `assert.Equal`, but not `assert`, `assert_eq`, or `assertEqual`
- `assert*`: matches `assert`, `assert_eq`, `assertEqual`, and `assert.Equal`
- `*.Equal`: matches `assert.Equal`, but not `Equal`

Notes:

- Patterns match [paths], not individual identifiers.
- `.` is treated literally like in a [`glob`] pattern, not like in regular expression.

### Paths

A path is a sequence of identifiers separated by `.`. Consider this example (from [Chainlink]):

```sol
operator.connect(roles.oracleNode).signer.sendTransaction({
    to: operator.address,
    data,
}),
```

In the above, `operator.connect` and `signer.sendTransaction` are paths.

Note, however, that paths like `operator.connect` are ambiguous:

- If `operator` refers to package or module, then `operator.connect` refers to a function.
- If `operator` refers to an object, then `operator.connect` refers to a method.

By default, Necessist ignores such a path if it matches either an `ignored_functions` or `ignored_macros` pattern. Setting the `ignored_path_disambiguation` option above to `Function` or `Method` causes Necessist ignore the path only if it matches an `ignored_functions` or `ignored_macros` pattern (respectively).

## Limitations

- **Slow.** Modifying tests requires them to be rebuilt. Running Necessist on even moderately sized codebases can take several hours.

- **Triage requires intimate knowledge of the source code.** Generally speaking, Necessist does not produce "obvious" bugs. In our experience, deciding whether a statement/method call should be necessary requires intimate knowledge of the code under test. Necessist is best run on codebases for which one has (or intends to have) such knowledge.

## Goals

- If a project uses a supported framework, then `cd`ing into the project's directory and typing `necessist` (with no arguments) should produce meaningful output.

## References

- Groce, A., Ahmed, I., Jensen, C., McKenney, P.E., Holmes, J.: How verified (or tested) is my code? Falsification-driven verification and testing. Autom. Softw. Eng. **25**, 917–960 (2018). A [preprint] is available. See Section 2.3.

## License

Necessist is licensed and distributed under the AGPLv3 license. [Contact us](mailto:opensource@trailofbits.com) if you're looking for an exception to the terms.

[`assert_cmd::assert::assert::success`]: https://docs.rs/assert_cmd/latest/assert_cmd/assert/struct.Assert.html#method.success
[Chainlink]: https://github.com/smartcontractkit/chainlink/blob/a39e54e157b57d5fc3dba0aed6ac9d58382953b2/contracts/test/v0.7/Operator.test.ts#L1725-L1728
[`glob`]: https://man7.org/linux/man-pages/man7/glob.7.html
[`rust-openssl`]: https://github.com/sfackler/rust-openssl
[`std::borrow::cow::into_owned`]: https://doc.rust-lang.org/std/borrow/enum.Cow.html#method.into_owned
[`std::clone::clone::clone`]: https://doc.rust-lang.org/std/clone/trait.Clone.html#tymethod.clone
[`std::iter::iterator::cloned`]: https://doc.rust-lang.org/std/iter/trait.Iterator.html#tymethod.cloned
[`std::iter::iterator::copied`]: https://doc.rust-lang.org/std/iter/trait.Iterator.html#tymethod.copied
[`std::option::option::expect`]: https://doc.rust-lang.org/std/option/enum.Option.html#method.expect
[`std::option::option::unwrap`]: https://doc.rust-lang.org/std/option/enum.Option.html#method.unwrap
[`std::result::result::expect_err`]: https://doc.rust-lang.org/std/result/enum.Result.html#method.expect_err
[`std::result::result::unwrap_err`]: https://doc.rust-lang.org/std/result/enum.Result.html#method.unwrap_err
[`testing.t`]: https://pkg.go.dev/testing#T
[`unnecessary_conversion_for_trait`]: https://github.com/trailofbits/dylint/tree/master/examples/supplementary/unnecessary_conversion_for_trait
[added to the test]: https://github.com/sfackler/rust-openssl/pull/1852
[crates.io]: https://crates.io/crates/necessist
[github.com]: https://github.com/trailofbits/necessist
[path]: #paths
[paths]: #paths
[patterns]: #patterns
[preprint]: https://agroce.github.io/asej18.pdf
[sqlitebrowser]: https://sqlitebrowser.org/
[toml]: https://toml.io/en/
[`universalmutator`]: https://github.com/agroce/universalmutator
