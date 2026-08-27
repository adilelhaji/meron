<h1 align="center">Oreneta</h1>
<div align="center">
  <p>Mail and calendar, at home with Exchange</p>
  <img src="build/appicon.png" width="128" alt="Oreneta">
</div>

Oreneta is a fast desktop mail and calendar app with chat and kanban views,
and native Microsoft Exchange (EWS) support. *Oreneta* is Catalan for the
swallow — the bird that always finds its way home.

It is a fork of [Meron](https://github.com/nonbili/meron) by Nonbili Inc.,
which declined native Exchange support upstream. Oreneta carries everything
Meron does, plus:

- **Native Exchange (EWS)**: mail against on-premise Exchange servers with no
  IMAP bridge in between — folders, incremental sync, send, flags, move.
- **A full calendar**: agenda, day, week and month views; events created,
  edited and deleted on the server; recurring series expanded server-side.
- **Calendars from anywhere**: an account's calendars arrive with the account
  (Exchange, Google Calendar), alongside local calendars and read-only
  subscriptions to published `.ics` addresses.

## Building

```sh
cargo build --release --bin meron-core   # the Rust engine
wails build -tags webkit2_41             # the desktop app
```

## License

Oreneta is licensed under the
[GNU Affero General Public License v3.0](LICENSE), as is the Meron code it
is built on. Copyright (C) 2026 Nonbili Inc. for the original work;
Copyright (C) 2026 Adil El Haji for the fork's changes. Internal module and
crate names keep Meron's naming on purpose, so upstream fixes keep merging
cleanly.
