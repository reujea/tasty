# macOS remote HTTP context

A macOS GUI client on macOS 27 or later shows a remote HTTP(S) document in a
WKWebView. Every request of that view reaches the document endpoint, which
forwards it to the remote host as described in the [HTTP bridge](html-http-bridge.md).
The client never connects to the logical target. WebRTC in a document whose
scripts are allowed is the exception, which the view only narrows, described
under [Direct network paths](#direct-network-paths). Releases before macOS 14
have no per-store proxy configuration, so those clients do not advertise the
HTTP viewer and keep the remote HTTP placeholder.

macOS 14–26 have the proxy configuration but are not measured. Their WebKit
could treat a `*.localhost` alias as loopback and connect to the client itself,
which no rule here would block. Those releases keep the placeholder too. A
release is added once the measurements under [Measured behaviour](#measured-behaviour)
pass on it, in particular that alias hosts reach only the proxy while the same
port on the client serves a different response.

## Data store and proxy

Each HTTP document view has its own nonpersistent `WKWebsiteDataStore`. Cookies
and other storage live only as long as that view. Its `proxyConfigurations` hold
one HTTP CONNECT proxy for `127.0.0.1:<endpoint port>` with failover disabled and
no match or excluded domains. The endpoint uses `Transport::HttpProxy`, the same
Basic proxy front as on [Windows](windows-http-context.md). The Network functions
are bound at runtime with `dlopen` and `dlsym`, so the app links no macOS 14
symbol. The viewer is advertised only when `WKWebsiteDataStore` responds to
`setProxyConfigurations:` and those functions resolve.

WebKit sends a plain `http://` request through the same `CONNECT` tunnel as
HTTPS. The first request receives `407`. WebKit then asks the navigation
delegate, which answers a proxy challenge only when the document is current and
the challenge names the endpoint's own `127.0.0.1` address and port. WebKit does
not report the proxy realm, so the realm is not compared. The credential is not
stored, so a successor endpoint with new credentials is asked again. The server
removes `Proxy-Authorization` before it forwards a request, so a remote `407`
cannot obtain the token. A remote page's `401` continues without credentials and
shows its own response.

## Loopback aliases

WebKit connects to a loopback address directly even when a proxy is configured.
On Darwin 27.2 the bypass covered `localhost`, `localhost.`, every `127.0.0.0/8`
literal, `[::1]`, IPv4-mapped loopback, and numeric forms such as `2130706433`.
Neither the SOCKS nor the HTTP CONNECT proxy, nor a match domain for the loopback
host, changed this. Non-loopback names, including `*.localhost` and names whose
DNS answer is `127.0.0.1`, went through the proxy by name.

A remote loopback host is therefore addressed by an alias under `tasty.localhost`:

| Remote host | Alias |
|---|---|
| `localhost` | `localhost.tasty.localhost` |
| `127.a.b.c`, `0.0.0.0` | `127-a-b-c.tasty.localhost`, `0-0-0-0.tasty.localhost` |
| `::1` | `ipv6-loopback.tasty.localhost` |

- The view loads the entry on its alias. A main-frame navigation or redirect to a
  remote loopback address is cancelled and continues on the alias with the same
  request. The address bar, the script gate and the main-response check use the
  remote address.
- The endpoint reads an alias back as the remote host before its origin policy
  runs. The request target, `Host`, and same-origin `Origin` and `Referer` values
  are rewritten to the remote host. The TLS leaf and SNI use the alias, which is
  the name WebKit verifies.
- A `WKContentRuleList` installed before the first load blocks every direct
  loopback URL, including userinfo forms. WebKit's url-filter has no alternation,
  so each loopback form is its own rule. A failed compile leaves the view
  unloaded.
- `*.localhost` stays a secure context, so features that need one keep working.

The page sees its alias as `location.host`. A subresource, fetch or WebSocket that
names the remote loopback address absolutely is blocked; relative references work.

## Certificates and scripts

A server-trust challenge is accepted only when the SHA-256 of the presented leaf's
DER equals the leaf this endpoint generated for that host and port. Any other
certificate is refused. The system trust store is not changed.

Script enablement is fixed per navigation, so the view runs scripts natively and
the endpoint decides from the verified main response, as on Windows. A blocked
document receives `Content-Security-Policy: script-src 'none'` on every response.
Script Allow replays the verified response once instead of a new request.

## App Transport Security

The app bundle's `Info.plist` sets only `NSAllowsArbitraryLoadsInWebContent`. App
Transport Security then leaves WKWebView loads alone and still applies to the app's
`URLSession` connections. `NSAllowsArbitraryLoads` is absent, and the key's
presence makes ATS ignore it anyway.

`NSExceptionDomains` is deliberately absent. Apple documents that a listed domain is
not affected by the web content key and that its own dictionary replaces the global
keys, so listing `tasty.localhost` would leave alias HTTPS under ATS.

Without the key, measured on macOS 27.2 with the real bundle:

- Plain HTTP to an alias or to a public name such as `example.com` failed with
  "App Transport Security policy requires the use of a secure connection".
- HTTPS to `example.com` failed with "A TLS error caused the secure connection to
  fail" after the endpoint leaf above had been accepted. ATS requires a chain the
  system trusts, and the per-document leaf is not one.
- In both cases no request reached the remote server.

The exemption does not widen what a view trusts:

- Each remote certificate is verified by the server that fetches the page. An
  untrusted certificate (`self-signed.badssl.com`) was refused there with the key
  present, and the view never loaded.
- The view-to-endpoint hop still accepts only the exact endpoint leaf.
- A local HTML surface does not use this path. Its network addresses stay blocked
  by its content rules with or without the key.

The measurements in the next sections that predate the key ran outside an app
bundle, where ATS does not apply.

## Reload

A user or agent Reload keeps the same view and data store. The store's proxy
address is fixed at creation, so the successor document's endpoint binds the same
loopback port after the predecessor has joined, with a fresh token. The view then
clears the disk, memory and fetch caches and service-worker registrations, keeps
cookies, and calls `reload`, which resends the current entry with its original
method, body and cookies. A document without a Reload receipt is replaced by a new
view.

## Direct network paths

WebRTC and DNS prefetch do not follow `proxyConfigurations`. WebKit has no public
setting for either apart from Lockdown Mode.

- Before the view exists, its configuration gets one `WKUserScript` at document
  start for all frames. It deletes the same WebRTC constructors as on
  [Windows](windows-http-context.md#direct-network-paths) from the same script
  file. `addUserScript` reports no failure, so there is no error to surface. Local
  and entry views do not get the script.
- This is a mitigation of the measured paths, not a guarantee. Without it, a
  document whose scripts are allowed sent STUN over UDP from the client to an
  address the page named. With it, the constructors were absent in the main frame,
  in a child frame from the document's origin and in an `about:blank` frame the
  page created, and no STUN packet was sent. `WebAssembly`, CSS and images were
  unchanged. Other ways to reach a constructor were not measured.
- No DNS prefetch lookup was observed with the per-store proxy.
  `X-DNS-Prefetch-Control: off` did not stop a `dns-prefetch` link without a
  proxy, so the endpoint does not add it.
- `WKWebpagePreferences.lockdownModeEnabled` also removed `RTCPeerConnection`,
  but it removed `WebAssembly` too. The header does not say it affects WebRTC,
  and other changes such as fonts and JIT were not measured, so the view does not
  enable it.

These results come from a standalone WKWebView with a nonpersistent store, the
same single HTTP CONNECT proxy configuration and the same script file on macOS
27.2. A UDP trap counted STUN packets and a multicast DNS listener counted system
resolver lookups of `.local` names. Whether STUN server names are resolved
without the script was not determined, because no control showed that this
listener sees WebRTC's own lookups. The Tasty client itself was not measured.

## Measured behaviour

These were measured on macOS 27.2 with an isolated headless server and GUI client
on one machine. Each connection to the origin was attributed to its process.

- A page from `http://localhost` showed its CSS and image. Every origin connection
  came from the server process. An absolute `localhost` image, fetch and
  WebSocket were blocked, and no WebKit process connected to the origin.
- Script Allow ran the page's scripts without a second main request.
- Reload kept the view, sent the cookie, and kept the script allowance.
- An auto-submitted POST reached the origin once with `Origin` and `Referer`
  naming `localhost`. Reloading that document resent the same body and cookie
  once.
- A `302` to an absolute `localhost` address continued on the alias.
- `https://example.com` passed the leaf check and rendered, and its Reload
  accepted the successor endpoint's new leaf.
- Closing the tab released the endpoint listener.

A standalone WKWebView trap measured the bypass and the rule list: with the rules
installed, CSS, `@import`, images, iframes, media, ping, fetch, XHR, WebSocket,
EventSource, beacon, preload, form POST and redirects to loopback reached no
direct listener.
