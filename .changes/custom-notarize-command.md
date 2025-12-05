---
tauri-bundler: minor:feat
tauri-utils: minor:feat
tauri-cli: minor:feat
---

Add custom notarization command support for macOS bundles.

Enables developers to specify custom notarization commands for .app bundles, .pkg installers, and .dmg disk images on macOS. This is useful for organizations that need to use proprietary notarization infrastructure (such as build servers or HSM-based solutions) instead of the native Apple notarization process.

The custom command is responsible for handling both notarization and stapling if desired. The existing `skipStapling` option only applies to native notarization.

Configuration example in tauri.conf.json:
```json
{
  "bundle": {
    "macOS": {
      "appNotarizeCommand": {
        "cmd": "./shims/notarize.sh",
        "args": ["%1"]
      },
      "dmgNotarizeCommand": {
        "cmd": "./shims/notarize.sh",
        "args": ["%1"]
      },
      "pkgNotarizeCommand": {
        "cmd": "./shims/notarize.sh",
        "args": ["%1"]
      }
    }
  }
}
```

The %1 placeholder in args is replaced with the path to the artifact being notarized. Commands run in the directory where `tauri build` was executed.

Configuration fields added to MacOsSettings:
- `appNotarizeCommand`: Custom command for notarizing .app bundles
- `dmgNotarizeCommand`: Custom command for notarizing .dmg disk images
- `pkgNotarizeCommand`: Custom command for notarizing .pkg installers
