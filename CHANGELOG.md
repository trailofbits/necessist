# Changelog

## 2.0.0

- Update `strum` to version 0.27 ([#1439](https://github.com/trailofbits/necessist/pull/1439))
- Update `strum_macros` to version 0.27 ([#1442](https://github.com/trailofbits/necessist/pull/1442))
- Fix a bug that caused TypeScript files with tab characters to be mishandled ([#1464](https://github.com/trailofbits/necessist/pull/1464))
- Ignore `throw` statements in Mocha-based tests ([efae165](https://github.com/trailofbits/necessist/commit/efae16577f99f358a1025d5a17710a318e4e0a52))
- Peel `FunctionCallBlock` expressions in Foundry backend ([36c4855](https://github.com/trailofbits/necessist/commit/36c4855d81a23a59ffb5cc945e426cfa7cc912d0))
- BREAKING CHANGE: Eliminate `--no-dry-run` option. During a dry run, Necessist records which tests execute which spans. This information facilitates testing span removals, because only the test(s) relevant to a span must be executed. ([#1472](https://github.com/trailofbits/necessist/pull/1472))
- Add experimental `--dump-candidate-counts` option ([#1468](https://github.com/trailofbits/necessist/pull/1468))
- Update `libsqlite3-sys` to version 0.31 ([#1479](https://github.com/trailofbits/necessist/pull/1479))
- Add support for the Vitest framework ([#1475](https://github.com/trailofbits/necessist/pull/1475))
- Update `swc_core` to version 0.16 ([#1478](https://github.com/trailofbits/necessist/pull/1478))

## 1.0.4

- Upgrade `tree-sitter` to version 0.25 ([#1435](https://github.com/trailofbits/necessist/pull/1435))

## 1.0.3

- Upgrade `itertools` to version 0.14 ([#1426](https://github.com/trailofbits/necessist/pull/1426))
- Upgrade `swc_core` to version 12 ([#1427](https://github.com/trailofbits/necessist/pull/1427))

## 1.0.2

- Upgrade `git2` to version 0.20 ([#1409](https://github.com/trailofbits/necessist/pull/1409))
- Change "Test Harness Mutilation" paper url. The `ieeexplore.ieee.org` url was returning invalid HTTP response codes. ([#1423](https://github.com/trailofbits/necessist/pull/1423))
- Upgrade `swc_core` to version 11 ([#1422](https://github.com/trailofbits/necessist/pull/1422))

## 1.0.1

- Upgrade `tree-sitter-go` to version 0.23.4 ([#1371](https://github.com/trailofbits/necessist/pull/1371))
- Upgrade `swc_core` to version 9 ([#1376](https://github.com/trailofbits/necessist/pull/1376) and [#1393](https://github.com/trailofbits/necessist/pull/1393))
- Refine regular expression used to identify times in Mocha logs ([#1378](https://github.com/trailofbits/necessist/pull/1378))
- Allow Mocha test files to have `.js` extensions ([#1379](https://github.com/trailofbits/necessist/pull/1379))
- Upgrade `cargo_metadata` to version 0.19 ([#1388](https://github.com/trailofbits/necessist/pull/1388))

## 1.0.0

- Fix caching in the Rust backend. The backend was recomputing data that was supposed to be cached. ([#1343](https://github.com/trailofbits/necessist/pull/1343))
- Ignore [`log`](https://crates.io/crates/log) macros (`debug!`, `error!`, `info!`, `trace!`, and `warn!`) in the Rust backend ([#1344](https://github.com/trailofbits/necessist/pull/1344))
- Update `swc_core` to version 4 ([#1345](https://github.com/trailofbits/necessist/pull/1345))
- BREAKING CHANGE: Make walking local functions opt-in rather than the default. Version 0.7.0 made walking local functions the default. However, this caused problems in languages such as Rust, where test and non-test functions could be declared within the same file. Specifically, the non-test functions would be walked, creating unnecessary noise. [PR #1351](https://github.com/trailofbits/necessist/pull/1351) requires users to name the functions that should be walked, rather than assume they all should be. ([#1351](https://github.com/trailofbits/necessist/pull/1351))

## 0.7.1

- Update `tree-sitter` to version 0.24 ([#1326](https://github.com/trailofbits/necessist/pull/1326))
- Update `swc_core` to version 1.0 ([#1327](https://github.com/trailofbits/necessist/pull/1327))
- Update documentation ([#1328](https://github.com/trailofbits/necessist/pull/1328))
- Fix a bug in the Anchor backend causing it to rebuild only Rust source files and not TypeScript source files ([cf36b40](https://github.com/trailofbits/necessist/commit/cf36b40ce2b11e59cf427ae8f86f1eef86c8f06d))

## 0.7.0

- Update `libsqlite3-sys` to version 0.30 ([#1260](https://github.com/trailofbits/necessist/pull/1260))
- Update `swc_core` to version 0.102 ([#1263](https://github.com/trailofbits/necessist/pull/1263))
- Do not consider `TestMain` a test in Go backend ([#1266](https://github.com/trailofbits/necessist/pull/1266))
- Add `Helper` as an ignored method in the Go backend ([#1276](https://github.com/trailofbits/necessist/pull/1276))
- FEATURE: Walk functions that are declared within the same files as the tests that call them ([#1268](https://github.com/trailofbits/necessist/pull/1268))
- Update `tree-sitter` and `tree-sitter-go` to version 0.23 ([#1279](https://github.com/trailofbits/necessist/pull/1279))

## 0.6.4

- Update `windows-sys` to version 0.59 ([#1231](https://github.com/trailofbits/necessist/pull/1231))
- Update `libsqlite3-sys` to version 0.29 ([#1233](https://github.com/trailofbits/necessist/pull/1233))
- Update `swc_core` to version 0.101 ([#1246](https://github.com/trailofbits/necessist/pull/1246))

## 0.6.3

- FEATURE: When a test cannot be run, show why ([#1201](https://github.com/trailofbits/necessist/pull/1201))
- Update `swc_core` to version 0.99 ([#1204](https://github.com/trailofbits/necessist/pull/1204))

## 0.6.2

- Update `swc_core` to version 0.96 ([#1178](https://github.com/trailofbits/necessist/pull/1178))
- Fix a bug causing Necessist to fail to build tests with recent versions of Foundry ([af5098f](https://github.com/trailofbits/necessist/commit/af5098fd7fcf5828ed149b1289bb6e5011f01dde))
- Improve Foundry test detection, i.e., when "Failed to run test..." warnings should be emitted ([#1186](https://github.com/trailofbits/necessist/pull/1186))
- Fix `necessist-backend`'s `rerun-if-changed` instructions, whose mention of a nonexistent file was causing the package to be unnecessarily rebuilt ([#1187](https://github.com/trailofbits/necessist/pull/1187))
- Shorten `--framework` values `anchor-ts` and `hardhat-ts` to just `anchor` and `hardhat` (respectively). `anchor-ts` and `hardhat-ts` continue to work as aliases for the shortened values. ([de21f2e](https://github.com/trailofbits/necessist/commit/de21f2eca2ab3532567f7c37ccab87c07bec0dc5))
- Improve error messages when certain Anchor and Hardhat files cannot be found ([e6e756e](https://github.com/trailofbits/necessist/commit/e6e756e9aec69f0dd9105f159860ae0d0b8e9b29))

## 0.6.1

- Don't remove expressions that end blocks in Rust code ([#1162](https://github.com/trailofbits/necessist/pull/1162))
- Update `git2` to version 0.19 ([0de7726](https://github.com/trailofbits/necessist/commit/0de772628b40b53fc86344ccfa1da18b1104bb4b))

## 0.6.0

- Don't remove `const` and `type` declarations in Go code ([#1139](https://github.com/trailofbits/necessist/pull/1139))
- Update `swc_core` to version 0.95 ([#1146](https://github.com/trailofbits/necessist/pull/1146))
- FEATURE: Use mutant schemata in the spirit of [Untch, et al. '93](https://dl.acm.org/doi/10.1145/154183.154265) when removing statements. This should make running Necessist faster. ([252ed2e](https://github.com/trailofbits/necessist/commit/252ed2eb4557cfe0741969f222b1add9584b3dbc))

## 0.5.1

- Update `solang-parser` to version 0.3.4 ([#1132](https://github.com/trailofbits/necessist/pull/1132))

## 0.5.0

- Fix mishandling of multibyte characters in the Go framework ([#1127](https://github.com/trailofbits/necessist/pull/1127))
- FEATURE: More informative summaries, e.g., "k removal candidates in m tests in n test files". Also, tests and test files are counted even if they contain no removal candidates. (fixes [#850](https://github.com/trailofbits/necessist/issues/850)) ([#1128](https://github.com/trailofbits/necessist/pull/1128))

## 0.4.9

- Add `panic` to list of ignored Go functions ([#1093](https://github.com/trailofbits/necessist/pull/1093))
- Update `swc_core` to version 0.92 ([#1099](https://github.com/trailofbits/necessist/pull/1099))

## 0.4.8

- Update `swc_core` to version 0.91 ([#1085](https://github.com/trailofbits/necessist/pull/1085))

## 0.4.7

- Update `libsqlite3-sys` to version 0.28 ([#1048](https://github.com/trailofbits/necessist/pull/1048))
- Update `tree-sitter` to version 0.22 ([674f1ff](https://github.com/trailofbits/necessist/commit/674f1ff9ed9bf6ee00856d308674fc1eb0464cdb))
- Update `tree-sitter-go` to version 0.21 ([48f716f](https://github.com/trailofbits/necessist/commit/48f716f4ed2cd66e73d0d2984c80115e71635e1c))

## 0.4.6

- Update `heck` to version 0.5 ([#1034](https://github.com/trailofbits/necessist/pull/1034))

## 0.4.5

- Eliminate reliance on `is-terminal` ([#1008](https://github.com/trailofbits/necessist/pull/1008) and [7878f3e](https://github.com/trailofbits/necessist/commit/7878f3e15337f090207823be7153482e44031292))
- Fix README.md table of contents ([f0c3c7a](https://github.com/trailofbits/necessist/commit/f0c3c7a05d15c614e362a4912fbc35a1dcca26e3))
- Add readme explaining origin of pt.rs file ([#1015](https://github.com/trailofbits/necessist/pull/1015))
- Update `toml_edit` to version 0.22 ([#1026](https://github.com/trailofbits/necessist/pull/1026))

## 0.4.4

- Update `strum_macros` to version 0.26 ([#975](https://github.com/trailofbits/necessist/pull/975))
- Update `swc_core` to version 0.90 ([#976](https://github.com/trailofbits/necessist/pull/976) and [#992](https://github.com/trailofbits/necessist/pull/992))
- Update `strum` to version 0.26 ([#978](https://github.com/trailofbits/necessist/pull/978))
- Include a copy of Solang's pt.rs in the repository, rather than download it when building ([#986](https://github.com/trailofbits/necessist/pull/986))

## 0.4.3

- Update `swc_core` to version 0.88.1 ([#963](https://github.com/trailofbits/necessist/pull/963))
- Update `shlex` to version 1.3.0 ([#967](https://github.com/trailofbits/necessist/pull/967))
- Update `env_logger` to version 0.11.0 ([#969](https://github.com/trailofbits/necessist/pull/969))

## 0.4.2

- Give an example when "configuration or test files have changed" (fix [#248](https://github.com/trailofbits/necessist/issues/248)) ([#936](https://github.com/trailofbits/necessist/pull/936))
- Make `--resume` work correctly following a dry run failure (fix [#249](https://github.com/trailofbits/necessist/issues/249)) ([#936](https://github.com/trailofbits/necessist/pull/936))
- Make parsing failures warnings instead of hard errors (fix [#245](https://github.com/trailofbits/necessist/issues/245)) ([#947](https://github.com/trailofbits/necessist/pull/947))

## 0.4.1

- Update `windows-sys` to version 0.52.0 ([#911](https://github.com/trailofbits/necessist/pull/911))

## 0.4.0

- Make error messages more informative ([#901](https://github.com/trailofbits/necessist/pull/901) and [#900](https://github.com/trailofbits/necessist/pull/900))
- FEATURE: Limited Windows support ([#879](https://github.com/trailofbits/necessist/pull/901))

## 0.3.4

- Fix link to README.md ([#887](https://github.com/trailofbits/necessist/pull/887))
- Strip ANSI escapes from build and test command output (a problem affecting `forge`, for example) ([#886](https://github.com/trailofbits/necessist/pull/886))

## 0.3.3

- Fix a bug involving the Foundry framework's handling of extra arguments ([#884](https://github.com/trailofbits/necessist/pull/884))

## 0.3.2

- Update list of ignored Rust methods ([51a0ec4](https://github.com/trailofbits/necessist/commit/51a0ec4ef5976cdf90d39704f249e8780f05a9ab))

## 0.3.1

- Simplify warning message ([20cf99e](https://github.com/trailofbits/necessist/commit/20cf99e0c23b7add56e8c88914d93078bbab0e8f))
- Initialize Sqlite database lazily ([00e2446](https://github.com/trailofbits/necessist/commit/00e2446648b436269f5d512a07e7a3db45d05b2d))

## 0.3.0

- Ignore `Skip`, `Skipf`, and `SkipNow` methods in Go framework ([#759](https://github.com/trailofbits/necessist/pull/759) and [#760](https://github.com/trailofbits/necessist/pull/760))
- [94e81c6](https://github.com/trailofbits/necessist/commit/94e81c6f6343ae4fc4ecce37ee494d914ffa668e) unintentionally removed `recursive_kill`'s post-visit behavior. [381a0ff](https://github.com/trailofbits/necessist/commit/381a0fff77233db5a89edc8f88983d69ebc9a64e) restores the post-visit behavior, but retains the non-recursiveness that [94e81c6](https://github.com/trailofbits/necessist/commit/94e81c6f6343ae4fc4ecce37ee494d914ffa668e) introduced. ([381a0ff](https://github.com/trailofbits/necessist/commit/381a0fff77233db5a89edc8f88983d69ebc9a64e))
- Add ability to ignore tests ([#798](https://github.com/trailofbits/necessist/pull/798))
- Lock project's root directory to help protect against concurrent uses of Necessist ([#791](https://github.com/trailofbits/necessist/pull/791))

## 0.2.3

- Limit the number of threads a test can allocate ([275b097](https://github.com/trailofbits/necessist/commit/275b0977c2d440f695ab0222b8447e8fffed7b9d))
- Make one recursive function not recursive to reduce the likelihood of a stack overflow ([94e81c6](https://github.com/trailofbits/necessist/commit/94e81c6f6343ae4fc4ecce37ee494d914ffa668e))

## 0.2.2

- Use `pnpm` if a pnpm-lock.yaml file exists ([bfb30b0](https://github.com/trailofbits/necessist/commit/bfb30b03a7002376f3dc4ea7968b68b74c844871))

## 0.2.1

- Fix a bug involving the Foundry framework's handling of trailing semicolons ([#663](https://github.com/trailofbits/necessist/pull/663))

## 0.2.0

- Add Anchor framework ([#587](https://github.com/trailofbits/necessist/pull/587))

## 0.1.3

- Verify that package.json exists before installing Node modules ([#580](https://github.com/trailofbits/necessist/pull/580))&mdash;thanks [@0xPhaze](https://github.com/0xPhaze)

## 0.1.2

- Migrate away from `atty` ([#556](https://github.com/trailofbits/necessist/pull/556))
- Improve Rust test discovery ([#537](https://github.com/trailofbits/necessist/pull/537))

## 0.1.1

- Improve Foundry support ([#515](https://github.com/trailofbits/necessist/pull/515))&mdash;thanks [@tarunbhm](https://github.com/tarunbhm)
- Improve sqlite database handling (e.g., fix [#119](https://github.com/trailofbits/necessist/issues/119)) ([#533](https://github.com/trailofbits/necessist/pull/533))&mdash;thanks [@tarunbhm](https://github.com/tarunbhm)

## 0.1.0

- Initial release
