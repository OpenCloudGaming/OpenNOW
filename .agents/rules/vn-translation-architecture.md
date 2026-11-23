# OpenNOW-ORC (VN Translation) Architecture & Performance Rules

Mọi tác vụ sửa đổi hoặc nâng cấp tính năng dịch màn hình (Translation OCR) trong OpenNOW phải tuân thủ tuyệt đối các bất biến sau:

## 1. Cô lập hoàn toàn trong 1 thư mục duy nhất (`opennow-qt/vn-translation/`)
- Toàn bộ mã nguồn C++ (`src/`), giao diện QML (`qml/`), phông chữ (`resources/fonts/`), dữ liệu (`resources/data/`), unit test (`tests/`), và file cấu hình build (`vn-translation.cmake`) PHẢI nằm gọn bên trong `opennow-qt/vn-translation/`.
- Không sửa đổi `native/opennow-core/`, `native/opennow-streamer/`, hoặc ABI FFI.
- Chỉ kết nối với OpenNOW gốc qua tối đa 4 điểm neo (shim hooks) tối giản để khi OpenNOW gốc ra bản vá mới, chỉ cần copy thư mục `opennow-qt/vn-translation/` sang:
  1. `opennow-qt/CMakeLists.txt`: 1 dòng `include(vn-translation/vn-translation.cmake)`.
  2. `opennow-qt/src/app/ApplicationStartup.cpp`: Khởi tạo `TranslationEngine` và nối signal khung hình.
  3. `opennow-qt/qml/desktop/stream/DesktopStreamScreen.qml`: Đặt `TranslationOverlay` ở `z: 6`.
  4. `opennow-qt/qml/desktop/settings/DesktopSettingsScreen.qml`: Đặt mục `Translation` (`page: 14`) nằm ngay dưới mục `Recording` (`page: 12`) trên thanh điều hướng trái, giữ nguyên toàn bộ chỉ số `page: 0..13` của dự án gốc.

## 2. Không đụng chạm luồng Stream & Tối ưu tuyệt đối cho cấu hình thấp (HP 840 G1 - i5-4300U / HD 4400 / 8GB RAM)
- **CẤM dùng `ShaderEffectSource { sourceItem: streamVideo; live: true }` hoặc `MultiEffect` Gaussian Blur trực tiếp lên `StreamVideoItem`**: Việc bắt Scene Graph copy `StreamVideoItem` ra offscreen FBO và chạy blur mỗi frame 60Hz sẽ làm tụt FPS trên iGPU Intel HD 4400.
- **CẤM băm SHA-256 toàn khung hình 1080p trên GUI Thread**: Phải dùng thuật toán băm lấy mẫu cách quãng (Fast Strided Luma FNV-1a 64-bit, `< 0.05ms`) để phát hiện thay đổi khung hình.
- **Tiết kiệm RAM (< 40MB cho 220.764 câu)**: Không nhân bản hàng trăm ngàn `QString` qua nhiều `QHash` rời rạc; ưu tiên chuẩn hóa key, strip thẻ `<color=...>` / `<size=...>`, và biên dịch cache nhị phân phẳng trên đĩa sau khi tải chunk.
- **CẤM tự động gọi mạng trong Constructor của `TranslationEngine` khi chạy Unit Test**: Unit test (`opennow-translation-tests`) khởi tạo `TranslationEngine` trong môi trường offscreen có giới hạn 30s; mọi tác vụ tải chunk phải có cổng bảo vệ hoặc kích hoạt ngoài constructor khi test.

## 3. Tiêu chuẩn Hiển thị: Khảm Tối Tàng Hình (Invisible Feathered Dark Inpaint)
- **CẤM vẽ hộp chữ nhật vuông vức màu trắng đục hoặc đen đặc**: Phải dùng lớp khảm tối mờ biên (Feathered Dark Gradient / Capsule Mask với độ mờ giảm dần ra mép `opacity -> 0`) ôm sát theo từng dòng/đoạn chữ, lấy màu nền tối cục bộ từ ảnh crop.
- **Typography đồng bộ với Wuthering Waves**: Dùng font `Be Vietnam Pro` kèm viền chữ sắc nét (`style: Text.Outline`) và tự động lấy mẫu màu chữ gốc (Trắng ngà / Vàng kim / Xanh) cùng chiều cao pixel thực tế của dòng chữ tiếng Anh gốc.
