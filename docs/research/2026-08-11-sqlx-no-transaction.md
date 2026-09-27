# SQLx SQLite no-transaction migration support

## Finding

Yes. SQLx **0.9.0** supports SQLite migrations beginning with
`-- no-transaction`. This is the first released SQLx version containing the
SQLite implementation; SQLx 0.8.6 parses the flag but does not honor it for
SQLite.

## Evidence

- SQLx's [0.9.0 changelog](https://github.com/launchbadge/sqlx/blob/v0.9.0/CHANGELOG.md#added)
  records PR [#4015](https://github.com/launchbadge/sqlx/pull/4015),
  "feat(sqlite): `no_tx` migration support." The PR explains that the feature
  is needed for SQLite statements such as `PRAGMA foreign_keys = ON|OFF` and
  for migrations which need their own transaction boundaries.
- The [0.9.0 SQLite migrator source](https://github.com/launchbadge/sqlx/blob/v0.9.0/sqlx-sqlite/src/migrate.rs#L157-L176)
  checks `migration.no_tx`; in that path it runs the migration directly on the
  connection rather than calling `self.begin()`. The normal path still opens
  and commits SQLx's transaction. The matching
  [revert path](https://github.com/launchbadge/sqlx/blob/v0.9.0/sqlx-sqlite/src/migrate.rs#L201-L217)
  does the same.
- In contrast, the [0.8.6 SQLite migrator](https://github.com/launchbadge/sqlx/blob/v0.8.6/sqlx-sqlite/src/migrate.rs#L129-L166)
  unconditionally begins a transaction. Its migration parser recognizes the
  flag ([source](https://github.com/launchbadge/sqlx/blob/v0.8.6/sqlx-core/src/migrate/source.rs#L126-L135)),
  but the SQLite runner does not use it.

## Upgrade compatibility

The smallest released upgrade containing the capability is the breaking major
upgrade to `sqlx = "0.9"`; there is no SQLx 0.8.7+ release. SQLx 0.9.0 declares
Rust 1.94.0 in its [workspace manifest](https://github.com/launchbadge/sqlx/blob/v0.9.0/Cargo.toml#L25-L44).
This worktree currently uses Rust 1.90.0, so it cannot make that upgrade
without a toolchain upgrade. The project also depends on
`tower-sessions-sqlx-store`, so its SQLx compatibility must be checked during
any upgrade.

## Recommendation

Do not upgrade SQLx solely to obtain this feature while the application is on
Rust 1.90. Use a custom, one-time migration runner for migration 003 or revise
the migration to avoid the out-of-transaction PRAGMA requirement. If the
toolchain is raised to Rust 1.94+, upgrade both direct SQLx dependencies to
0.9, regenerate `Cargo.lock`, and run the complete test suite before adopting
the existing `-- no-transaction` migration design.
