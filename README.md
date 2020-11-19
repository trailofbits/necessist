# Necessist

Runs tests with statements removed to help identify unnecessary statements

## Setup

```sh
cd necessist
cargo build --workspace
source env.sh
```

## Usage

1. Checkout a *clean* copy of your target repository. *This is important!*
1. `cd target-repo`
1. `necessist_instrument.sh`
1. `cargo necessist`

By default, necessist outputs to the console. Passing `--sqlite` causes necessist to instead output to a sqlite database. A tool like [sqlitebrowser](https://sqlitebrowser.org/) can then be used to filter/sort the results.

## Output

| Result  | Meaning (With the statement removed...) | Silenced with|
|-|-|-|
| <span style="color:green">passed</span> | The tests built and passed. | n/a |
| <span style="color:cyan">timed-out</span> | The tests built but timed-out. | See below. |
| <span style="color:blue">failed</span> | The tests built but failed. | See below. |
| nonbuildable | The tests did not build. | `-qq` |
| <span style="color:yellow">skipped</span> | See below.  | `-q` |
| <span style="color:red">inconclusive</span> | An internal error (e.g., [rust-lang/rust#75734](https://github.com/rust-lang/rust/issues/75734)) prevented necessist from removing the statement. | n/a |

### Silencing <span style="color:cyan">timed-out</span> and <span style="color:blue">failed</span> results

* Passing `-qqq` silences <span style="color:cyan">timed-out</span> and <span style="color:blue">failed</span> results for all statements *except* local (let) bindings.
* Passing `-qqqq` silences <span style="color:cyan">timed-out</span> and <span style="color:blue">failed</span> results entirely.

### Skipped statements

Necessist will not attempt to remove a statement if any of the following conditions apply.

* The statement is an invocation of a [whitelisted macro](#whitelisted-macros).
* `--skip-calls regex` is passed and the statement is a function call, macro invocation, or method call matching `regex`.
* `--skip-controls` is passed and the statement is a `break` or `continue`.
* `--skip-locals` is passed and the statement is a local (`let`) binding.

#### Whitelisted macros
* `assert`
* `assert_eq`
* `assert_ne`
* `panic`
* `unimplemented`
* `unreachable`

## References

* Groce, A., Ahmed, I., Jensen, C., McKenney, P.E., Holmes, J.: How verified (or tested) is my code? Falsification-driven verification and testing. Autom. Softw. Eng. **25**, 917–960 (2018). A preprint is available [here](https://agroce.github.io/asej18.pdf). See Section 2.3.
