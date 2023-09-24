# Necessist

Run tests with statements and method calls removed to help identify broken tests

Necessist currently supports Anchor (TS), Foundry, Go, Hardhat (TS), and Rust.

**Contents**

- [Installation](#installation)
- [Overview](#overview)
- [Usage](#usage)
- [Details](#details)
- [Configuration files](#configuration-files)
- [Limitations](#limitations)
- [Semantic versioning policy](#semantic-versioning-policy)
- [Goals](#goals)
- [References](#references)
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

<details>
<summary>Click to expand</summary>

Conventional mutation testing tries to identify _gaps in test coverage_, whereas Necessist tries to identify _bugs in existing tests_.

Conventional mutation testing tools (such a [`universalmutator`]) randomly inject faults into source code, and see whether the code's tests still pass. If they do, it could mean the code's tests are inadequate.

Notably, conventional mutation testing is about finding deficiencies in the set of tests as a whole, not in individual tests. That is, for any given test, randomly injecting faults into the code is not especially likely to reveal bugs in that test. This is unfortunate since some tests are more important than others, e.g., because ensuring the correctness of some parts of the code is more important than others.

By comparison, Necessist's approach of iteratively removing statements and method calls does target individual tests, and thus can reveal bugs in individual tests.

Of course, there is overlap is the sets of problems the two approaches can uncover, e.g., a failure to find an injected fault could indicate a bug in a test. Nonetheless, for the reasons just given, we see the two approaches as complementary, not competing.

</details>

### Theoretical motivation

<details>
<summary>Click to expand</summary>

The following criterion (`*`) comes close to describing the statements that Necessist aims to remove:

- (`*`) Statement `S`'s [weakest precondition] `P` has the same context (e.g., variables in scope) as `S`'s postcondition `Q`, and `P` does not imply `Q`.

The notion that (`*`) tries to capture is: a statement that affects a subsequently asserted condition. In this section, we explain and motivate this choice, and briefly discuss alternatives. For concision, we focus on statements, but the remarks in this section apply to method calls as well.

Consider a test through the lens of [predicate transformer semantics]. A test is a function with no inputs or outputs. Thus, an alternative procedure for determining whether a test passes is the following. Starting with `True`, iteratively work backwards through the test's statements, computing the weakest precondition of each. If the precondition arrived at for the test's first statement is `True`, then the test passes. If the precondition is `False`, the test fails.

Now, imagine we were to apply this procedure, and consider a statement `S` that violates (`*`). We argue that it might not make sense to remove `S`:

- If `S` adds or removes variables from the scope (e.g., `S` is a declaration), or `S` changes a variable's type, then removing `S` would likely result in a compilation failure. (On top of that, since `S`'s precondition and postcondition have different contexts, it's not clear how to compare them.)

- If `S`'s precondition is stronger than its postcondition (e.g., `S` an `assert`), then `S` imposes constraints on the environments in which it executes. Put another way, `S` _tests_ something. Thus, removing `S` would likely detract from the overarching test's purpose.

Conversely, consider a statement `S` that satisfies (`*`). Here is why it might make sense to remove `S`. Think of `S` as _shifting_ the set of valid environments, rather than constraining them. More precisely, if `S`'s weakest precondition `P` does not imply `Q`, and if `Q` is satisfiable, the there is an assignment to `P` and `Q`'s free variables that satisfies both `P` and `Q`. If such an assignment results from each environment in which `S` is actually executed, then the necessity of `S` is called into question.

The main utility of (`*`) is in helping to select the statements that Necessist ignores. That is, if we imagine a predicate transformer semantics for one of Necessist's supported languages, and a statement `S` in that language, we can ask: would `S` satisfy (`*`)? If not, then then Necessist should likely ignore `S`.

But (`*`) has other nice consequences. For example, the rule that the last statement in a test should be ignored follows from (`*`). To see this, note the such a statement's postcondition `Q` is always `True`. Thus, if the statement doesn't change the context, then its weakest precondition necessarily implies `Q`.

Having said all this, (`*`) doesn't quite capture what Necessist actually _does_. Consider a statement like `x -= 1`. Necessist will remove such a statement unconditionally, but (`*`) says maybe Necessist shouldn't. Assuming [overflow checks] are enabled, computing this statement's weakest precondition would look something like the following:

```
{ Q[(x - 1)/x] ^ x >= 1 }
x -= 1;
{ Q }
```

Note that `x -= 1` does not change the context, and that `Q[(x - 1)/x] ^ x >= 1` could imply `Q`. For example, if `Q` does not contain `x`, then `Q[(x - 1)/x] = Q` and `Q ^ x >= 1` implies `Q`.

A question one can then ask is: _should_ Necessist remove this statement? Put another way, should Necessist's current behavior be adjusted, or should (`*`) be adjusted?

One way to look at this question is: which statements are worth removing, i.e., which statements are "interesting?" As implied above, (`*`) considers a statement "interesting" if it affects a subsequently asserted condition. Agreeing with this notion and that (`*`) adequately captures it are reasons to keep (`*`) and adjust Necessist's behavior.

But there are other possible, useful definitions of "interesting statement" upon which one could base an argument for adjusting (`*`). The following example is due to @2over12. Instead of weakest preconditions, one could consider [strongest postconditions]. For example, computing the strongest postcondition of `x -= 1` would look something like the following:

```
{ P }
x -= 1;
{ (exists x')[P[x'/x] ^ x' >= 1 ^ x = x' - 1] }
```

One could then consider a statement "interesting" if its strongest postcondition contains "interesting clauses" as determined by heuristics. @2over12 notes that a common source of bugs in tests is unintended side effects (e.g., if `x -= 1` were unintended). As already noted, (`*`) might not catch such bugs, but the just mentioned strongest postcondition scheme might.

Other possible, useful definitions of "interesting statement" could involve frameworks besides [Hoare logic] entirely.

To be clear, Necessist does not apply (`*`) formally, e.g., Necessist does not actually compute weakest preconditions. The current role of (`*`) is to help guide which statements Necessist should ignore, and (`*`) seems to do well in that role. As such, we leave revision of (`*`) to future work.

</details>

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
      --framework <FRAMEWORK>  Assume testing framework is <FRAMEWORK> [possible values: anchor-ts, auto, foundry, go, hardhat-ts, rust]
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
<summary>Anchor TS</summary>

#### Ignored functions

- `assert`
- Anything beginning with `assert.` (e.g., `assert.equal`)
- Anything beginning with `console.` (e.g., `console.log`)
- `expect`

#### Ignored methods

- `toNumber`
- `toString`

</details>

<details>
<summary>Foundry</summary>

In addition to the below, the Foundry framework ignores:

- a statement immediately following a use of `vm.prank` or any form of `vm.expect` (e.g., `vm.expectRevert`)
- an `emit` statement

#### Ignored functions

- Anything beginning with `assert` (e.g., `assertEq`)
- Anything beginning with `vm.expect` (e.g., `vm.expectCall`)
- Anything beginning with `console.log` (e.g., `console.log`, `console.logInt`)
- Anything beginning with `console2.log` (e.g., `console2.log`, `console2.logInt`)
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
- `Skip`
- `Skipf`
- `SkipNow`

\* This list is based primarily on [`testing.T`]'s methods. However, some methods with commonplace names are omitted to avoid colliding with other types' methods.

</details>

<details>
<summary>Hardhat TS</summary>

The ignored functions and methods are the same as for Anchor TS above.

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
- `into_os_str_bytes`
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

  - `Either` (default): Ignore if the path matches either an `ignored_functions` or `ignored_methods` pattern.

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

By default, Necessist ignores such a path if it matches either an `ignored_functions` or `ignored_methods` pattern. Setting the `ignored_path_disambiguation` option above to `Function` or `Method` causes Necessist ignore the path only if it matches an `ignored_functions` or `ignored_methods` pattern (respectively).

## Limitations

- **Slow.** Modifying tests requires them to be rebuilt. Running Necessist on even moderately sized codebases can take several hours.

- **Triage requires intimate knowledge of the source code.** Generally speaking, Necessist does not produce "obvious" bugs. In our experience, deciding whether a statement/method call should be necessary requires intimate knowledge of the code under test. Necessist is best run on codebases for which one has (or intends to have) such knowledge.

## Semantic versioning policy

We reserve the right to change what syntax Necessist ignores by default, and to consider such changes non-breaking.

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
[configuration file]: #configuration-files
[crates.io]: https://crates.io/crates/necessist
[github.com]: https://github.com/trailofbits/necessist
[Hoare logic]: https://en.wikipedia.org/wiki/Hoare_logic
[overflow checks]: https://doc.rust-lang.org/rustc/codegen-options/index.html#overflow-checks
[path]: #paths
[paths]: #paths
[patterns]: #patterns
[predicate transformer semantics]: https://en.wikipedia.org/wiki/Predicate_transformer_semantics
[preprint]: https://agroce.github.io/asej18.pdf
[sqlitebrowser]: https://sqlitebrowser.org/
[toml]: https://toml.io/en/
[`universalmutator`]: https://github.com/agroce/universalmutator
[strongest postconditions]: https://en.wikipedia.org/wiki/Predicate_transformer_semantics#Strongest_postcondition
[weakest precondition]: https://en.wikipedia.org/wiki/Predicate_transformer_semantics#Weakest_preconditions
