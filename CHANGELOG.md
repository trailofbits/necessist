# Changelog

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
