# ElgatoBar D-Bus contract v1

The Linux daemon owns the well-known user-session bus name and primary interface `io.github.ttiimmaahh.ElgatoBar1` at `/io/github/ttiimmaahh/ElgatoBar1`. Clients never fall back to device HTTP when that name is unavailable.

## Methods

| Member | Inputs | Output | Behavior |
| --- | --- | --- | --- |
| `AccessoryInfo` | none | accessory snapshot | Reads the configured device's identity and firmware fields. |
| `Snapshot` | none | light snapshot | Returns cached state without network traffic. |
| `Refresh` | none | light snapshot | Polls immediately and updates cached state. |
| `SetState` | presence flag, power, brightness, temperature | light snapshot | Reads current state, merges supplied fields, and writes all fields. Zero means “not supplied” for brightness and temperature. |
| `Toggle` | none | light snapshot | Reads current state and writes the inverse power state while preserving other fields. |
| `Identify` | none | none | Requests physical identification. |

Brightness values are `3..=100`; native Elgato temperatures are `143..=344`. Mutating operations are serialized by the daemon so concurrent clients cannot interleave read-modify-write sequences.

## Snapshots and signals

A light snapshot contains the configured endpoint, online flag, power, brightness, native temperature, and last error text. On connectivity or protocol failure, the daemon retains the last-known power, brightness, and temperature, marks the snapshot offline, and records the error.

`StateChanged` carries the complete latest snapshot after polling or a state mutation, including failed polls. Clients should treat it as replacement state rather than a partial update.

## Errors

Service errors use the prefix `io.github.ttiimmaahh.ElgatoBar1.Error` with `InvalidInput`, `Connectivity`, and `Protocol` variants. Failure to connect to the session bus or acquire the well-known service is also a connectivity failure at the CLI boundary.
