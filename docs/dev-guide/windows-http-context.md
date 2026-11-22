# Windows remote HTTP context

A Windows GUI client shows a remote HTTP(S) document in WebView2. Every request of
that WebView reaches the document endpoint, which forwards it to the remote host
as described in the [HTTP bridge](html-http-bridge.md). The client never connects
to the logical target. WebRTC in a document whose scripts are allowed is the
exception, which the view only narrows, described under
[Direct network paths](#direct-network-paths).

## Why an HTTP proxy front

Chromium has no SOCKS5 username/password authentication, so the SOCKS front used
by WebKitGTK cannot authenticate a WebView2 context. On Windows the document
endpoint uses `Transport::HttpProxy` instead. The listener still binds only a
fresh loopback port and keeps the same budgets, destination policy, body
ownership and certificate map. Only the front differs:

- A request without valid `Proxy-Authorization` gets `407` with a
  `Basic realm="tasty-<random>"` challenge and `Connection: close`.
- The user is `tasty` and the password is the hex document token. Neither appears
  in a URL, a browser argument or a log.
- `CONNECT host:port` selects a tunnel. The endpoint answers
  `200 Connection Established` and then continues with the same TLS or plaintext
  routing as the SOCKS front.
- An absolute-form `http://` request is served on the same connection. The
  endpoint removes `Proxy-Authorization` and `Proxy-Connection` from the head
  before the request codec reads it. Userinfo, other schemes and origin-form
  requests get `403`.
- The connection is closed after any refusal. Its write side is shut down first
  and a bounded amount of input is drained, so Windows does not reset the
  connection before the browser reads the reply.

## WebView2 context

Each HTTP document view has its own user-data folder under
`webview/http` in the Tasty home and an InPrivate controller. Cookies therefore
live only as long as that view. The environment uses these browser arguments:

- `--proxy-server=http://127.0.0.1:<port>` sends every scheme to the endpoint.
- `--proxy-bypass-list=<-loopback>` removes Chromium's implicit loopback bypass, so
  `localhost` and `127.0.0.1` refer to the remote host.
- `--disable-quic` keeps HTTP/3 off, since QUIC would not use the proxy.
- `--disable-background-networking` keeps background browser services quiet.

`BasicAuthenticationRequested` answers only when the document is still current
and the challenge URI and realm match its own endpoint. Every other challenge,
including a remote page's `401`, is cancelled. Reputation checks, password
saving and general autofill are off, and new windows are refused.

`ServerCertificateErrorDetected` allows a certificate only when the status is
`CertificateIsInvalid` and the DER SHA-256 equals the leaf this endpoint generated
for that host and port. The endpoint generates that leaf, and the remote host
verified the real origin certificate. A leaf match therefore leaves an untrusted
issuer as the only possible cause. Any other status or certificate keeps
WebView2's refusal.

## Script policy

WebView2 fixes script enablement when a navigation starts, before any response
exists. A native setting cannot follow the scan of the verified main response.
In HTTP mode the native setting therefore stays on, and the endpoint decides:

- `NavigationStarting` records the main-frame URL and starts the gate's load.
- The endpoint's next response for that URL is the main response. Its exact
  verified body is scanned and decides the document's scripts.
- While scripts are not allowed, every response carries
  `Content-Security-Policy: script-src 'none'` and `Cache-Control: no-store`. Each
  iframe response also carries the policy, because CSP does not inherit.
- A commit while that main response is still unserved is WebView2's own page,
  such as the first attempt that received the proxy challenge or an error page.
  It does not commit a document to the gate.

Script Allow marks the verified response for one replay and calls `Reload`. The
endpoint answers the reloaded main request with that exact response, so a POST
is not sent again.

## Reload

A user or agent Reload keeps the same view and WebView2 context, like Linux. The
environment fixed its proxy address at creation, so the successor document's
endpoint binds the same loopback port:

1. The view retires the current document's Reload receipt, records its proxy
   port, marks the session retained and closes it.
2. The new document generation creates a successor session from that receipt.
   Its endpoint waits until the predecessor endpoint has joined, then binds the
   recorded port with a fresh token and realm.
3. When that endpoint listens, the view adopts the successor session and its
   credentials and moves the script gate to it.
4. The view clears the disk cache, service workers and Cache Storage. Cookies and
   other storage stay. A failed clear fails the session instead of reloading.
5. `Reload()` resends the current entry with its original method, body and
   cookies. WebView2's cached proxy credentials no longer match, so the endpoint
   answers `407` and the view authenticates again with the new credentials.

The successor bridge admits only a first request that matches the captured main
request's URL, method and upload. A document without a Reload receipt, such as
one whose first load failed or whose peer did not negotiate Reload, is replaced
by a new view instead. That view requests the entry URL with GET and starts
without the previous cookies. If another socket, in another process or in Tasty
itself, takes the port before the successor binds it, the successor fails and the
surface shows the usual failed state. The warning log names the error kind and
code the OS returned for the bind, usually `AddrInUse` for a taken port.
Winsock can return `WSAEACCES` (`PermissionDenied`) instead, for example when the
port falls in an excluded port range.

## Direct network paths

WebRTC and DNS prefetch do not use the proxy, and WebView2 has no setting for
either.

- Before the first navigation, the view registers one script with
  `AddScriptToExecuteOnDocumentCreated`. It runs in every frame when its document
  is created and deletes `RTCPeerConnection`, `webkitRTCPeerConnection`,
  `RTCDataChannel`, `RTCSessionDescription` and `RTCIceCandidate`. If WebView2
  refuses the registration, view creation fails with that error and no remote
  document opens. Local and entry views do not get the script.
- This is a mitigation of the measured paths, not a guarantee. Without it, a
  document whose scripts are allowed sent STUN over UDP from the client to an
  address the page named and resolved the STUN server name itself. With it, the
  constructors were absent in the main frame, in a child frame from the document's
  origin and in an `about:blank` frame the page created, and no STUN packet was
  sent. Other ways to reach a constructor were not measured.
- No DNS prefetch lookup was observed with the view's proxy arguments.
  `X-DNS-Prefetch-Control: off` did not stop a `dns-prefetch` link without a
  proxy, so the endpoint does not add it.
- `--force-webrtc-ip-handling-policy=disable_non_proxied_udp` is a Chromium
  switch but not a documented WebView2 flag. It had no effect on STUN UDP, so the
  view does not pass it.

These results come from a standalone WebView2 host with the same browser
arguments, the same `webview2-com` binding and the same script file. It used an
unauthenticated local proxy, a UDP trap and the browser network log, and ran in
the interactive session with a hidden window on runtime 156.0.4314.50. The Tasty
client itself was not measured.

## Limits

- WebSocket and other upgrades are refused, like on Linux.
- `[::1]` is unreachable: WebView2 sends no request for it to the proxy.
- SmartScreen sends an unauthenticated `CONNECT` that receives `407` and is not
  repeated. Reputation checking is disabled.

## Measured behaviour

These were observed with WebView2 runtime 155.0.4283.62 on Windows 11 in an interactive
session. An SSH session0 environment cannot create the controller
(`0x80070578`).

- The first request receives `407`. WebView2 then raises
  `BasicAuthenticationRequested` and repeats the request. Later requests send
  `Proxy-Authorization` without a challenge.
- The challenged attempt completes as `ValidProxyAuthenticationRequired` and can
  raise `ContentLoading`. The view ignores both.
- With `<-loopback>`, navigation, fetch, images and WebSocket requests to
  `localhost` and `127.0.0.1` all reached the proxy. A trap listener received no
  direct connection.
- `Reload()` of a document opened by POST resends the POST with the same body
  and cookies. A proxy listener closed and rebound on the same port with new
  credentials and realm makes WebView2 receive `407` and ask again.
- `IsScriptEnabled` changes after `NavigationStarting` do not affect that
  navigation. A `script-src 'none'` response header blocks the document's
  scripts.

Two isolated debug instances on one PC confirmed the whole path. They showed a
local origin with CSS, images, a cookie and blocked scripts, and the banner. Allow
ran scripts and a same-origin fetch without a second main request. An
auto-submitted POST reached the origin once. Reload kept the same view, the
cookie and the script allowance, and a POST document's Reload sent the same body
and cookie again. An HTTPS Reload accepted the successor endpoint's new leaf.
`https://example.com` passed the leaf check and rendered.
