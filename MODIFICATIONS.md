# EsanCraft modifications

EsanCraft is a modified distribution based on SoundCraft by the ArtCraft team.
The upstream copyright, MIT/Apache-2.0 licenses, original notices and contributor
records are retained. EsanCraft has no upstream endorsement.

The upstream reference is SoundCraft commit `7dc56a4`; the original local fork base
is `ab7ede2`. On 2026-10-26 the distribution was initialized as SoundThai and then
renamed to EsanCraft. Git records the exact file changes and original authors.

Modified text files carry an EsanCraft change notice. Run
`python3 scripts/fork-notices.py --write` after staging changes, then
`python3 scripts/fork-notices.py` to check them. The index is checked so that
unrelated unstaged artwork is not silently included in the release.

Changes include application and package names, settings paths, public environment
variables, menus, update source, installers, source documentation, and the local
SoundCraft improvements already present in the original fork base. Internal
`soundcraft-*` crate names, command IDs and `.scraft` session compatibility remain.

The ArtCraft logo and mark files were removed from the current source tree.
Upstream copyright/attribution and the original brand terms remain as plain text.
Original upstream history is preserved; older commits can still contain the
original upstream artwork. Current application packages do not include it.

Distribution scripts now ship LICENSE, LICENSE-MIT, LICENSE-APACHE, NOTICE,
ATTRIBUTION.md, this file and THIRD-PARTY-LICENSES.txt. The latter reproduces the
bundled Noto font-family notices verbatim; NOTICE retains the dependency font texts.
The bundled font binaries and reserved font names were not modified by this rename.

Original third-party licenses and copyright texts are not annotated or rewritten.
Binary files that were only renamed keep their original bytes; their provenance
remains in ATTRIBUTION.md. The pre-existing, unstaged custom icon work remains
outside the rename commit and retains its own attribution in the working copy.
