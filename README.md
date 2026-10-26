> Modified for EsanCraft; original notices retained. See MODIFICATIONS.md.

# EsanCraft

เดิมใช้ชื่อ **SoundThai** — เปลี่ยนชื่อเป็น **EsanCraft** เมื่อ 26 ตุลาคม 2026 โดยคงประวัติและฐานโค้ดเดิมไว้

**EsanCraft** เป็นแอป Digital Audio Workstation (DAW) แยกจาก [SoundCraft](https://github.com/storytold/soundcraft) สำหรับพัฒนาต่อในโปรเจกต์ของตนเอง เขียนด้วย Rust และใช้ egui รองรับ macOS, Windows, Linux, FreeBSD และ Web/WASM ตามฐานโค้ดเดิม

Repository: [reoniazx/EsanCraft](https://github.com/reoniazx/EsanCraft)

ฐานเริ่มต้นคือ SoundCraft `main` ในเครื่องที่ commit `ab7ede2` (26 ตุลาคม 2026) รวมงานที่ commit แล้วของ fork เดิม โดยเก็บประวัติ Git และเครดิตผู้พัฒนาต้นฉบับไว้ทั้งหมด

> สถานะ: **pre-alpha** ตามฐาน SoundCraft ที่นำมาใช้ ยังมีข้อจำกัดด้านการบันทึกเสียง/MIDI และการแลกเปลี่ยน session ดู [ROADMAP.md](ROADMAP.md) การแยกแอปครั้งนี้ยังไม่ได้แปลเมนูทั้งหมดเป็นภาษาไทย

## รันแอป

ต้องมี Rust toolchain (ขั้นต่ำ Rust 1.90) บน Linux ต้องติดตั้งไลบรารีพัฒนา ALSA, Fontconfig, Wayland และ X11 เพิ่มเติม

```sh
git clone https://github.com/reoniazx/EsanCraft.git
cd EsanCraft
cargo run --release -p esancraft -- --demo
```

เปิดแอปเปล่า: `cargo run --release -p esancraft` หรือเปิด session `.scraft` ที่ใช้งานร่วมกับ SoundCraft ได้

```sh
# CLI / MCP
cargo run --release -p esancraft-cli -- --help
cargo run --release -p esancraft-cli -- mcp --demo

# Web (ต้องติดตั้ง trunk และ wasm32-unknown-unknown)
cd apps/esancraft-web
trunk serve --release
```

## สิ่งที่แยกเป็น EsanCraft

- Desktop binary `esancraft` และชื่อแอป/เมนู **EsanCraft**
- CLI `esancraft-cli` และเว็บ `esancraft-web`
- App ID `ai.storyteller.esancraft` แยกจาก SoundCraft
- ค่าตั้งค่า, autosave, presets และ logs แยกตามแอป
- ชื่อไฟล์ติดตั้ง, Windows upgrade/component IDs และ Flatpak ID แยกกัน
- ระบบอัปเดตชี้เฉพาะ releases ของ `reoniazx/EsanCraft`

พื้นที่เก็บข้อมูลของแอป:

| ระบบ | ตำแหน่ง |
|---|---|
| macOS | `~/Library/Application Support/EsanCraft/` |
| Windows | `%APPDATA%\EsanCraft\` |
| Linux / FreeBSD | `$XDG_CONFIG_HOME/esancraft/` หรือ `~/.config/esancraft/` |
| Portable | `EsanCraftData/` ข้าง executable เมื่อมี `portable.txt` |

ใช้ `ESANCRAFT_CONFIG_DIR` เพื่อเลือกโฟลเดอร์เอง หรือ `ESANCRAFT_NO_PREFS=1` สำหรับรันโดยไม่อ่าน/เขียนค่าตั้งค่า Logs อยู่ใน `logs/esancraft.log`

## พัฒนาและตรวจสอบ

```sh
cargo xtask ci
```

ชื่อ crate ภายในยังใช้ `soundcraft-*` เพื่อให้รวมการแก้ไขจาก upstream ได้สะดวก สถาปัตยกรรม, command IDs และฟอร์แมต `.scraft` ยังคงเข้ากันได้ ดู [คู่มือ fork และ upstream](docs/esancraft-fork.md), [control protocol](docs/control-protocol.md), [MCP](docs/mcp.md) และ [roadmap](ROADMAP.md)

## Build และ release

สคริปต์ใน `packaging/` สร้างแพ็กเกจชื่อ EsanCraft ตัวอย่างบน Apple silicon:

```sh
bash packaging/macos/package.sh --arch aarch64
```

ผลลัพธ์อยู่ที่ `dist/release/` การรันในเครื่องจะใช้ ad-hoc signing ถ้ายังไม่ได้ตั้งใบรับรอง ส่วน workflow Release เดิมต้องตั้ง signing secrets ของ EsanCraft เองก่อน push สาขา `release` ดู [release playbook](docs/release-playbook.md) และ [fork guide](docs/esancraft-fork.md)

## ต้นฉบับและสิทธิ์การใช้งาน

EsanCraft เป็น independent source fork: based on SoundCraft by the ArtCraft team. ผู้พัฒนา upstream ไม่ได้ให้การรับรอง EsanCraft

โค้ดต้นฉบับคงสิทธิ์ **MIT OR Apache-2.0** ตาม [LICENSE-MIT](LICENSE-MIT) และ [LICENSE-APACHE](LICENSE-APACHE) ส่วน [LICENSE](LICENSE) เก็บ MIT license ของ EsanCraft repository เดิมไว้ การคงไฟล์นี้ไม่ได้เปลี่ยน license ของโค้ดต้นฉบับ

ไอคอนและฟอนต์ที่นำมาใช้จาก upstream คงเครดิตและ license ใน [ATTRIBUTION.md](ATTRIBUTION.md) และ [NOTICE](NOTICE) รายชื่อผู้ร่วมพัฒนาคงอยู่ใน `contributors/` เครื่องหมายและโลโก้ ArtCraft ถูกนำออกจากไฟล์ปัจจุบันของ fork แล้ว โดยคงเครดิตและประกาศลิขสิทธิ์เดิมไว้


License/branding audit and distribution evidence: [docs/license-audit.md](docs/license-audit.md).
