# ElgatoBar Linux storage contract v1

The daemon is the sole writer. Device inventory is stored at `$XDG_DATA_HOME/elgatobar/devices-v1.json` (fallback `~/.local/share/elgatobar/devices-v1.json`). Settings are stored at `$XDG_CONFIG_HOME/elgatobar/settings-v1.json` (fallback `~/.config/elgatobar/settings-v1.json`). No runtime/state document is needed in this phase.

Both documents have an integer `version` field fixed at `1`; unknown future versions are rejected with an upgrade-oriented error and are never rewritten. Device entries reuse the validated `PersistedDevice` domain representation, but this inventory is intentionally separate from the scene-capable cross-platform interchange document.

The settings document contains `refreshIntervalSeconds`, restricted to `3`, `5`, `10`, or `30` and defaulting to `5` when the document does not exist.

Writes encode the complete document first, create a unique same-directory temporary file without overwriting an existing file, write all bytes, synchronize file contents and metadata, atomically rename over the destination, and synchronize the containing directory. Failures before replacement leave the prior valid destination untouched and remove the temporary file when possible.
