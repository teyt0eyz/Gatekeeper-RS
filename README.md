# 🛡️ Gatekeeper-RS
**Automated Site Reliability & Infrastructure Monitoring Tool**

Gatekeeper-RS คือเครื่องมือสำหรับเฝ้าระวังและกู้คืนระบบอัตโนมัติที่พัฒนาด้วยภาษา **Rust** ออกแบบมาเพื่อดูแลความเสถียรของ Infrastructure (เช่น API Services, Databases, Web Servers) โดยเน้นความรวดเร็ว ความปลอดภัย และการใช้งานที่ง่ายผ่านหน้าจอ Terminal

> **Project Concept:** โปรเจกต์นี้สร้างขึ้นเพื่อแก้ปัญหาการล่มของเซอร์วิสในระบบ Cloud และ Virtualization (Proxmox) โดยระบบจะทำหน้าที่เป็น "ผู้เฝ้าประตู" ที่คอยตรวจสอบสถานะและตัดสินใจสั่งซ่อมแซมระบบเบื้องต้นได้ทันทีโดยไม่ต้องรอเจ้าหน้าที่ดำเนินการ

---

## ✨ คุณสมบัติหลัก
* **Interactive Configuration Wizard:** ระบบตั้งค่าผ่านหน้าจอ TUI ไม่ต้องแก้ไขไฟล์ .toml ด้วยมือ ลดความผิดพลาด
* **Real-time Monitoring Dashboard:** หน้าจอแสดงผลสถานะระบบ พร้อมกราฟ Sparkline แสดง Latency และสถิติการใช้ Resource (CPU/RAM)
* **Automated Self-Healing:** ระบบกู้คืนอัตโนมัติ สั่ง Restart ได้ทันทีเมื่อตรวจพบความผิดปกติ (No Shell Injection)
* **Smart Alerting:** แจ้งเตือนผ่าน Discord Webhook พร้อมระบบ Debounce ป้องกันการส่งข้อความซ้ำซ้อน
* **State-Transition Logging:** บันทึกเฉพาะช่วงที่มีการเปลี่ยนแปลงสถานะเพื่อประหยัดพื้นที่จัดเก็บข้อมูล
* **Safety First:** รองรับโหมด Dry Run และระบบ Graceful Shutdown เพื่อการทำงานที่ปลอดภัย

---

## 🚀 วิธีการติดตั้งและรัน

### สิ่งที่ต้องมีในเครื่อง
1. **Rust Toolchain** (Cargo)
2. **สิทธิ์ระดับสูง** (เช่น sudo สำหรับ systemctl) หากต้องการสั่ง Restart Service ระบบ

### การรันโปรแกรม
# 1. ดาวน์โหลดและเข้าสู่โฟลเดอร์โปรเจกต์
git clone https://github.com/teyt0eyz/Gatekeeper-RS.git
cd Gatekeeper-RS

# 2. เริ่มการตั้งค่าครั้งแรกผ่าน Wizard
cargo run -- --config

# 3. รันโปรแกรมในโหมดปกติเพื่อเริ่มการตรวจสอบ
cargo run

# 4. (ตัวเลือก) รันในโหมดทดสอบคำสั่ง (Dry Run)
cargo run -- --dry-run


### การควบคุมภายในหน้าจอ TUI
# R	     โหลดไฟล์ตั้งค่าใหม่ทันที (Hot Reload)
# Esc / Q	ปิดโปรแกรมอย่างปลอดภัย
# Tab	     เปลี่ยนช่องกรอกข้อมูล (ในหน้า Config Wizard)
# Enter	ยืนยันการกรอกข้อมูลหรือบันทึกค่า


### โครงสร้างไฟล์ที่สำคัญ
# gatekeeper.log: ไฟล์บันทึกเหตุการณ์และการเปลี่ยนแปลงสถานะ
# config.toml: ไฟล์ตั้งค่าหลัก (จัดการผ่าน Config Wizard)
# testconfig.toml: ไฟล์ตั้งค่าสำหรับทดสอบ Local
