# Elgato local HTTP contract

ElgatoBar controls supported lights over **unauthenticated plaintext HTTP** on the local network. The default port is `9123`. This protocol provides no server authentication, client authentication, confidentiality, or integrity protection; use it only on a trusted LAN. ElgatoBar rejects non-HTTP endpoints, URL credentials, paths, queries, fragments, redirects, and inherited HTTP proxies so requests cannot be silently rerouted.

Each device request has a five-second default timeout. Any `2xx` response is successful. Timeouts and connection failures are connectivity errors; non-success status codes, malformed JSON, invalid domain values, and empty light arrays are protocol errors.

## Endpoints

| Intent | Method | Path | Body |
| --- | --- | --- | --- |
| Read light state | `GET` | `/elgato/lights` | none |
| Set light state | `PUT` | `/elgato/lights` | full one-light state |
| Read accessory information | `GET` | `/elgato/accessory-info` | none |
| Identify a light | `POST` | `/elgato/identify` | none |

State reads decode the first member of `lights` and reject an empty array. Power is on only when the wire `on` value is exactly `1`. ElgatoBar constrains brightness to `3..=100` and native temperature to `143..=344`.

A write always sends all three state fields, even when the user changed only one:

```json
{"lights":[{"on":0,"brightness":75,"temperature":200}]}
```

Set and toggle operations first read current state, merge the requested change, and then write the complete state. This preserves brightness and temperature during power changes and matches the existing macOS behavior.

The accessory response requires `productName`, `hardwareBoardType`, `firmwareBuildNumber`, `firmwareVersion`, and `serialNumber`. `displayName`, `features`, and `wifi-info` are optional. `displayName` is preferred when it is present and non-empty.

Recorded examples live in `shared/api-fixtures/`. They are protocol evidence, not credentials or live-device captures.
