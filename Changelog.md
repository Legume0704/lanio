# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.2]

- Add manual workflow dispatch for test Docker image builds

## [0.5.1]

- Add `linux/arm/v7` Docker image support for 32-bit Raspberry Pi OS

## [0.5.0]

- Catalog entries now use the TMDB title for the display name (falling back to the parsed filename)
- Add richer metadata to catalog entries from the existing TMDB lookups (no extra requests):
  - `releaseInfo` (release year) and `description` (overview)
  - `appExtras.ratings.tmdb` with TMDB rating and vote count
  - `posterShape: "poster"` on each entry
- Bump Rust MSRV-compatible TMDB metadata so file years no longer affect release year when TMDB data is present

## [0.4.0]

- Add optional `SCAN_CRON` config to run a full media library rescan on a schedule
  - Supports standard 5-field cron plus 6- and 7-field forms with seconds
- Add `POST /rescan` endpoint for Sonarr/Radarr webhooks
  - Requests arriving while a scan is in progress are queued and run when the current scan finishes
- Refactor automated integration tests into a shared helper module with focused per-feature suites

## [0.3.0]

- Add optional config override for poster URL

## [0.2.2]

- Changed automated integration tests to be more black-box

## [0.2.1]

- Add automated integration tests

## [0.2.0]

- Add authentication
  - To enable authentication, set a `PASSWORD` environment variable

## [0.1.4]

- Fix TMDB API Rate Limiter

## [0.1.3]

- Add Wiki

## [0.1.2]

- Tag and build containers on push to main

## [0.1.1]

- Version check for Pull Requests
- Cargo test & clippy for Pull Requests
- Tag generator when merged into main

## [0.1.0]

- Lanio initial version
- Scan library files
- Serve Stremio endpoints
- Add catalogs for media
