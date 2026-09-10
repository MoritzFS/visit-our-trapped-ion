#!/usr/bin/env python3
"""Serve trap_vr.html over HTTPS on the local network, for the headset.

    python3 serve.py               # https on :8443  (+ plain http on :8444)
    python3 serve.py 9000          # another port
    python3 serve.py --no-plain    # https only

Why HTTPS for a file on your own laptop: WebXR only offers immersive mode on a
*secure context*. `file://` and a plain `http://192.168.x.x` are not secure
contexts, so the Quest browser loads the page and simply never shows Enter VR.
`https://` with a self-signed certificate is, once you tap through the warning
-- which is the route Meta's own WebXR docs tell you to take.

No internet is needed, by the laptop or the headset. They only have to be able
to reach each other. Nothing in the page fetches anything.

Diagnostics, because a headset gives you no error messages at all:

  * every connection, request, TLS failure and disconnect is logged with a
    timestamp. If the log stays empty, the headset is not reaching this
    machine and the problem is the network, not the page.
  * /ping answers instantly with a few bytes, on both ports. Load that first:
    it separates "cannot reach the Mac" from "cannot get through 1.1 MB".
  * the plain-http port exists only to test reachability. WebXR will not work
    over it -- if /ping works on :8444 but not on :8443, the problem is TLS.
  * the certificate is regenerated automatically when this machine's addresses
    change, which they do every time you join a different network.
"""
from __future__ import annotations

import datetime
import http.server
import os
import re
import socket
import socketserver
import ssl
import subprocess
import sys
import tempfile
import threading

HERE = os.path.dirname(os.path.abspath(__file__)) or "."
CERTDIR = os.path.join(HERE, ".certs")
CERT = os.path.join(CERTDIR, "cert.pem")
KEY = os.path.join(CERTDIR, "key.pem")
HANDSHAKE_TIMEOUT = 15
REQUEST_TIMEOUT = 60

args = [a for a in sys.argv[1:]]
PLAIN = "--no-plain" not in args
PORT = next((int(a) for a in args if a.isdigit()), 8443)
PLAIN_PORT = PORT + 1

_lock = threading.Lock()


def log(*parts: object) -> None:
    stamp = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
    with _lock:
        sys.stdout.write("  %s  %s\n" % (stamp, " ".join(str(p) for p in parts)))
        sys.stdout.flush()


# ── addresses ──────────────────────────────────────────────────────────────

def local_ips() -> list[str]:
    """Every IPv4 this machine answers on, the outward-facing one first."""
    ips: list[str] = []
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("192.0.2.1", 1))          # TEST-NET-1, no packet is sent
        ips.append(s.getsockname()[0])
    except OSError:
        pass
    finally:
        s.close()
    for cmd in (["ifconfig"], ["ip", "-4", "addr"]):
        try:
            out = subprocess.run(cmd, capture_output=True, text=True,
                                 timeout=5).stdout
        except (OSError, subprocess.SubprocessError):
            continue
        for ip in re.findall(r"inet\s+(\d+\.\d+\.\d+\.\d+)", out):
            if ip != "127.0.0.1" and ip not in ips:
                ips.append(ip)
        break
    return ips


# ── certificate ────────────────────────────────────────────────────────────

def cert_covers(ips: list[str]) -> bool:
    """Is the stored certificate still valid for where we are now?

    Joining a different network changes this machine's address, and a
    certificate that does not name the address you typed is a hard failure in
    some browsers rather than a dismissable warning.
    """
    if not (os.path.exists(CERT) and os.path.exists(KEY)):
        return False
    try:
        out = subprocess.run(["openssl", "x509", "-in", CERT, "-noout", "-text"],
                             capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.SubprocessError):
        return False
    named = set(re.findall(r"IP Address:(\d+\.\d+\.\d+\.\d+)", out))
    return all(ip in named for ip in ips)


