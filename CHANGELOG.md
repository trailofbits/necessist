# Changelog

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
