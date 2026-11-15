- **Auto-update**: Neon Strider checks GitHub when it starts and once a day. System -> Updates
  shows the new version's release notes, including every version you skipped, with Install and
  Not now (Check now looks at once); the HUD shows `UPDATE READY // v1.3.0` once per version.
  Install downloads and verifies the signed update, then closes, installs and reopens by
  itself; "What's new" shows the notes once after the update.
- Installing an update never pauses or stops the belt. The treadmill keeps going while the app
  is closed for a few seconds (the HUD and shortcuts are unavailable then, so use the
  treadmill's own controls if needed); after the reconnect the walk carries on as the same
  session, and a running program carries on at the same step with its timer on time.
- The belt never starts by itself after a restart, reconnect or update: a program that finds
  the belt still comes back paused and waits for GO.
- GitHub releases now carry that version's notes from the changelog.