def make_cert(ips: list[str]) -> None:
    """Self-signed cert covering localhost and every local address.

    Written through an OpenSSL config file rather than -addext, because macOS
    ships LibreSSL and the two disagree about that flag.
    """
    os.makedirs(CERTDIR, exist_ok=True)
    alt = ["DNS.1 = localhost", "IP.1 = 127.0.0.1"]
    for i, ip in enumerate(ips, start=2):
        alt.append("IP.%d = %s" % (i, ip))
    cnf = ("[req]\ndistinguished_name = dn\nx509_extensions = v3\nprompt = no\n"
           "[dn]\nCN = traart-vr\n"
           "[v3]\nsubjectAltName = @alt\nbasicConstraints = CA:FALSE\n"
           "keyUsage = digitalSignature, keyEncipherment\n"
           "extendedKeyUsage = serverAuth\n"
           "[alt]\n" + "\n".join(alt) + "\n")
    with tempfile.NamedTemporaryFile("w", suffix=".cnf", delete=False) as fh:
        fh.write(cnf)
        path = fh.name
    try:
        subprocess.run(
            ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
             "-days", "825", "-keyout", KEY, "-out", CERT, "-config", path],
            check=True, capture_output=True)
    except FileNotFoundError:
        raise SystemExit("openssl not found -- install it, or serve the page "
                         "over adb (see README).")
    except subprocess.CalledProcessError as e:
        raise SystemExit("openssl failed:\n" + e.stderr.decode(errors="replace"))
    finally:
        os.unlink(path)


# ── handler ────────────────────────────────────────────────────────────────

class Handler(http.server.SimpleHTTPRequestHandler):
    # HTTP/1.0: one connection per request, no idle keep-alives to time out.
    # There is only one real request here, so nothing is lost.
    protocol_version = "HTTP/1.0"

    def __init__(self, *a, **kw):
        super().__init__(*a, directory=HERE, **kw)

    def do_GET(self):
        if self.path.startswith("/ping"):
            body = b"pong\n"
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if self.path in ("/", "/index.html"):
            # bare address -> the page, so there is less to type in mid-air
            self.send_response(302)
            self.send_header("Location", "/trap_vr.html")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        super().do_GET()

    def end_headers(self):
        # the page changes on every rebuild; never let anything cache it
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    # Log everything. A headset shows you no errors, so the server's log is
    # the only witness. (The base class passes ints here for error codes, so
    # this must not assume strings.)
    def log_message(self, fmt, *a):
        try:
            msg = fmt % a
        except Exception:
            msg = " ".join(str(x) for x in (fmt,) + a)
        log("%-15s %s %s" % (self.client_address[0], self.scheme_tag, msg))

    scheme_tag = "https"


class PlainHandler(Handler):
    scheme_tag = "http "


class RedirectToTLS(http.server.BaseHTTPRequestHandler):
    """Answers plain HTTP that arrives on the HTTPS port.

    Typing a bare `192.168.1.5:8443` into a browser gets you `http://`, and
    every browser's URL bar does this -- but a headset's shows you a spinner
    rather than a protocol error, so the mistake is invisible and expensive.
    Send them back with the right scheme instead of failing the handshake.
    """

    protocol_version = "HTTP/1.0"

    def _redirect(self):
        host = self.headers.get("Host") or "%s:%d" % (self.server.server_address)
        self.send_response(301)
        self.send_header("Location", "https://%s%s" % (host, self.path))
        self.send_header("Content-Length", "0")
        self.end_headers()

    do_GET = do_HEAD = do_POST = _redirect

    def log_message(self, fmt, *a):
        try:
            msg = fmt % a
        except Exception:
            msg = " ".join(str(x) for x in (fmt,) + a)
        log("%-15s http→ %s  (sent plain HTTP to the TLS port)"
            % (self.client_address[0], msg))


# ── server ─────────────────────────────────────────────────────────────────

class TLSServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    """HTTPS server that does the TLS handshake in the worker thread.

    The obvious way -- wrapping the *listening* socket -- performs the
    handshake inside the accept loop, so one client that connects and then
    says nothing blocks every other connection. Browsers open speculative
    connections all the time, and Chromium-based ones (the Quest browser
    included) do it eagerly, which is a very good way to make a page hang
    forever with no error anywhere. Handshake in the thread, with a timeout.
    """

    daemon_threads = True
    allow_reuse_address = True

    def __init__(self, addr, handler, ctx):
        self.ctx = ctx
        super().__init__(addr, handler)

    def get_request(self):
        sock, addr = self.socket.accept()
        log("%-15s ·  connect" % addr[0])
        sock.settimeout(HANDSHAKE_TIMEOUT)
        return sock, addr                    # still plain; wrapped below

    def finish_request(self, request, addr):
        # Look at the first byte without consuming it: a TLS ClientHello is a
        # handshake record, 0x16. Anything else is someone speaking plain HTTP.
        try:
            first = request.recv(1, socket.MSG_PEEK)
        except (socket.timeout, OSError) as e:
            log("%-15s !  connected but sent nothing (%s)" % (addr[0], e))
            return
        if first and first != b"\x16":
            RedirectToTLS(request, addr, self)
            return
        try:
            tls = self.ctx.wrap_socket(request, server_side=True)
        except ssl.SSLError as e:
            log("%-15s !  TLS handshake failed: %s" % (addr[0], e))
            return
        except (socket.timeout, OSError) as e:
            log("%-15s !  connected but sent nothing (%s)" % (addr[0], e))
            return
        tls.settimeout(REQUEST_TIMEOUT)
        self.RequestHandlerClass(tls, addr, self)

    def handle_error(self, request, addr):
        exc = sys.exc_info()[1]
        if isinstance(exc, (ConnectionResetError, BrokenPipeError, socket.timeout)):
            log("%-15s ·  client went away (%s)" % (addr[0], type(exc).__name__))
            return
        log("%-15s !  %s: %s" % (addr[0], type(exc).__name__, exc))


class PlainServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


def self_test(port: int) -> None:
    """Prove the server answers before blaming the network."""
    try:
        ctx = ssl.create_default_context()
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE
        with socket.create_connection(("127.0.0.1", port), timeout=5) as raw:
            with ctx.wrap_socket(raw) as s:
                s.sendall(b"GET /ping HTTP/1.0\r\n\r\n")
                buf = b""
                while True:                      # read to EOF, not one packet
                    chunk = s.recv(4096)
                    if not chunk:
                        break
                    buf += chunk
                ok = b"pong" in buf
        log("self-test on 127.0.0.1:%d %s" % (port, "OK" if ok else "FAILED"))
    except Exception as e:
        log("self-test on 127.0.0.1:%d FAILED: %s" % (port, e))


def main() -> None:
    ips = local_ips()
    if not cert_covers(ips):
        print("generating a certificate for", ", ".join(ips) or "localhost")
        make_cert(ips)

    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(CERT, KEY)
    httpd = TLSServer(("0.0.0.0", PORT), Handler, ctx)

    print()
    print("  serving  %s" % HERE)
    print()
    print("  in the headset browser -- TYPE THE https:// YOURSELF, the URL bar")
    print("  will assume http:// for a bare address and you will get a spinner:")
    for ip in ips or ["<this machine's IP>"]:
        print("      https://%s:%d          the page" % (ip, PORT))
        print("      https://%s:%d/ping     load this first, it is 5 bytes" % (ip, PORT))
    print("  over a USB cable (adb reverse tcp:%d tcp:%d):" % (PORT, PORT))
    print("      https://localhost:%d" % PORT)
    if PLAIN:
        plain = PlainServer(("0.0.0.0", PLAIN_PORT), PlainHandler)
        threading.Thread(target=plain.serve_forever, daemon=True).start()
        for ip in ips[:1]:
            print("  reachability test only, no WebXR over this:")
            print("      http://%s:%d/ping" % (ip, PLAIN_PORT))
    print()
    print("  first load warns about the certificate: Advanced -> Proceed.")
    print("  every connection is logged below. an empty log means nothing")
    print("  is reaching this machine.  ctrl-C to stop.")
    print()

    threading.Timer(0.4, self_test, args=(PORT,)).start()
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nstopped")


if __name__ == "__main__":
    main()
