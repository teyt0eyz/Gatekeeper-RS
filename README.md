<div align="center">

# Gatekeeper-RS

**Infrastructure Health Monitor & Self-Healing Engine — built in Rust**

[![Made with Rust](https://img.shields.io/badge/Made%20with-Rust-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![TUI: Ratatui](https://img.shields.io/badge/TUI-Ratatui-7B2CBF)](https://ratatui.rs/)
[![Async: Tokio](https://img.shields.io/badge/Async-Tokio-1F6FEB)](https://tokio.rs/)
[![Platform: Linux](https://img.shields.io/badge/Platform-Linux%20%7C%20macOS-success)]()
[![Status: v2.0](https://img.shields.io/badge/version-v2.0-brightgreen)]()

*"ผู้เฝ้าประตู" สำหรับ Infrastructure ของคุณ — ตรวจจับ, แจ้งเตือน, และซ่อมแซมอัตโนมัติ*

</div>

---

## 📖 Overview

**Gatekeeper-RS** คือเครื่องมือ Site Reliability & Auto-healing ที่ออกแบบมาเพื่อทีม **DevOps / SRE / System Administrator** ทำงานในรูปแบบ Terminal UI (TUI) บนเซิร์ฟเวอร์ Linux โดยตรง

โปรเจกต์นี้ถือกำเนิดขึ้นเพื่อแก้ปัญหาที่พบบ่อยใน production:

- 🌙 **Service ล่มกลางดึก** — ไม่มีคนเฝ้าหน้าจอ → กว่าจะรู้ก็ผ่านไป 30 นาที
- 🔔 **Alert ล้น Slack/Discord** — แต่ไม่มีใครซ่อมจริง
- 📉 **ขาด visibility** — uptime, latency, ทรัพยากร host รวมอยู่คนละที่
- ⚙️ **Tools เดิมหนัก** — Prometheus + Alertmanager + Grafana = ตั้งง่ายช่วงสั้น แต่ดูแลรายวันลำบาก

Gatekeeper-RS รวมทุกอย่างไว้ในไบนารีเดียว ไม่มี database, ไม่ต้องมี agent, ใช้ทรัพยากรน้อยมาก เหมาะกับ **homelab, Proxmox node, single VPS, edge device**

---

## ✨ What's New in v2.0

> v2.0 เปลี่ยนจาก "monitor ตัวเล็กๆ" เป็น **SRE automation suite** เต็มตัว

| Feature | Description |
|---|---|
| 🎛️ **Menu-Driven Setup Wizard** | TUI Wizard แบบหลายหน้าจอ (Main Menu → Global Settings / Service List / Service Edit) ใช้งานง่าย ใส่ข้อมูลครบทุกฟิลด์โดยไม่ต้องเขียน TOML เอง |
| 🔍 **Local Service Discovery** | กด `S` ใน Wizard → รัน `systemctl list-units` แล้วเลือก service จริงในเครื่อง — auto-fill `Name` + `restart_command` |
| 🔧 **Maintenance Mode** | กด `M` ใน Dashboard → หยุด heal + alert ชั่วคราวขณะทำ patching/deployment โดยที่ monitoring ยังทำงานต่อ |
| 🧪 **Dry-Run Mode** | `--dry-run` flag → ทดสอบ detection + alert จริง แต่ไม่สั่ง restart จริง — เหมาะกับการ verify config ก่อน deploy |
| 🔄 **Hot-Reload Config** | กด `R` ใน Dashboard → reload `config.toml` แบบ runtime ไม่ต้องรีสตาร์ตโปรแกรม |
| 📅 **Local-Time Daily Logs** | log rotate ตาม **เวลา local ของเครื่อง** (ไม่ใช่ UTC) — ไฟล์ `gatekeeper.YYYY-MM-DD.log` ตรงกับวันที่จริงในประเทศไทย |
| 📊 **Live Sparklines** | กราฟ rolling 20 จุดของ latency รายตัว service พร้อมสีตาม status |
| 💻 **Host Health Sidebar** | gauge CPU + RAM แบบ real-time พร้อม threshold สี (เขียว/เหลือง/แดง) |

---

## 🏗️ Architecture

```
   ┌──────────────────────────────────────────────────────────────┐
   │                       Gatekeeper-RS                          │
   │                                                              │
   │   ┌────────────────┐    ┌──────────────────┐                 │
   │   │ sysinfo thread │───▶│                  │                 │
   │   └────────────────┘    │                  │                 │
   │                         │   mpsc channel   │                 │
   │   ┌────────────────┐    │   (UiUpdate)     │    ┌─────────┐  │
   │   │ checker tasks  │───▶│                  │───▶│ TUI     │  │
   │   │ (× N services) │    │                  │    │ render  │  │
   │   └───────┬────────┘    └──────────────────┘    └─────────┘  │
   │           │                                                  │
   │           ▼                                                  │
   │   ┌────────────────┐    ┌──────────────────┐                 │
   │   │ healer (exec)  │    │ notifier (HTTP)  │                 │
   │   └────────────────┘    └──────────────────┘                 │
   │           │                       │                          │
   └───────────┼───────────────────────┼──────────────────────────┘
               ▼                       ▼
       systemctl restart …      Discord webhook
```

**Design highlights:**
- **Concurrent checks** ผ่าน `tokio::spawn` — service จำนวนมากตรวจขนานกันโดยไม่ block
- **State-transition–based heal** — ยิง `restart_command` เฉพาะตอน UP→DOWN เท่านั้น ไม่ยิงทุก poll
- **Lock-free maintenance flag** ผ่าน `Arc<AtomicBool>` — toggle ได้รวดเร็วโดยไม่ต้องรอ task อื่น
- **Hot-reloadable config** ผ่าน `Arc<RwLock<Config>>` — checker task อ่านค่าใหม่ทุก tick

---

## 🚀 Quick Start

```bash
# 1. Build
git clone <repo-url> gatekeeper-rs && cd gatekeeper-rs
cargo build --release

# 2. ตั้งค่าครั้งแรก (Wizard)
./target/release/gatekeeper-rs --config

# 3. รัน Monitor
./target/release/gatekeeper-rs
```

แค่ 3 ขั้นตอน — Wizard จะถามคำถามทุกอย่างและสร้าง `config.toml` ให้

---

## 📦 Installation

### Prerequisites

| Requirement | Note |
|---|---|
| **Rust toolchain** ≥ 1.75 | ติดตั้งผ่าน [rustup](https://rustup.rs/) |
| **Linux + systemd** *(แนะนำ)* | สำหรับฟีเจอร์ Service Discovery (บน macOS/Windows ใช้ฟีเจอร์อื่นได้ปกติ) |
| **sudo NOPASSWD** สำหรับคำสั่ง restart | ดูหัวข้อ [Best Practices](#-best-practices) ด้านล่าง |

### Build from source

```bash
# Debug build (เร็วในการ compile, ไบนารีใหญ่กว่า, เหมาะกับ dev)
cargo build

# Release build (compile นานกว่า, optimized, เหมาะกับ production)
cargo build --release
```

ไบนารีจะอยู่ที่:
- Debug: `./target/debug/gatekeeper-rs`
- Release: `./target/release/gatekeeper-rs`

---

## ⚙️ Configuration

### ผ่าน Setup Wizard (แนะนำ)

```bash
./gatekeeper-rs --config
```

Wizard มี 4 หน้าจอหลัก:

```
┌────────────────────────┐
│      MAIN MENU         │
│  > Edit Global Settings │  ──▶  กรอก Discord Webhook + Polling Interval
│    Manage Services     │  ──▶  ดู / แก้ / ลบ service ที่มีอยู่
│    Add New Service     │  ──▶  เพิ่ม service ใหม่
│    Save & Exit         │  ──▶  เขียนลง config.toml
└────────────────────────┘
```

#### ✨ Service Discovery (กด `S`)

ในหน้า **Manage Services** หรือ **Main Menu** กด `S` → ระบบจะรัน:

```bash
systemctl list-units --type=service --state=running --no-pager --plain
```

แล้วแสดงรายการ service ที่กำลังรันให้เลือก เมื่อกด `Enter` ระบบจะ auto-fill:
- **Name:** ชื่อ unit (ตัด `.service` ออก)
- **Restart Command:** `sudo systemctl restart <unit>.service`

เหลือแค่กรอก **URL/Address** ของ service เอง

> **หมายเหตุ:** ฟีเจอร์นี้ใช้ได้เฉพาะ Linux ที่มี systemd บน macOS/Windows จะแสดง `Scanner only available on Linux/Systemd.`

### ผ่านไฟล์ TOML โดยตรง

หากต้องการแก้ไขเอง ไฟล์ `config.toml` มีโครงสร้างดังนี้:

```toml
interval_secs = 30
discord_webhook_url = "https://discord.com/api/webhooks/123/abc"

[[services]]
name = "nginx"
url = "http://localhost:80/health"
method = "HTTP"
restart_command = "sudo systemctl restart nginx.service"

[[services]]
name = "redis"
url = "127.0.0.1:6379"
method = "TCP"
restart_command = "sudo systemctl restart redis.service"
```

### Test-Mode Override

หาก `testconfig.toml` มีอยู่ในโฟลเดอร์เดียวกัน ระบบจะ **overlay** ทับ `config.toml` field-by-field — เหมาะกับการทดสอบบน staging/local โดยไม่ต้องแก้ไฟล์ production

---

## 🎮 Operation Modes

### Monitor Mode (Default)

```bash
./gatekeeper-rs                       # ใช้ config.toml ใน working dir
./gatekeeper-rs /etc/gatekeeper.toml  # ระบุ path เอง
```

**Dashboard layout:**

```
┌─ HEADER (uptime, clock, mode banners) ─────────────────────────┐
├──────────────────────────────────────┬─────────────────────────┤
│  Services Table                      │  Host Health (CPU/RAM)  │
│  NAME  STATUS  LATENCY  LAST CHECK   │                         │
├──────────────────────────────────────┴─────────────────────────┤
│  Latency Sparklines (rolling 20 samples)                       │
├────────────────────────────────────────────────────────────────┤
│  Infrastructure Health Gauge                                   │
├────────────────────────────────────────────────────────────────┤
│  Events Log (latest 6 lines)                                   │
└────────────────────────────────────────────────────────────────┘
```

**ความหมายของสี:**

| สัญลักษณ์ | ความหมาย |
|---|---|
| 🟢 `● UP` | service ตอบสนองปกติ |
| 🔴 `● DOWN` | ตรวจไม่พบ (HTTP error / TCP refused / timeout) |
| ⚪️ `○ ??` | ยังไม่เคยตรวจ (poll cycle แรก) |
| 🟢 CPU/RAM gauge | ปกติ (CPU < 50%, RAM < 60%) |
| 🟡 CPU/RAM gauge | เริ่มสูง |
| 🔴 CPU/RAM gauge | critical |

### 🔧 Maintenance Mode (กด `M`)

ใช้เมื่อต้องการทำงาน maintenance เอง ไม่ให้ Gatekeeper เข้าไปแทรก

| พฤติกรรม | ปกติ | Maintenance ON |
|---|---|---|
| Polling | ✅ | ✅ |
| สั่ง `restart_command` | ✅ | ❌ ระงับ |
| ส่ง Discord alert | ✅ | ❌ ระงับ |
| บันทึก log | ✅ | ✅ (มี tag `[MAINT]`) |

**สัญญาณภาพ:**
- Header border เปลี่ยนเป็น **เหลือง**
- มีแถบกระพริบ `🔧 MAINTENANCE MODE ACTIVE`
- log มีบรรทัด `Maintenance mode ENABLED`

> **อย่าลืมกด `M` อีกครั้งเพื่อปิด** เมื่อทำงานเสร็จ มิฉะนั้น service ที่ล่มจริงจะไม่ถูกซ่อม

### 🧪 Dry-Run Mode

```bash
./gatekeeper-rs --dry-run
```

ทำงานเหมือนปกติทุกอย่าง **ยกเว้น**ไม่สั่ง `restart_command` จริง — เหมาะกับ:
- Verify config ใหม่ก่อน deploy production
- ทดสอบว่า detection + Discord notification ทำงาน

> **เปรียบเทียบกับ Maintenance:** Dry-Run ส่ง alert แต่ไม่ heal | Maintenance ไม่ส่ง alert ไม่ heal

---

## ⌨️ Keyboard Shortcuts

### Monitor Mode

| ปุ่ม | Action |
|---|---|
| `Q` / `q` | ออกจากโปรแกรม (graceful: ส่ง Discord offline alert ก่อน) |
| `Ctrl+C` | Force quit |
| `R` / `r` | Reload `config.toml` แบบ hot |
| `M` / `m` | Toggle Maintenance Mode |

### Setup Wizard

| ปุ่ม | Action |
|---|---|
| `↑` `↓` หรือ `j` `k` | เลื่อน cursor |
| `Enter` | เปิดเมนู / ยืนยัน / ไปฟิลด์ถัดไป |
| `Tab` / `Shift+Tab` | สลับ focus ระหว่างฟิลด์ |
| `Esc` / `q` | กลับเมนูก่อนหน้า |
| `S` | สแกน Service ในเครื่อง (Local Discovery) |
| `D` | ลบ service ที่เลือก (ในหน้า Manage Services) |
| `← / → / Space` | สลับ Method ระหว่าง HTTP ↔ TCP |

### CLI Flags

| Flag | Description |
|---|---|
| `--config` | เปิด Setup Wizard |
| `--dry-run` | รันแบบไม่ heal จริง |
| `--help` | แสดงข้อความช่วยเหลือ |
| `[CONFIG_FILE]` | path ของ config (default: `config.toml`) |

---

## 📜 Logging

### File location

```
./logs/gatekeeper.YYYY-MM-DD.log
```

- ใช้ **เวลา local** ของระบบ ไม่ใช่ UTC
- Rotate อัตโนมัติทุกเที่ยงคืน local time
- Plain text — ไม่มี ANSI codes ใช้ `grep` / `tail` / `awk` ได้สะดวก

### Format

```
2026-04-29T01:23:45.123456Z  INFO gatekeeper_rs: <message> <key=value> ...
```

### Key events to look for

| Event | Meaning |
|---|---|
| `Service DOWN — triggering auto-heal service=X cmd=...` | ตรวจพบ X ล่ม กำลังจะ restart |
| `Service RECOVERED service=X` | service กลับมา UP |
| `Discord DOWN alert sent service=X` | Discord ส่งสำเร็จ |
| `[DRY-RUN] heal skipped service=X` | อยู่ใน dry-run — ไม่ได้ restart จริง |
| `[MAINT] DOWN observed — alert + heal suppressed` | อยู่ใน maintenance mode |
| `Maintenance mode ENABLED / DISABLED` | กด `M` toggle |
| `Configuration reloaded successfully` | กด `R` reload |

### Common queries

```bash
# ดู transition (DOWN/RECOVERED) เท่านั้น
grep -E "DOWN|RECOVERED" logs/gatekeeper.*.log

# นับจำนวนครั้งที่แต่ละ service ล่มในวันนี้
grep "Service DOWN" logs/gatekeeper.$(date +%F).log \
  | awk -F 'service=' '{print $2}' | awk '{print $1}' \
  | sort | uniq -c

# ดู log สด
tail -f logs/gatekeeper.$(date +%F).log
```

---

## 🛡️ Best Practices

### 1. ตั้ง sudo NOPASSWD แบบเฉพาะเจาะจง

สร้างไฟล์ `/etc/sudoers.d/gatekeeper` (ใช้ `visudo`):

```
gatekeeper ALL=(ALL) NOPASSWD: /bin/systemctl restart nginx, /bin/systemctl restart redis
```

> ⚠️ **อย่า**ใช้ `NOPASSWD: ALL` เด็ดขาด — ระบุ command ที่อนุญาตเป็นรายชื่อจะปลอดภัยกว่ามาก

ทดสอบ:

```bash
sudo -u gatekeeper sudo -n systemctl restart nginx  # ต้องไม่ขอ password
```

### 2. รันเป็น systemd service

`/etc/systemd/system/gatekeeper.service`:

```ini
[Unit]
Description=Gatekeeper-RS Infrastructure Monitor
After=network.target

[Service]
Type=simple
User=gatekeeper
WorkingDirectory=/opt/gatekeeper
ExecStart=/opt/gatekeeper/gatekeeper-rs /opt/gatekeeper/config.toml
Restart=on-failure
RestartSec=10s

[Install]
WantedBy=multi-user.target
```

> เมื่อรันเป็น systemd จะไม่มี TUI ให้ดู — ใช้ `journalctl -fu gatekeeper` หรือ `tail -f logs/gatekeeper.*.log` แทน

### 3. กัน Restart Loop / Flapping

**ปัจจุบัน Gatekeeper-RS ยังไม่มี cooldown timer ในตัว** — แต่มี safeguard ระดับ state machine:

> heal command ยิงเฉพาะตอนที่ **transition UP→DOWN** เท่านั้น
> ไม่ใช่ทุก poll cycle ที่เห็น DOWN

**คำแนะนำ:**
- ตั้ง `interval_secs ≥ 30` สำหรับ production
- ใช้ HTTP healthcheck endpoint ที่ stable (เช่น `/healthz`) — ไม่ใช่หน้าแรกที่ load หนัก
- Monitor log ถ้าเห็น `DOWN`/`RECOVERED` สลับกันถี่ๆ → ปัญหาที่ตัว service เอง ไม่ใช่ heal มากขึ้นจะช่วย
- ใช้ Maintenance Mode ระหว่าง deployment

### 4. Security

- `discord_webhook_url` ถือเป็น **secret** → `chmod 600 config.toml`
- `restart_command` execute ตามที่เขียน — ไม่มี sandbox → ระวัง shell injection patterns
- `logs/` อาจมี URL internal service → จำกัดสิทธิ์อ่าน

---

## 🗺️ Roadmap

ฟีเจอร์ที่อยู่ระหว่างพิจารณาสำหรับ v2.x:

- [ ] **Heal cooldown timer** (เช่น "heal 1 ครั้งต่อ 5 นาที / service")
- [ ] **Slack / Webhook generic** (รองรับเกินกว่า Discord)
- [ ] **Multi-host monitoring** ผ่าน SSH หรือ agent-light
- [ ] **HTTP body assertion** (เช่น `expect_substring = "OK"` ใน healthcheck)
- [ ] **Prometheus metrics endpoint** สำหรับ scrape
- [ ] **Web dashboard** (read-only mirror ของ TUI)

---

## 🧰 Project Structure

```
gatekeeper-rs/
├── src/
│   ├── main.rs              # Entry point + run_app event loop
│   ├── config.rs            # TOML config loading + testconfig overlay
│   ├── checker.rs           # HTTP / TCP probe logic
│   ├── healer.rs            # Restart command executor (no shell injection)
│   ├── notifier.rs          # Discord webhook sender
│   ├── tui.rs               # Monitor Mode dashboard rendering
│   ├── config_wizard.rs     # Setup Wizard state machine
│   ├── config_wizard_ui.rs  # Setup Wizard rendering
│   └── log_appender.rs      # Local-time daily log rotator
├── config.toml              # Production config (managed by Wizard)
├── testconfig.toml          # Optional staging overlay
└── logs/
    └── gatekeeper.*.log     # Daily-rotated logs
```

---

## 📄 License & Credits

- พัฒนาโดย: ทีม DevOps / Infrastructure
- เขียนด้วย: [Rust](https://www.rust-lang.org/) + [Tokio](https://tokio.rs/) + [Ratatui](https://ratatui.rs/) + [Crossterm](https://github.com/crossterm-rs/crossterm)
- Inspiration: Prometheus + Alertmanager + supervisord — แต่บีบเป็นไบนารีเดียว

---

<div align="center">

**คำถาม / รายงานบั๊ก** → ติดต่อทีมผู้ดูแลโปรเจกต์
**เวอร์ชันคู่มือ:** v2.0 *(เมษายน 2026)*

*Stay green. Stay healed. 🛡️*

</div>
