# EsanCraft license and branding audit

Reviewed 2026-10-26 for the SoundThai → EsanCraft rename. This is a repository and
packaging audit of the checked files, not a legal opinion or trademark clearance.

| Requirement | Evidence and action |
|---|---|
| Core license and copyright | LICENSE-MIT and LICENSE-APACHE retain the original text. LICENSE preserves the initial repository's MIT notice. Original SoundCraft/ArtCraft copyright remains in legal notices. Cargo still declares MIT OR Apache-2.0. |
| Modified-file notices | Modified text files carry an EsanCraft notice. MODIFICATIONS.md identifies the upstream base, fork changes and exclusions; scripts/fork-notices.py checks staged source relative to the pinned baseline. Original license/font texts are exempt from modification. |
| Upstream NOTICE | The original NOTICE is retained byte-for-byte as the prefix. A separate EsanCraft addendum distinguishes the modified distribution and notes the removed brand artwork. |
| Font/asset notices | ATTRIBUTION.md remains. THIRD-PARTY-LICENSES.txt reproduces all four bundled Noto-family OFL files and the CJK copyright file verbatim. NOTICE retains egui dependency-font texts. Font binaries and reserved font names are unchanged by the rename. |
| Branding | Eight ArtCraft logo/mark files were removed from the current tree. Original brand terms remain as plain-text provenance. Product UI, repo links, installer names and Windows CompanyName use EsanCraft. Upstream signing company/team examples were replaced with instructions to use the fork's own credentials. |
| Distribution | macOS app Resources, Windows MSI/portable packages, Linux/FreeBSD documentation and the web zip receive the code licenses, NOTICE, attribution, modification record and font-license supplement. The About dialog provides readable code/font license texts. |

## Scope and limits

Original upstream history is preserved. Older commits still contain original
upstream brand artwork; the current tree and packaging no longer use those marks.
The seven unchanged upstream UI screenshots under `docs/images/` were visually inspected; they show the SoundCraft interface and contain no ArtCraft logos/marks. They remain licensed upstream reference images, with their attribution retained.

Plain-text origin/copyright references to ArtCraft are retained where appropriate.
They do not represent sponsorship or endorsement.

Pre-existing custom icon/source-image edits remain unstaged and are excluded from
the rename commit. Their working-copy attribution records the supplied artwork's
author and license; this audit does not independently establish ownership of that
source image. The local development app uses the working copy. Review and commit
the icon work and its attribution together before distributing that icon publicly.

The local audit checks file contents, source notices and legal-file staging. It does
not claim that signed Windows/Linux/FreeBSD installers have all been built and
installed on their target systems, and does not constitute a full dependency-license
or software-patent audit. No commercial release is published by this work.

No search or legal clearance for third-party rights in the name EsanCraft was
performed. The source licenses permit modification but do not grant rights to the
upstream trademarks or to unrelated third-party marks.

## Sources checked

- [MIT license](https://opensource.org/license/mit): copyright and permission notice retention.
- [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0): sections 4(a)–(d) (redistribution/changes/notices) and 6 (trademarks).
- [SIL OFL 1.1](https://openfontlicense.org/open-font-license-official-text/): copyright/license retention and reserved names.
- [Original upstream brand terms](brand/LICENSE-brand.txt): forks must remove/replace ArtCraft marks; plain-text provenance is permitted.
- Repository LICENSE, LICENSE-MIT, LICENSE-APACHE, NOTICE and ATTRIBUTION.md.
