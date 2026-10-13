# Android changelogs

Edit `android/changelogs.json` for each release. Gradle packages it into the app automatically. The top-level key is the Android `versionCode` from `app/build.gradle.kts`; `version` is the displayed version name. Each `notes` item is one bullet. Entries above the installed build are hidden.

```json
{
  "161": {
    "version": "2.0.5",
    "notes": [
      "Added **bold** text",
      "Also supports *italic*, __underline__, and ***__all three__***"
    ]
  }
}
```

Settings > Changelogs shows all bundled entries, newest first. Tap any version to expand or collapse it. After a Google Play update, the app shows notes newer than the last seen build once in the same full-screen layout. A fresh install does not get an update prompt. Formatting is limited to the markers above; release notes are bundled with the app and available offline.
