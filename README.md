# SoundThai

**SoundThai** เป็นแอป Digital Audio Workstation (DAW) แยกจาก [SoundCraft](https://github.com/storytold/soundcraft) สำหรับพัฒนาต่อในโปรเจกต์ของตนเอง เขียนด้วย Rust และใช้ egui รองรับ macOS, Windows, Linux, FreeBSD และ Web/WASM ตามฐานโค้ดเดิม

Repository: [reoniazx/SoundThai](https://github.com/reoniazx/SoundThai)

ฐานเริ่มต้นคือ SoundCraft `main` ในเครื่องที่ commit `ab7ede2` (26 ตุลาคม 2026) รวมงานที่ commit แล้วของ fork เดิม โดยเก็บประวัติ Git และเครดิตผู้พัฒนาต้นฉบับไว้ทั้งหมด

> สถานะ: **pre-alpha** ตามฐาน SoundCraft ที่นำมาใช้ ยังมีข้อจำกัดด้านการบันทึกเสียง/MIDI และการแลกเปลี่ยน session ดู [ROADMAP.md](ROADMAP.md) การแยกแอปครั้งนี้ยังไม่ได้แปลเมนูทั้งหมดเป็นภาษาไทย

## รันแอป

ต้องมี Rust toolchain (ขั้นต่ำ Rust 1.90) บน Linux ต้องติดตั้งไลบรารีพัฒนา ALSA, Fontconfig, Wayland และ X11 เพิ่มเติม

```sh
git clone https://github.com/reoniazx/SoundThai.git
cd SoundThai
cargo run --release -p soundthai -- --demo
```

เปิดแอปเปล่า: `cargo run --release -p soundthai` หรือเปิด session `.scraft` ที่ใช้งานร่วมกับ SoundCraft ได้

```sh
# CLI / MCP
cargo run --release -p soundthai-cli -- --help
cargo run --release -p soundthai-cli -- mcp --demo

# Web (ต้องติดตั้ง trunk และ wasm32-unknown-unknown)
cd apps/soundthai-web
trunk serve --release
```

## สิ่งที่แยกเป็น SoundThai

- Desktop binary `soundthai` และชื่อแอป/เมนู **SoundThai**
- CLI `soundthai-cli` และเว็บ `soundthai-web`
- App ID `ai.storyteller.soundthai` แยกจาก SoundCraft
- ค่าตั้งค่า, autosave, presets และ logs แยกตามแอป
- ชื่อไฟล์ติดตั้ง, Windows upgrade/component IDs และ Flatpak ID แยกกัน
- ระบบอัปเดตชี้เฉพาะ releases ของ `reoniazx/SoundThai`

พื้นที่เก็บข้อมูลของแอป:

| ระบบ | ตำแหน่ง |
|---|---|
| macOS | `~/Library/Application Support/SoundThai/` |
| Windows | `%APPDATA%\SoundThai\` |
| Linux / FreeBSD | `$XDG_CONFIG_HOME/soundthai/` หรือ `~/.config/soundthai/` |
| Portable | `SoundThaiData/` ข้าง executable เมื่อมี `portable.txt` |

ใช้ `SOUNDTHAI_CONFIG_DIR` เพื่อเลือกโฟลเดอร์เอง หรือ `SOUNDTHAI_NO_PREFS=1` สำหรับรันโดยไม่อ่าน/เขียนค่าตั้งค่า Logs อยู่ใน `logs/soundthai.log`

## พัฒนาและตรวจสอบ

```sh
cargo xtask ci
```

ชื่อ crate ภายในยังใช้ `soundcraft-*` เพื่อให้รวมการแก้ไขจาก upstream ได้สะดวก สถาปัตยกรรม, command IDs และฟอร์แมต `.scraft` ยังคงเข้ากันได้ ดู [คู่มือ fork และ upstream](docs/soundthai-fork.md), [control protocol](docs/control-protocol.md), [MCP](docs/mcp.md) และ [roadmap](ROADMAP.md)

## Build และ release

สคริปต์ใน `packaging/` สร้างแพ็กเกจชื่อ SoundThai ตัวอย่างบน Apple silicon:

```sh
bash packaging/macos/package.sh --arch aarch64
```

ผลลัพธ์อยู่ที่ `dist/release/` การรันในเครื่องจะใช้ ad-hoc signing ถ้ายังไม่ได้ตั้งใบรับรอง ส่วน workflow Release เดิมต้องตั้ง signing secrets ของ SoundThai เองก่อน push สาขา `release` ดู [release playbook](docs/release-playbook.md) และ [fork guide](docs/soundthai-fork.md)

## ต้นฉบับและสิทธิ์การใช้งาน

SoundThai เป็น independent source fork ของ SoundCraft ไม่ได้เป็นผลิตภัณฑ์ของ ArtCraft หรือ Avid

โค้ดต้นฉบับคงสิทธิ์ **MIT OR Apache-2.0** ตาม [LICENSE-MIT](LICENSE-MIT) และ [LICENSE-APACHE](LICENSE-APACHE) ส่วน [LICENSE](LICENSE) เก็บ MIT license ของ SoundThai repository เดิมไว้ การคงไฟล์นี้ไม่ได้เปลี่ยน license ของโค้ดต้นฉบับ

ไอคอนและฟอนต์ที่นำมาใช้จาก upstream คงเครดิตและ license ใน [ATTRIBUTION.md](ATTRIBUTION.md) และ [NOTICE](NOTICE) รายชื่อผู้ร่วมพัฒนาคงอยู่ใน `contributors/` ส่วนเครื่องหมาย ArtCraft ภายใต้ `docs/brand/` เป็นของเจ้าของเดิมตามเงื่อนไขในโฟลเดอร์นั้น
