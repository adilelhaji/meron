#!/usr/bin/env python3
"""Smoke-test the native EWS backend against a real Exchange server.

Drives the meron-core sidecar over its JSON-lines stdio protocol: adds an
Exchange account, lists folders, syncs a folder and reads one message —
without any UI, and against a throwaway profile so nothing touches the
installed app's data.

The password is read from the EWS_PASSWORD environment variable or prompted
for; it is sent only to the server named in --url and never written to disk
in cleartext (the throwaway profile is deleted on exit).

Usage:
  EWS_PASSWORD=... ./ews-smoketest.py \
      --url https://mail.example.org/EWS/Exchange.asmx \
      --user 'DOMAIN\\user' --email user@example.org
"""
import argparse, getpass, json, os, shutil, subprocess, sys, tempfile, threading, time

BIN = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                   "meron-core", "target", "release", "meron-core")


class Sidecar:
    def __init__(self, profile):
        env = dict(os.environ)
        env["MERON_CORE_DB"] = os.path.join(profile, "meron.db")
        env["MERON_MEDIA_DIR"] = os.path.join(profile, "media")
        env["MERON_KEYRING"] = "off"      # no OS keychain for a throwaway run
        self.proc = subprocess.Popen(
            [BIN], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, env=env, text=True, bufsize=1)
        self.next_id = 0
        self.responses = {}
        self.events = []
        self.lock = threading.Condition()
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._read_err, daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            with self.lock:
                if "id" in msg:
                    self.responses[msg["id"]] = msg
                else:
                    self.events.append(msg)
                self.lock.notify_all()

    def _read_err(self):
        for line in self.proc.stderr:
            line = line.rstrip()
            if line:
                print(f"    [core] {line}", file=sys.stderr)

    def call(self, method, params=None, timeout=90):
        self.next_id += 1
        rid = self.next_id
        self.proc.stdin.write(
            json.dumps({"id": rid, "method": method, "params": params or {}}) + "\n")
        self.proc.stdin.flush()
        deadline = time.time() + timeout
        with self.lock:
            while rid not in self.responses:
                if not self.lock.wait(timeout=max(0, deadline - time.time())):
                    raise TimeoutError(f"{method} timed out after {timeout}s")
            return self.responses.pop(rid)

    def wait_event(self, name, timeout=120):
        """Wait for a named event; the sidecar reports sync completion as
        `mail.synced` for both folder and message syncs."""
        deadline = time.time() + timeout
        with self.lock:
            while True:
                for i, ev in enumerate(self.events):
                    if ev.get("event") == name:
                        return self.events.pop(i)
                if not self.lock.wait(timeout=max(0, deadline - time.time())):
                    return None

    def close(self):
        try:
            self.proc.terminate()
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


def check(label, response):
    """Print one step's outcome; return its result or None on failure."""
    if "error" in response:
        print(f"  FAIL  {label}: {response['error'].get('message', response['error'])}")
        return None
    print(f"  ok    {label}")
    return response.get("result")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", required=True, help="EWS endpoint URL")
    ap.add_argument("--user", required=True, help="username, or DOMAIN\\user")
    ap.add_argument("--email", required=True)
    ap.add_argument("--folder", default="Inbox", help="folder to sync (default: Inbox)")
    ap.add_argument("--limit", type=int, default=5, help="messages to sync")
    ap.add_argument("--probe", action="store_true",
                    help="instead of the smoke test, report which envelope "
                         "properties this server accepts")
    args = ap.parse_args()

    if not os.path.exists(BIN):
        sys.exit(f"binary not found: {BIN}\nbuild it with: cargo build --release --bin meron-core")

    password = os.environ.get("EWS_PASSWORD") or getpass.getpass("Exchange password: ")
    if not password:
        sys.exit("no password given")

    if args.probe:
        # Reuse this script's password handling rather than making the caller
        # get a secret through the shell safely.
        probe = os.path.join(os.path.dirname(BIN), "examples", "ews_probe")
        if not os.path.exists(probe):
            sys.exit(f"probe not built: cargo build --release --example ews_probe")
        env = dict(os.environ, EWS_URL=args.url, EWS_USER=args.user,
                   EWS_PASSWORD=password, EWS_FOLDER=args.folder)
        return subprocess.call([probe], env=env)

    profile = tempfile.mkdtemp(prefix="meron-ews-smoketest-")
    core = Sidecar(profile)
    failures = 0
    try:
        print(f"\n== EWS smoke test against {args.url}\n")

        print("1. add account (validates with a live EWS round trip)")
        account = args.email
        result = check("account.connect", core.call("account.connect", {
            "id": account, "ews_url": args.url, "user": args.user,
            "password": password, "email": args.email, "provider": "exchange",
            "auth_type": "password", "validate": True,
        }))
        if result is None:
            return 1
        print(f"        account id: {account}")

        print("\n2. list folders (SyncFolderHierarchy)")
        core.call("folders.list", {"account": account, "refresh": True})
        core.wait_event("mail.synced", timeout=90)
        folders = check("folders.list", core.call(
            "folders.list", {"account": account, "refresh": False}))
        names = []
        if folders:
            names = [f.get("name", "?") for f in folders.get("folders", [])]
            print(f"        {len(names)} mail folders: {', '.join(names[:12])}"
                  + (" …" if len(names) > 12 else ""))
            if not names:
                print("  FAIL  no folders returned")
                failures += 1
        else:
            failures += 1

        target = args.folder
        if names and target not in names:
            inbox = next((n for n in names if n.lower() in ("inbox", "bandeja de entrada")), None)
            if inbox:
                print(f"        '{target}' not found; using '{inbox}'")
                target = inbox

        print(f"\n3. sync '{target}' ({args.limit} messages: SyncFolderItems + envelope GetItem)")
        if check("messages.sync", core.call(
                "messages.sync", {"account": account, "folder": target, "limit": args.limit})) is None:
            failures += 1
        core.wait_event("mail.synced", timeout=180)
        recent = check("messages.recent", core.call(
            "messages.recent", {"account": account, "folder": target, "limit": args.limit}))
        headers = []
        if recent:
            headers = recent.get("messages", recent.get("headers", []))
            print(f"        {len(headers)} messages cached")
            for h in headers[:5]:
                subject = (h.get("subject") or "(no subject)")[:60]
                print(f"          uid {h.get('uid'):>5}  {h.get('from_addr', '?'):32.32}  {subject}")
            if not headers:
                print("  FAIL  no messages returned")
                failures += 1
        else:
            failures += 1

        if headers:
            uid = headers[0].get("uid")
            print(f"\n4. read message uid {uid} (GetItem with MIME)")
            body = check("messages.read", core.call(
                "messages.read", {"account": account, "folder": target, "uid": uid}))
            if body:
                message = body.get("message", body)
                plain = message.get("body") or ""
                html = message.get("body_html") or ""
                print(f"        subject: {message.get('subject', '?')}")
                print(f"        from:    {message.get('from_addr', '?')}")
                print(f"        body:    {len(plain)} chars plain, {len(html)} chars html, "
                      f"{len(message.get('attachments') or [])} attachments")
                snippet = " ".join((plain or html).split())[:70]
                if snippet:
                    print(f"        starts:  {snippet}…")
                else:
                    print("  WARN  body parsed empty")
                    failures += 1
            else:
                failures += 1

        print(f"\n== {'PASSED' if failures == 0 else str(failures) + ' STEP(S) FAILED'}\n")
        return 1 if failures else 0
    finally:
        core.close()
        shutil.rmtree(profile, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
