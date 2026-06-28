# MPhotoPicker

App desktop **lọc & chép ảnh theo danh sách tên file**, chạy trên **macOS và Windows**. Bạn dán danh sách tên ảnh (đã chọn từ đâu đó hoặc trang chọn ảnh của mình là photo.maclife.vn), chọn thư mục nguồn + đích, app tìm các ảnh khớp tên rồi **chép** sang thư mục mới — giữ nguyên file gốc.

Xây bằng **Tauri 2** (Rust backend + UI web), không phụ thuộc trình duyệt như bản web cũ (vốn chỉ chạy được trên Chrome/Edge).

## Tính năng

- **So khớp theo tên gốc, bỏ qua đuôi**: `DSC_0001` khớp `DSC_0001.JPG`, `DSC_0001.NEF`… (không phân biệt hoa/thường).
- **Chọn loại đuôi**: Tất cả / Chỉ JPG / Chỉ RAW / RAW + JPG / Tùy chỉnh → chép cả RAW lẫn JPG hoặc chỉ một loại.
- **Quét cả thư mục con** (bật/tắt).
- **Chép (copy)** — luôn giữ file gốc; xử lý trùng tên ở đích: Đổi tên / Bỏ qua / Ghi đè.
- **Quét khớp** (xem trước, không chép) và **Chép ảnh lọc** (chép thật) với thanh tiến độ.
- **Báo cáo file không tìm thấy**, xuất ra file `.txt`.
- **Kéo-thả thư mục** vào ô Nguồn/Đích, **nhớ thư mục gần nhất**, nút **Mở thư mục đích**.

## Yêu cầu

- [Node.js](https://nodejs.org) 18+ và [Rust](https://rustup.rs) (stable).
- macOS: Xcode Command Line Tools. Windows: WebView2 (có sẵn trên Win 10/11) + Microsoft C++ Build Tools.

## Chạy & build

```bash
npm install          # cài phụ thuộc JS

npm run tauri dev    # chạy app ở chế độ phát triển
npm run tauri build  # đóng gói: .dmg/.app (macOS), .msi/.exe (Windows)
```

Chạy test logic backend:

```bash
cd src-tauri && cargo test
```

## Cấu trúc

```
src/                 # frontend (index.html, styles.css, main.ts)
src-tauri/src/
  filter.rs          # lõi quét & chép + unit tests
  lib.rs             # đăng ký plugin + command
  main.rs
```
