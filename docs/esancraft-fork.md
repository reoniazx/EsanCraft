> Modified for EsanCraft; original notices retained. See MODIFICATIONS.md.

# EsanCraft fork maintenance

Renamed from SoundThai on 2026-10-26. Git history, licenses and the original fork base are retained.

Created: 2026-10-26. Base: committed local SoundCraft `main`, `ab7ede2` (including its 49 commits ahead of the locally tracked upstream). The two uncommitted audio changes in the SoundCraft working directory were not imported. The initial EsanCraft `main` commit, `f1b12c7`, remains an ancestor with its LICENSE intact.

EsanCraft is an independent Git repository with shared SoundCraft source history. GitHub's fork-network flag is not required: the target repository already existed independently.

## Repository setup

```sh
git clone https://github.com/reoniazx/EsanCraft.git
cd EsanCraft
git remote add upstream https://github.com/storytold/soundcraft.git
git fetch upstream
```

For the initial local checkout, `origin` is EsanCraft, `upstream` is SoundCraft, and `source` records the local checkout used as the base. No remote in the original SoundCraft checkout is changed.

## Bring upstream changes into EsanCraft

Start from a clean working directory. Preserve EsanCraft branding, its settings paths and its update source when resolving conflicts.

```sh
git switch main
git fetch upstream
git switch -c sync/upstream-main
git merge upstream/main
# Resolve conflicts, then:
cargo xtask ci
# Commit merge resolution if needed, review the diff, then merge the branch into main.
```

App package directories are `apps/esancraft`, `apps/esancraft-cli` and `apps/esancraft-web`. Core library crates keep their original `soundcraft-*` names. Session JSON, command IDs, `.scraft`, and `application/x-soundcraft-session` retain the upstream format. macOS imports the upstream session UTI and offers EsanCraft as an alternate handler.

## Independent identity

- App/Flatpak/desktop ID: `ai.storyteller.esancraft` (the inherited updater expects this namespace; distinct from `ai.storyteller.soundcraft`).
- Windows installer UpgradeCode and explicit component GUIDs are unique to EsanCraft.
- Desktop settings and portable data never use SoundCraft's default settings folders.
- Environment variables exposed by the app use `ESANCRAFT_`, including `ESANCRAFT_CONTROL_PORT`, `ESANCRAFT_CONFIG_DIR`, `ESANCRAFT_NO_PREFS`, `ESANCRAFT_WAYLAND` and `ESANCRAFT_REQUIRE_WINRES`.
- Runtime update checks use `reoniazx/EsanCraft`, never the SoundCraft release channel. Publish packages using the existing EsanCraft artifact naming conventions.
- Current icon artwork is reused from upstream under MIT OR Apache-2.0, with original credits. The inherited custom DMG Finder layout is omitted because it names SoundCraft.app.

## CI and distribution

CI and packaging workflows reuse the public `storytold/craft-common-actions` workflows. Release inputs are `esancraft` / `EsanCraft`. This shares build infrastructure, while source and releases remain in EsanCraft.

The release workflow requires this repository's own Apple/Windows signing secrets for pushes to `release`; upstream secrets are not copied. A manual workflow_dispatch can be used for test builds, subject to the shared workflow's checks. No release is published by the initial fork setup.

SonarCloud is disabled by default because the upstream project/token does not belong to EsanCraft. Configure a EsanCraft project and credentials before setting the repository variable `ENABLE_SONARCLOUD=true`.

Inherited parity documents retain the upstream assessments and issue references; their numbers are not claims of additional EsanCraft functionality. Full Thai UI localization remains separate future work.

## Legal and credits

Retain upstream copyright notices, MIT/Apache license files, NOTICE and asset attribution. LICENSE preserves the initial EsanCraft repository's MIT notice. Do not rewrite contributors/contributors.json or add personal names without the contributor's consent.

## Settings from the former name

The app now uses `EsanCraft`, `esancraft`, `EsanCraftData` and `ESANCRAFT_` for its settings paths, portable data and public environment variables. Existing SoundThai settings are left in their original folders; to keep using them, point `ESANCRAFT_CONFIG_DIR` to that folder or copy the settings to the new EsanCraft folder while both apps are closed. Session files still use `.scraft`.


License/branding audit and distribution evidence: [license-audit.md](license-audit.md).
