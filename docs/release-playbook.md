# Release playbook

SoundThai-specific setup, its independent release channel, and signing requirements are documented in [soundthai-fork.md](soundthai-fork.md). This repository must use its own signing credentials.

The release playbook shared by every crafting app lives in craftrules:
[`../craftrules/release/playbook.md`](../../craftrules/release/playbook.md)
(`storytold/craftrules`). It is the canonical copy; don't duplicate it here.

SoundCraft's implementation (`.github/workflows/release.yml`, `packaging/`,
`cargo xtask version`) was copied from DesignCraft/PhotoCraft and renamed. SoundCraft-specific
details are in [releasing.md](releasing.md).
