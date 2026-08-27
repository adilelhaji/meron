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

## Google sign-in: use your own credentials

Exchange, IMAP and Outlook accounts need nothing here — this section is only
about signing in with Google.

The Google OAuth credentials compiled into this source tree are **Meron's**,
inherited when Oreneta forked from it. Do not ship builds that use them:
Nonbili paid for the verification behind that OAuth client, and borrowing it
would show *Meron* on the consent screen, spend their quota, and put their
verified status at risk for users who are not theirs.

Point a build at your own Google Cloud project instead. Both the Go app and
the Rust engine read these, and they take precedence over the compiled values:

```sh
export MERON_GOOGLE_CLIENT_ID="…apps.googleusercontent.com"
export MERON_GOOGLE_CLIENT_SECRET="GOCSPX-…"
```

In the Google Cloud console you will need: an OAuth client of type **Desktop
app** (the sign-in flow redirects to loopback, which a Web client rejects),
the **Google Calendar API** enabled, and the calendar scope added to the
consent screen. An app left in *Testing* issues refresh tokens that expire
after 7 days; publishing it to *Production* ends that, and no verification is
required to publish — verification only removes the "unverified app" warning
and the 100-user cap.

Distributing Oreneta publicly with Google sign-in working out of the box is a
different matter: Gmail is a *restricted* scope, so it needs Google's full
verification and an annual CASA security assessment. Until someone takes that
on, public builds should ship without Google sign-in — Gmail still works over
IMAP with an app password, and Google calendars can be subscribed to read-only
through their secret `.ics` address.

## License

Oreneta is licensed under the
[GNU Affero General Public License v3.0](LICENSE), as is the Meron code it
is built on. Copyright (C) 2026 Nonbili Inc. for the original work;
Copyright (C) 2026 Adil El Haji for the fork's changes. Internal module and
crate names keep Meron's naming on purpose, so upstream fixes keep merging
cleanly.
