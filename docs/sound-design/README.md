# Agent session sounds

Compact three-cue signature for the coding agent:

- [`done.wav`](../../crates/ui/assets/sounds/done.wav): completion, settling rounded pair.
- [`request.wav`](../../crates/ui/assets/sounds/request.wav): agent question, rising rounded pair.
- [`attention.wav`](../../crates/ui/assets/sounds/attention.wav): run failure, restrained downward rounded pair.

Regenerate assets:

```bash
python3 scripts/generate-notification-sounds.py
python3 scripts/generate-sound-auditions.py --install-attention
```

Coder Settings → Agent → **Session sounds** is the master switch. `HALO_DISABLE_SOUND` kills playback for the process.
