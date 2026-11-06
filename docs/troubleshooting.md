# Troubleshooting

## Host says the plugin "was not installed" / "not installed or not scanned"

LYNX shows this generic message whenever an insert fails to load. Missing
files are only one cause. Work through these in order:

1. **Prove the plugin itself works.** Run `./build2lynx.sh <name>`. If the
   `clap-smoke` step prints `PASS`, the bundle loads, processes audio and
   saves state, and the problem is on the host side.
2. **Try a known-good third-party plugin.** If that fails too, the host is
   at fault, not your plugin.
3. **Close and reopen the plugin or mixer window**, or restart the host,
   after reinstalling. On 2026-11-07 a LYNX window kept showing "not
   installed" after a reinstall, and reopening the window fixed it.
4. **Rescan plugins** (LYNX: Options > Scan for plugins), then remove the
   old insert and add a fresh one.
5. **Toggle the host's separate-process loading.** LYNX has "Always run in
   a separate process" per plugin, and a global sandbox setting.

## Useful places to look (LYNX)

| Path | What it tells you |
|---|---|
| `~/Library/Application Support/LYNX/plugins.xml` | Scan results: name, format, path and ID for each plugin |
| `~/Library/Application Support/LYNX/sessions/` | Current session log, including which plugins loaded and how |
| `~/Library/Application Support/LYNX/crash-history.log` | Sessions that ended without closing cleanly |
| `~/Library/Application Support/LYNX/autosave/*.lynx` | Project JSON. A plugin entry with `"state": ""` means no instance was ever created |
| `~/Library/Logs/DiagnosticReports/` | macOS crash reports |

LYNX refers to a plugin as `<FORMAT>-<Name>-<hash(path)>-<uniqueId>`, using
JUCE's scheme. The hash is a 31-based string hash of the full bundle path,
so moving or renaming a bundle changes the ID.

## Checking a bundle by hand

```sh
B="$HOME/Library/Audio/Plug-Ins/VST3/Dragon 24EQ.vst3"
lipo -info "$B/Contents/MacOS/Dragon 24EQ"        # x86_64 arm64
codesign -dv "$B" 2>&1 | grep Signature              # adhoc
nm -gU "$B/Contents/MacOS/Dragon 24EQ" | grep -E "GetPluginFactory|clap_entry"
xattr -l "$B"                                        # no com.apple.quarantine
```

## Renamed a plugin and now there are two

The installer only replaces bundles with the *current* name. Delete the old
ones from `~/Library/Audio/Plug-Ins/{VST3,CLAP}/` and rescan. If the CLAP or
VST3 ID changed too, existing project inserts will show as missing: replace
them with the new plugin.

## Build warnings from `nih_export_vst3!`

Warnings like "trailing semicolon in macro used in expression position"
come from inside nih-plug's VST3 macro, not from this code. They're safe
to ignore until nih-plug fixes them upstream.
