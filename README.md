# necessist

Runs tests with statements removed to help identify unnecessary statements

## Setup

```sh
cd necessist
cargo build
source env.sh
```

## Usage

1. Checkout a *clean* copy of your target repository. *This is important!*
1. `cd target-repo`
1. `necessist_instrument.sh`
1. `cargo necessist`

## Output

| Message  | Meaning (With the statement removed...) | Silenced with|
|-|-|-|
| <span style="color:green">passed</span> | The tests built and passed. | n/a |
| <span style="color:cyan">built</span> | The tests built but did not pass. | See below. |
| failed | The tests did not build. | `-qq` |
| <span style="color:yellow">skipped</span> | See below.  | `-q` |
| <span style="color:red">inconclusive</span> | An internal error (e.g., [rust-lang/rust#75734](https://github.com/rust-lang/rust/issues/75734)) prevented `necessist` from removing the statement. | n/a |

### Silencing <span style="color:cyan">built</span> messages

* Passing `-qqq` silences <span style="color:cyan">built</span> messages for all statements *except* local (let) bindings.
* Passing `-qqqq` silences <span style="color:cyan">built</span> messages entirely.

### Skipped statements

Necessist will not attempt to remove a statement if any of the following conditions apply.

* The statement is an invocation of [whitelisted macro](#whitelisted-macros).
* `--skip-locals` is passed and the statement is a local (let) binding.
* `--skip-calls regex` is passed and the statment is a function call, macro invocation, or method call matching `regex`.

#### Whitelisted macros
* `assert`
* `assert_eq`
* `assert_ne`
* `panic`
* `unimplemented`
* `unreachable`
