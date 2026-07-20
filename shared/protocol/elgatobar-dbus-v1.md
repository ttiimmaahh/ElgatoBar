# ElgatoBar D-Bus contract v1

The Linux daemon owns the well-known user-session bus name and interface `io.github.ttiimmaahh.ElgatoBar1` at `/io/github/ttiimmaahh/ElgatoBar1`. Clients never fall back to device HTTP when that name is unavailable. The daemon is the only Linux process that polls devices, mutates lights, or writes ElgatoBar application data.

## Stable device IDs

Every manager method selects devices with a canonical identity string, not an endpoint or list position:

- serial identities: `serial/` followed by lowercase hexadecimal UTF-8 bytes of the normalized serial;
- stable mDNS identities: `mdns/<instance-hex>/<product-hex>/<hardware-board>`;
- installation-local identities: `local/<lowercase-hyphenated-uuid>`.

Endpoints are mutable metadata for serial and stable-mDNS identities. Installation-local identities remain tied to their confirmed endpoint and cannot be silently reassociated.

## Native structures

`DeviceSnapshot` is the native D-Bus structure `(sssbbbyqus)` in this field order:

1. stable device ID (`s`);
2. user-visible name (`s`);
3. endpoint metadata (`s`);
4. online (`b`);
5. has last-known light state (`b`);
6. power (`b`);
7. brightness (`y`);
8. native Elgato temperature (`q`);
9. consecutive failed refresh cycles (`u`);
10. last actionable error (`s`).

Power, brightness, and temperature remain the last-known values after refresh failures. `has_state` distinguishes real cached values from zero placeholders before a first successful read.

`OperationResult` is `(ss(sssbbbyqus)ss)`: device ID, status, complete replacement snapshot, error kind, and error text. Status is `succeeded`, `failed`, or `skipped-offline`; error kind is empty on success and otherwise `connectivity`, `protocol`, or `offline`.

## Additive manager methods

| Member | Inputs | Output | Behavior |
| --- | --- | --- | --- |
| `ListDevices` | none | array of `DeviceSnapshot` | Returns all configured devices and cached state without network traffic. |
| `AddDevice` | endpoint `s` | `DeviceSnapshot` | Validates accessory-info and light-state before atomic persistence. A trusted existing serial/mDNS identity updates endpoint metadata instead of creating a duplicate. |
| `RemoveDevice` | device ID `s` | removed `DeviceSnapshot` | Removes local configuration only; never mutates hardware. |
| `DeviceSnapshot` | device ID `s` | `DeviceSnapshot` | Returns one cached snapshot without network traffic. |
| `RefreshDevice` | device ID `s` | `OperationResult` | Runs the normal retry/offline transition for one device. |
| `RefreshAll` | none | array of `OperationResult` | Refreshes all devices with at most eight requests in flight and returns every result. |
| `SetDeviceState` | device ID, presence flag, power, brightness, temperature | `OperationResult` | Serialized read/merge/full-write for one device. Zero means absent for brightness and temperature. |
| `ToggleDevice` | device ID `s` | `OperationResult` | Serialized read/invert/full-write for one device. |
| `IdentifyDevice` | device ID `s` | `OperationResult` | Requests physical identification. |
| `ToggleAll` | none | array of `OperationResult` | Skips offline devices. If any online light is on, targets all online lights off; otherwise targets them all on. |

Aggregate results always contain every configured target. One device failure does not erase successes; offline toggle targets are explicitly `skipped-offline`.

## Compatibility methods

The original `AccessoryInfo`, `Refresh`, `SetState`, `Toggle`, and `Identify` methods retain their original behavior only when exactly one device is configured. They return `InvalidInput` with a selection hint for zero or multiple devices. The original non-failing cached `Snapshot` returns an unavailable legacy snapshot with that hint in `last_error` when selection is ambiguous. This avoids silently reinterpreting old method names.

Brightness values are `3..=100`; native Elgato temperatures are `143..=344`.

## Signals

`DevicesChanged` carries the complete array of latest `DeviceSnapshot` values after configuration changes, polling, refreshes, and state mutations. Treat it as replacement state. When exactly one device is configured, the daemon also emits the legacy `StateChanged` complete snapshot.

## Refresh and offline rules

Polling uses the persisted interval (3, 5, 10, or 30 seconds; default 5), runs devices concurrently with at most eight in flight, and retries one failed attempt after 500 milliseconds. A failed cycle increments the failure count once. The first failed cycle preserves the prior online status; the second marks the device offline. A success resets the count, clears the error, and marks it online.

## Errors

Service errors use the prefix `io.github.ttiimmaahh.ElgatoBar1.Error` with `InvalidInput`, `Connectivity`, `Protocol`, and `Storage` variants. Failure to connect to the session bus or well-known service is a connectivity failure at the CLI seam.
