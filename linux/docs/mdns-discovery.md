# Linux mDNS discovery research

Status: implementation recommendation, researched 2026-07-20.

## Recommendation

Implement `_elg._tcp` discovery through Avahi's system-bus D-Bus API using the
workspace's existing zbus/Tokio stack. Keep manual hostname/IP entry as a
first-class fallback when Avahi is absent, disabled, or blocked by the network.

Avahi already owns Linux mDNS traffic and maintains the host's record cache; its
daemon documents D-Bus as the rich object-oriented API for local applications.
Its configuration also explicitly discourages multiple mDNS stacks because they
reduce reliability and waste resources. Calling Avahi over D-Bus therefore has
the cleanest coexistence story and adds no C FFI or development-header build
dependency.

Sources: [Arch `avahi-daemon(8)`][arch-daemon],
[Arch `avahi-daemon.conf(5)`][arch-config].

## Proposed discovery flow

1. Connect to the system bus name `org.freedesktop.Avahi` and server object `/`.
2. Call `ServiceBrowserPrepare` for all interfaces/protocols, service type
   `_elg._tcp`, domain `local`, and no lookup flags.
3. Subscribe to the returned browser object's `ItemNew`, `ItemRemove`,
   `Failure`, `AllForNow`, and `CacheExhausted` signals **before** calling
   `Start`. The prepared API exists specifically to separate object creation
   from starting the browser, avoiding an early-signal subscription race.
4. Resolve each `ItemNew` with `ResolveService`, preserving the interface and
   protocol supplied by the event. Request IPv4 initially because the current
   Elgato HTTP endpoint model does not encode an IPv6 scope identifier. The
   result provides the host name, address, port, TXT data, and lookup flags.
5. Normalize and deduplicate results before validation. A service can arrive on
   more than one interface/address family; the discovery adapter should expose
   one candidate per service identity and usable endpoint.
6. Bound an explicit scan with a three-to-five-second Tokio timeout. Treat
   `AllForNow` as "the current cache has been enumerated," not as proof that a
   late multicast response cannot arrive. A short debounce after `AllForNow`
   can make an initial scan feel fast while the hard timeout remains the bound.
7. Always call the browser object's `Free` on completion, cancellation, or
   error. Treat Avahi name loss/unavailability as a recoverable discovery error;
   it must not stop the ElgatoBar daemon or prevent manual add.

The server, browser, and resolver method/signal contracts are defined in
Avahi's own D-Bus interface XML: [Server][avahi-server],
[ServiceBrowser][avahi-browser], and [ServiceResolver][avahi-resolver].

## Adapter and test design

Keep Avahi types below a narrow async discovery seam, for example:

```text
Discoverer::scan(deadline) -> stream/list of DiscoveredEndpoint
DiscoveredEndpoint { service_instance, host, address, port, txt }
```

The daemon should own discovery and expose results through the typed ElgatoBar
D-Bus contract; GTK must not perform independent network discovery. This keeps
Waybar and GTK exclusively daemon-backed and gives every client the same device
set.

Test in layers:

- Unit tests use a fake discovery event stream to cover normalization,
  deduplication, `AllForNow`, late results, timeout, cancellation, Avahi loss,
  and partial resolution failures.
- Isolated D-Bus tests provide only the small Avahi subset used by the adapter:
  prepare/start/free, browser signals, and service resolution. This matches the
  repository's existing isolated zbus testing style and needs no multicast.
- One opt-in native Avahi acceptance test verifies the real system bus and
  `_elg._tcp` browse path.
- Real-light acceptance confirms that discovered candidates pass the existing
  accessory-info validation before persistence. CI must not require Avahi,
  multicast networking, or physical hardware.

## Alternatives considered

### `mdns-sd` 0.20

This is the best portable fallback: it is pure Rust, runs its own thread, emits
found/resolved/removed events, offers async channel receiving, interface
controls, `stop_browse`, and `shutdown`. Its socket implementation sets
`SO_REUSEADDR` and attempts `SO_REUSEPORT`, which follows RFC 6762's basic
coexistence recommendation. It still creates a second host mDNS stack, however,
which Avahi itself advises against. Socket initialization errors are also lazy
and must be observed through the daemon monitor. Prefer it only if a later
cross-platform requirement outweighs Linux integration.

Sources: [`ServiceDaemon` API][mdns-daemon], [`ServiceEvent` API][mdns-events],
[`ResolvedService` API][mdns-resolved], [socket setup source][mdns-socket], and
[RFC 6762 section 15][rfc6762-multiple].

### `zeroconf` 0.18

On Linux this crate wraps Avahi and therefore coexists correctly, but it links
through `avahi-sys`, requires native Avahi client development libraries, and
uses a callback/event-loop polling model. Direct zbus access avoids that FFI and
packaging surface and fits the daemon's Tokio lifecycle better.

Sources: [`zeroconf` crate documentation][zeroconf-docs] and
[`zeroconf` upstream prerequisites][zeroconf-prereqs].

[arch-daemon]: https://man.archlinux.org/man/avahi-daemon.8.en
[arch-config]: https://man.archlinux.org/man/avahi-daemon.conf.5.en
[avahi-server]: https://github.com/avahi/avahi/blob/master/avahi-daemon/org.freedesktop.Avahi.Server.xml
[avahi-browser]: https://github.com/avahi/avahi/blob/master/avahi-daemon/org.freedesktop.Avahi.ServiceBrowser.xml
[avahi-resolver]: https://github.com/avahi/avahi/blob/master/avahi-daemon/org.freedesktop.Avahi.ServiceResolver.xml
[mdns-daemon]: https://docs.rs/mdns-sd/0.20.2/mdns_sd/struct.ServiceDaemon.html
[mdns-events]: https://docs.rs/mdns-sd/0.20.2/mdns_sd/enum.ServiceEvent.html
[mdns-resolved]: https://docs.rs/mdns-sd/0.20.2/mdns_sd/struct.ResolvedService.html
[mdns-socket]: https://docs.rs/mdns-sd/0.20.2/src/mdns_sd/service_daemon.rs.html#877-907
[rfc6762-multiple]: https://www.rfc-editor.org/rfc/rfc6762.html#section-15
[zeroconf-docs]: https://docs.rs/zeroconf/0.18.0/zeroconf/
[zeroconf-prereqs]: https://github.com/windy1/zeroconf-rs#prerequisites
