# SoundThai fork maintenance

Created: 2026-10-26. Base: committed local SoundCraft `main`, `ab7ede2` (including its 49 commits ahead of the locally tracked upstream). The two uncommitted audio changes in the SoundCraft working directory were not imported. The initial SoundThai `main` commit, `f1b12c7`, remains an ancestor with its LICENSE intact.

SoundThai is an independent Git repository with shared SoundCraft source history. GitHub's fork-network flag is not required: the target repository already existed independently.

## Repository setup

```sh
git clone https://github.com/reoniazx/SoundThai.git
cd SoundThai
git remote add upstream https://github.com/storytold/soundcraft.git
git fetch upstream
```

For the initial local checkout, `origin` is SoundThai, `upstream` is SoundCraft, and `source` records the local checkout used as the base. No remote in the original SoundCraft checkout is changed.

## Bring upstream changes into SoundThai

Start from a clean working directory. Preserve SoundThai branding, its settings paths and its update source when resolving conflicts.

```sh
git switch main
git fetch upstream
git switch -c sync/upstream-main
git merge upstream/main
# Resolve conflicts, then:
cargo xtask ci
# Commit merge resolution if needed, review the diff, then merge the branch into main.
```

App package directories are `apps/soundthai`, `apps/soundthai-cli` and `apps/soundthai-web`. Core library crates keep their original `soundcraft-*` names. Session JSON, command IDs, `.scraft`, and `application/x-soundcraft-session` retain the upstream format. macOS imports the upstream session UTI and offers SoundThai as an alternate handler.

## Independent identity

- App/Flatpak/desktop ID: `ai.storyteller.soundthai` (the inherited updater expects this namespace; distinct from `ai.storyteller.soundcraft`).
- Windows installer UpgradeCode and explicit component GUIDs are unique to SoundThai.
- Desktop settings and portable data never use SoundCraft's default settings folders.
- Environment variables exposed by the app use `SOUNDTHAI_`, including `SOUNDTHAI_CONTROL_PORT`, `SOUNDTHAI_CONFIG_DIR`, `SOUNDTHAI_NO_PREFS`, `SOUNDTHAI_WAYLAND` and `SOUNDTHAI_REQUIRE_WINRES`.
- Runtime update checks use `reoniazx/SoundThai`, never the SoundCraft release channel. Publish packages using the existing SoundThai artifact naming conventions.
- Current icon artwork is reused from upstream under MIT OR Apache-2.0, with original credits. The inherited custom DMG Finder layout is omitted because it names SoundCraft.app.

## CI and distribution

CI and packaging workflows reuse the public `storytold/craft-common-actions` workflows. Release inputs are `soundthai` / `SoundThai`. This shares build infrastructure, while source and releases remain in SoundThai.

The release workflow requires this repository's own Apple/Windows signing secrets for pushes to `release`; upstream secrets are not copied. A manual workflow_dispatch can be used for test builds, subject to the shared workflow's checks. No release is published by the initial fork setup.

SonarCloud is disabled by default because the upstream project/token does not belong to SoundThai. Configure a SoundThai project and credentials before setting the repository variable `ENABLE_SONARCLOUD=true`.

Inherited parity documents retain the upstream assessments and issue references; their numbers are not claims of additional SoundThai functionality. Full Thai UI localization remains separate future work.

## Legal and credits

Retain upstream copyright notices, MIT/Apache license files, NOTICE and asset attribution. LICENSE preserves the initial SoundThai repository's MIT notice. Do not rewrite contributors/contributors.json or add personal names without the contributor's consent.
