---
name: opennow-vn-translation
description: >-
  Quy trình chuẩn ECC (plan -> test -> implement -> review -> verify) để phát triển,
  kiểm toán dữ liệu wuwa-vi-chunks, tối ưu OCR/Overlay và di chuyển module
  opennow-qt/vn-translation giữa các bản cập nhật OpenNOW.
---

# OpenNOW VN-Translation Engineering Skill (ECC Workflow)

Sử dụng skill này khi xây dựng, sửa lỗi, tối ưu hóa hoặc nâng cấp hệ thống dịch OCR và cơ sở dữ liệu `wuwa-vi-chunks`.

## 1. Quy trình ECC bắt buộc
1. **Plan**: Xác định rõ file trong `opennow-qt/vn-translation/` cần sửa, kiểm tra ảnh hưởng tới RAM/CPU/GPU trên cấu hình i5-4300U / HD 4400 / 8GB RAM.
2. **Test-First**: Cập nhật unit test trong `opennow-qt/vn-translation/tests/tst_translation.cpp` và `translation_logic_check.py`.
3. **Implement**: Viết code C++20 / QML sạch, không rò rỉ bộ nhớ, không chặn GUI thread.
4. **Review**: Kiểm tra tất cả các cổng bảo vệ (không leak API key, không FBO copy trên `StreamVideoItem`, không gọi mạng trong unit test).
5. **Verify**:
   - Chạy `python opennow-qt/vn-translation/tests/translation_logic_check.py`
   - Chạy `node scripts/check-translations.mjs`
   - Chạy CTest `opennow-translation-tests` (offscreen)

## 2. Xử lý & Tra cứu dữ liệu `wuwa-vi-chunks` (220.764 chuỗi)
Khi nạp hoặc kiểm toán các file `chunks/vi_chunk_NNN.json`:
1. **Phát hiện & sửa lỗi trượt dòng (Off-by-one shift)**: Kiểm tra tương quan độ dài ký tự, dấu câu kết thúc (`.`, `?`, `!`, `...`), và các placeholder (`{q_count}`) giữa key tiếng Anh và value tiếng Việt.
2. **Chuẩn hóa RichText & Biến động**:
   - Strip toàn bộ `<color=...>`, `</color>`, `<size=...>`, `</size>` khỏi key khi đánh chỉ mục.
   - Chuyển các chuỗi có `{q_count}`, `{0}`, `{PlayerName}` thành mẫu so khớp động (Template Pattern) để bắt chính xác các câu có số đếm trong game (ví dụ `Defeat TD: 3/7`).
3. **Gộp dòng đoạn văn (Paragraph Line-Merger)**:
   - Trước khi tra DB, gom các dòng OCR liên tiếp có khoảng cách dọc `gapY < 1.35 * lineHeight` và cùng căn lề thành một đoạn văn hoàn chỉnh để khớp 100% các câu thoại dài bị ngắt thành 2–3 dòng trên màn hình.
4. **Bộ lọc nhiễu HUD (HUD Noise Filter)**:
   - Bỏ qua các chuỗi chỉ chứa số, FPS, ping (`ms`), `UID:`, `LV.`, phân số máu thuần túy hoặc phím tắt đơn lẻ (`E`, `F`, `Q`, `R`).
