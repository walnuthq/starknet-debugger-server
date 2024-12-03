# Changelog

## [0.0.5] (https://github.com/walnuthq/walnut-server/releases/tag/0.0.5)

- installed Rust (and cargo) in Dockerfile final stage - it is bugfix, missing cargo causes verification problem
  due to lack of `cargo` binary

## [0.0.6] (https://github.com/walnuthq/walnut-server/releases/tag/0.0.6)

- fixed Telegram notifications to split to multiple messages when message is too long (longer than 4096 characters)
- fixed Telegram notifications when the block number is not provided
- added readonly S3 Scaleway API KEYs to .env.example
