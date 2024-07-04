# Changelog

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

- Verify that package.json exists before installing Node modules ([#580](https://github.com/trailofbits/necessist/pull/580))

## 0.1.2

- Migrate away from `atty` ([#556](https://github.com/trailofbits/necessist/pull/556))
- Improve Rust test discovery ([#537](https://github.com/trailofbits/necessist/pull/537))

## 0.1.1

- Improve Foundry support ([#515](https://github.com/trailofbits/necessist/pull/515))
- Improve sqlite database handling (e.g., fix [#119](https://github.com/trailofbits/necessist/issues/119)) ([#533](https://github.com/trailofbits/necessist/pull/533))

## 0.1.0

- Initial release
