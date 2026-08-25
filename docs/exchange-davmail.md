# Using Exchange accounts via DavMail

Meron speaks IMAP/SMTP. Many organizations run Microsoft Exchange with
IMAP/SMTP disabled or firewalled, exposing only Outlook Web Access (OWA) and
EWS over HTTPS. [DavMail](https://davmail.sourceforge.net/) bridges that gap:
it runs locally, talks EWS to the Exchange server, and serves IMAP/SMTP on
localhost for Meron to connect to.

```
Meron  ──IMAP/SMTP (localhost)──▶  DavMail  ──EWS over HTTPS──▶  Exchange
```

This setup was validated against an on-premises Exchange server with all
IMAP/SMTP ports filtered.

## 1. Install DavMail

Most Linux distributions package it (`apt install davmail`,
`dnf install davmail`); Windows and macOS builds are on the
[DavMail site](https://davmail.sourceforge.net/download.html).

## 2. Configure DavMail

Minimal `davmail.properties` for headless use:

```properties
davmail.server=true
davmail.mode=EWS
# Point at the EWS endpoint directly. Using the /owa/ URL also works but goes
# through OWA form-based login, which is less reliable (intermittent 400s).
davmail.url=https://mail.example.org/EWS/Exchange.asmx

# Local ports, only reachable from this machine
davmail.imapPort=1143
davmail.smtpPort=1025
davmail.allowRemote=false
davmail.bindAddress=127.0.0.1

# Serving an exact message size requires downloading the message; approximate
# sizes make folder listings much faster through EWS.
davmail.imapAlwaysApproxMsgSize=true
# Enable IMAP IDLE so Meron gets push notifications instead of polling
davmail.imapIdleDelay=1
```

Run it with `davmail /path/to/davmail.properties` (or enable it as a service —
on Linux a systemd user unit with
`ExecStart=/usr/bin/davmail %h/.davmail.properties` works well).

## 3. Add the account in Meron

Use manual configuration (autodiscover cannot find a localhost gateway):

|            | Incoming (IMAP)  | Outgoing (SMTP)  |
| ---------- | ---------------- | ---------------- |
| Server     | `127.0.0.1`      | `127.0.0.1`      |
| Port       | `1143`           | `1025`           |
| Encryption | None             | None             |
| Username   | your email address, or `DOMAIN\user` as used in OWA | same |
| Password   | your Exchange password | same |

Plaintext on these connections is fine: they never leave your machine, and
DavMail talks to the Exchange server over HTTPS. Your password is forwarded to
Exchange by DavMail and stored only by Meron.

## Troubleshooting

**"Could not connect… retrying" while mail trickles in, or
`sync <folder>: timed out after 30s` in the log.** DavMail downloads the full
message from Exchange to serve each header, so syncing a large folder through
EWS is much slower than direct IMAP and can exceed Meron's default 30s
per-folder sync budget. Raise it with the `MERON_SYNC_TIMEOUT` environment
variable (seconds), e.g. `MERON_SYNC_TIMEOUT=300`. For a Flatpak install:

```
flatpak override --user jp.nonbili.meron --env=MERON_SYNC_TIMEOUT=300
```

**Login fails from Meron but works in OWA.** Try the `DOMAIN\user` form of
your username, and point `davmail.url` at `…/EWS/Exchange.asmx` rather than
`…/owa/`. DavMail's log (`davmail.logFilePath`) shows the Exchange-side
response.

**Only mail is bridged.** This setup covers mail. DavMail can also expose
calendar (CalDAV) and address book (LDAP/CardDAV), but Meron does not consume
those.
