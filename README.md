<div align="center">

# Gatekeeper-RS

**Infrastructure Health Monitor & Self-Healing Engine — built in Rust**

[![Made with Rust](https://img.shields.io/badge/Made%20with-Rust-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![TUI: Ratatui](https://img.shields.io/badge/TUI-Ratatui-7B2CBF)](https://ratatui.rs/)
[![Async: Tokio](https://img.shields.io/badge/Async-Tokio-1F6FEB)](https://tokio.rs/)
[![Platform: Linux](https://img.shields.io/badge/Platform-Linux%20%7C%20macOS-success)]()
[![Status: v3.0](https://img.shields.io/badge/version-v3.0-brightgreen)]()

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

## ✨ What's New in v3.0

> v3.0 เพิ่ม per-service recovery control และ refactor wizard ให้ปลอดภัยขึ้น

| Feature | Description |
|---|---|
| ⏱️ **Per-Service Recovery Delay** | แต่ละ service ตั้ง `recovery_delay` (วินาที) ได้อิสระ — หลัง detect DOWN ระบบรอตาม delay ที่กำหนดก่อนสั่ง restart command อัตโนมัติ (default: 30 วินาที) |
| 📟 **Heal Countdown in TUI** | คอลัมน์ LATENCY สำหรับ DOWN service แสดง `heal Xs` นับถอยหลังแบบ real-time ตาม `recovery_delay` ของแต่ละ service — ไม่ใช่ global poll timer อีกต่อไป |
| 🔍 **Scan-First "Add Service"** | เมนู "Add Service" เปิด Service Scanner ทันที — ไม่มีฟอร์มกรอกเอง ลดข้อผิดพลาดและป้องกัน service ที่ชื่อผิดหลุดเข้า config |
| 🚫 **No More Blank Service Forms** | Wizard เริ่มต้นด้วย services list ว่างเปล่า — ทุก service ต้องมาจาก Scanner เท่านั้น ไม่มีการกรอก Name จากศูนย์ |

## ✨ What's New in v2.0

> v2.0 เปลี่ยนจาก "monitor ตัวเล็กๆ" เป็น **SRE automation suite** เต็มตัว

| Feature | Description |
|---|---|
| 🎛️ **Menu-Driven Setup Wizard** | TUI Wizard แบบหลายหน้าจอ (Main Menu → Global Settings / Service List / Service Edit) ใช้งานง่าย ใส่ข้อมูลครบทุกฟิลด์โดยไม่ต้องเขียน TOML เอง |
| 🔍 **Local Service Discovery** | รัน `systemctl list-units` แล้วเลือก service จริงในเครื่อง — auto-fill `Name` + `restart_command` |
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
   │   │ delayed healer │    │ notifier (HTTP)  │                 │
   │   │ (sleep N secs) │    └──────────────────┘                 │
   │   └────────────────┘              │                          │
   │           │                       ▼                          │
   └───────────┼──────────────── Discord webhook ─────────────────┘
               ▼
       systemctl restart …
```

**Design highlights:**
- **Concurrent checks** ผ่าน `tokio::spawn` — service จำนวนมากตรวจขนานกันโดยไม่ block
- **State-transition–based heal** — ยิง `restart_command` เฉพาะตอน UP→DOWN เท่านั้น ไม่ยิงทุก poll
- **Per-service recovery delay** — spawned task นอนรอ `recovery_delay` วินาทีก่อน exec ไม่ block checker loop
- **Lock-free maintenance flag** ผ่าน `Arc<AtomicBool>` — toggle ได้รวดเร็วโดยไม่ต้องรอ task อื่น
- **Hot-reloadable config** ผ่าน `Arc<RwLock<Config>>` — checker task อ่านค่าใหม่ทุก tick

---

## 🚀 Quick Start

```bash
# 1. Build
git clone https://github.com/teyt0eyz/Gatekeeper-RS.git gatekeeper-rs && cd gatekeeper-rs
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
│    Add Service         │  ──▶  เปิด Scanner เลือก service จากระบบ
│    Save & Exit         │  ──▶  เขียนลง config.toml
└────────────────────────┘
```

#### Service Edit Fields

เมื่อเลือก service จาก Scanner หรือแก้ไขจาก Manage Services จะมีฟิลด์ให้กรอก:

| Field | Description | Default |
|---|---|---|
| **Name** | ชื่อ service (auto-fill จาก Scanner) | — |
| **URL / Address** | HTTP: `http://host:port/path` \| TCP: `host:port` | — |
| **Method** | `HTTP` หรือ `TCP` (Toggle ด้วย `←/→`) | `HTTP` |
| **Restart Command** | คำสั่ง restart (auto-fill จาก Scanner) | — |
| **Recovery Delay (seconds)** | วินาทีที่รอก่อนสั่ง restart หลัง detect DOWN | `30` |

#### ✨ Service Discovery ("Add Service")

เลือก **Add Service** ใน Main Menu หรือกด `S` ใน Manage Services → ระบบจะรัน:

```bash
systemctl list-units --type=service --state=running --no-pager --plain
```

แล้วแสดงรายการ service ที่กำลังรันให้เลือก เมื่อกด `Enter` ระบบจะ auto-fill:
- **Name:** ชื่อ unit (ตัด `.service` ออก)
- **Restart Command:** `sudo systemctl restart <unit>.service`

จากนั้น Wizard จะพาไปยัง Service Edit ให้กรอก **URL/Address**, **Method**, และ **Recovery Delay** ก่อน save

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
recovery_delay = 30

[[services]]
name = "redis"
url = "127.0.0.1:6379"
method = "TCP"
restart_command = "sudo systemctl restart redis.service"
recovery_delay = 60
```

#### Config Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `interval_secs` | `u64` | — | ความถี่ในการ poll (วินาที) |
| `discord_webhook_url` | `String` | *(optional)* | URL สำหรับส่ง alert |
| `services[].name` | `String` | — | ชื่อ service |
| `services[].url` | `String` | — | HTTP URL หรือ `host:port` |
| `services[].method` | `"HTTP"\|"TCP"` | — | วิธีตรวจสอบ |
| `services[].restart_command` | `String` | — | คำสั่ง restart |
| `services[].recovery_delay` | `u64` | `30` | วินาทีรอก่อน restart หลัง DOWN |

> **Tip:** `recovery_delay` เป็น optional field — config เก่าที่ไม่มีฟิลด์นี้จะใช้ค่า default `30` โดยอัตโนมัติ ไม่ต้องแก้ไขไฟล์

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

**LATENCY column — DOWN services:**

| ข้อความ | ความหมาย |
|---|---|
| `heal 28s` | Heal command จะยิงใน 28 วินาที (นับถอยหลังตาม `recovery_delay`) |
| `next 12s` | Heal ยิงแล้ว — รอ poll รอบหน้าเพื่อยืนยันว่า service กลับมาหรือไม่ |
| `checking` | Poll cycle กำลังรันอยู่ |

### ⏱️ Recovery Delay

เมื่อตรวจพบ DOWN:

```
t=0s   ── detect DOWN ── HealScheduled ──▶ TUI แสดง "heal 30s"
t=30s  ── spawn task ตื่น ── exec restart_command
t=60s  ── poll cycle ถัดไป ── ตรวจสอบว่ากลับมาแล้วหรือยัง
```

- แต่ละ service มี `recovery_delay` เป็นของตัวเอง ปรับได้ตาม SLA
- Heal task ทำงาน **แยกจาก checker loop** (tokio::spawn) — ไม่ block การตรวจ service อื่น
- ถ้า service กลับมาก่อน delay หมด (เช่น restart เอง) — poll รอบหน้าจะ detect RECOVERED และ cancel countdown ใน TUI อัตโนมัติ

### 🔧 Maintenance Mode (กด `M`)

ใช้เมื่อต้องการทำงาน maintenance เอง ไม่ให้ Gatekeeper เข้าไปแทรก

| พฤติกรรม | ปกติ | Maintenance ON |
|---|---|---|
| Polling | ✅ | ✅ |
| รอ recovery_delay + สั่ง restart | ✅ | ❌ ระงับ |
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
| `S` | สแกน Service ในเครื่อง (ใช้ได้จากหน้า Manage Services) |
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
| `Service DOWN — triggering auto-heal service=X cmd=...` | ตรวจพบ X ล่ม — กำลังตั้ง heal timer |
| `Service RECOVERED service=X` | service กลับมา UP หลัง heal |
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

### 3. ปรับ Recovery Delay ให้เหมาะกับแต่ละ Service

`recovery_delay` ไม่ใช่แค่ cooldown — มันคือ "เวลาที่ให้ service ฟื้นตัวเอง" ก่อนที่ Gatekeeper จะเข้าแทรกแซง

**แนวทาง:**

| ประเภท Service | recovery_delay แนะนำ | เหตุผล |
|---|---|---|
| Web server (nginx, caddy) | `15–30s` | restart เร็ว ผล impact ทันที |
| Database (postgres, mysql) | `60–120s` | ต้องการเวลา recovery + crash-safe shutdown |
| Message broker (redis, rabbitmq) | `30–60s` | มี in-memory state ที่ต้องระวัง |
| Background worker / cron | `60s+` | อาจกำลัง graceful shutdown อยู่ |

```toml
# ตัวอย่าง: ให้ database เวลา 2 นาทีก่อน restart
[[services]]
name = "postgres"
url = "127.0.0.1:5432"
method = "TCP"
restart_command = "sudo systemctl restart postgresql"
recovery_delay = 120
```

> **Safeguard ที่ยังมีอยู่:** heal command ยิงเฉพาะตอน **transition UP→DOWN** เท่านั้น — ถ้า service ยัง DOWN ในรอบถัดไปโดยไม่มี UP คั่น จะไม่ยิงซ้ำ ป้องกัน restart loop

### 4. Security

- `discord_webhook_url` ถือเป็น **secret** → `chmod 600 config.toml`
- `restart_command` execute ตามที่เขียน — ไม่มี shell (split on whitespace เท่านั้น) → ไม่มี command injection แต่ต้องดูแลว่าคำสั่งถูกต้อง
- `logs/` อาจมี URL internal service → จำกัดสิทธิ์อ่าน

---

## 🧰 Project Structure

```
gatekeeper-rs/
├── src/
│   ├── main.rs              # Entry point + run_app event loop + UiUpdate channel
│   ├── config.rs            # TOML config loading + testconfig overlay
│   ├── checker.rs           # HTTP / TCP probe logic
│   ├── healer.rs            # Restart command executor (no shell injection)
│   ├── notifier.rs          # Discord webhook sender
│   ├── tui.rs               # Monitor Mode dashboard rendering + heal countdown
│   ├── config_wizard.rs     # Setup Wizard state machine (scan-first add flow)
│   ├── config_wizard_ui.rs  # Setup Wizard rendering
│   └── log_appender.rs      # Local-time daily log rotator
├── config.toml              # Production config (managed by Wizard)
├── testconfig.toml          # Optional staging overlay
└── logs/
    └── gatekeeper.*.log     # Daily-rotated logs
```

---

- เขียนด้วย: [Rust](https://www.rust-lang.org/) + [Tokio](https://tokio.rs/) + [Ratatui](https://ratatui.rs/)
