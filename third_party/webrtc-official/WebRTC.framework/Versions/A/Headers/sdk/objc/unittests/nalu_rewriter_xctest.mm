/*
 *  Copyright 2018 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#include <span>

#include "common_video/h264/h264_common.h"
#include "components/video_codec/nalu_rewriter.h"

#import <XCTest/XCTest.h>

#if TARGET_OS_IPHONE
#import <AVFoundation/AVFoundation.h>
#import <UIKit/UIKit.h>
#endif

@interface NaluRewriterTests : XCTestCase

@end

static const uint8_t NALU_TEST_DATA_0[] = {0xAA, 0xBB, 0xCC};
static const uint8_t NALU_TEST_DATA_1[] = {0xDE, 0xAD, 0xBE, 0xEF};

// clang-format off
static const uint8_t SPS_PPS_BUFFER[] = {
  // SPS nalu.
  0x00, 0x00, 0x00, 0x01, 0x27, 0x42, 0x00, 0x1E, 0xAB, 0x40, 0xF0, 0x28,
  0xD3, 0x70, 0x20, 0x20, 0x20, 0x20,
  // PPS nalu.
  0x00, 0x00, 0x00, 0x01, 0x28, 0xCE, 0x3C, 0x30};
// clang-format on

@implementation NaluRewriterTests

- (void)testCreateVideoFormatDescription {
  CMVideoFormatDescriptionRef description =
      webrtc::CreateVideoFormatDescription(SPS_PPS_BUFFER);
  XCTAssertTrue(description);
  if (description) {
    CFRelease(description);
    description = nullptr;
  }

  // clang-format off
  const uint8_t sps_pps_not_at_start_buffer[] = {
    // Add some non-SPS/PPS NALUs at the beginning
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0xFF, 0x00, 0x00, 0x00, 0x01,
    0xAB, 0x33, 0x21,
    // SPS nalu.
    0x00, 0x00, 0x01, 0x27, 0x42, 0x00, 0x1E, 0xAB, 0x40, 0xF0, 0x28, 0xD3,
    0x70, 0x20, 0x20, 0x20, 0x20,
    // PPS nalu.
    0x00, 0x00, 0x01, 0x28, 0xCE, 0x3C, 0x30};
  // clang-format on
  description =
      webrtc::CreateVideoFormatDescription(sps_pps_not_at_start_buffer);

  XCTAssertTrue(description);

  if (description) {
    CFRelease(description);
    description = nullptr;
  }

  const uint8_t other_buffer[] = {0x00, 0x00, 0x00, 0x01, 0x28};
  XCTAssertFalse(webrtc::CreateVideoFormatDescription(other_buffer));
}

- (void)testReadEmptyInput {
  webrtc::AnnexBBufferReader reader({});
  std::span<const uint8_t> nalu;
  XCTAssertEqual(0u, reader.BytesRemaining());
  XCTAssertFalse(reader.ReadNalu(nalu));
}

- (void)testReadSingleNalu {
  const uint8_t annex_b_test_data[] = {0x00, 0x00, 0x00, 0x01, 0xAA};
  webrtc::AnnexBBufferReader reader(annex_b_test_data);
  std::span<const uint8_t> nalu;
  XCTAssertEqual(std::size(annex_b_test_data), reader.BytesRemaining());
  XCTAssertTrue(reader.ReadNalu(nalu));
  XCTAssertEqual(annex_b_test_data + 4, nalu.data());
  XCTAssertEqual(1u, nalu.size());
  XCTAssertEqual(0u, reader.BytesRemaining());
  XCTAssertFalse(reader.ReadNalu(nalu));
}

- (void)testReadSingleNalu3ByteHeader {
  const uint8_t annex_b_test_data[] = {0x00, 0x00, 0x01, 0xAA};
  webrtc::AnnexBBufferReader reader(annex_b_test_data);
  std::span<const uint8_t> nalu;
  XCTAssertEqual(std::size(annex_b_test_data), reader.BytesRemaining());
  XCTAssertTrue(reader.ReadNalu(nalu));
  XCTAssertEqual(annex_b_test_data + 3, nalu.data());
  XCTAssertEqual(1u, nalu.size());
  XCTAssertEqual(0u, reader.BytesRemaining());
  XCTAssertFalse(reader.ReadNalu(nalu));
}

- (void)testReadMissingNalu {
  // clang-format off
  const uint8_t annex_b_test_data[] = {0x01,
                                       0x00, 0x01,
                                       0x00, 0x00, 0x00, 0xFF};
  // clang-format on
  webrtc::AnnexBBufferReader reader(annex_b_test_data);
  std::span<const uint8_t> nalu;
  XCTAssertEqual(0u, reader.BytesRemaining());
  XCTAssertFalse(reader.ReadNalu(nalu));
}

- (void)testReadMultipleNalus {
  // clang-format off
  const uint8_t annex_b_test_data[] = {0x00, 0x00, 0x00, 0x01, 0xFF,
                                       0x01,
                                       0x00, 0x01,
                                       0x00, 0x00, 0x00, 0xFF,
                                       0x00, 0x00, 0x01, 0xAA, 0xBB};
  // clang-format on
  webrtc::AnnexBBufferReader reader(annex_b_test_data);
  std::span<const uint8_t> nalu;
  XCTAssertEqual(std::size(annex_b_test_data), reader.BytesRemaining());
  XCTAssertTrue(reader.ReadNalu(nalu));
  XCTAssertEqual(annex_b_test_data + 4, nalu.data());
  XCTAssertEqual(8u, nalu.size());
  XCTAssertEqual(5u, reader.BytesRemaining());
  XCTAssertTrue(reader.ReadNalu(nalu));
  XCTAssertEqual(annex_b_test_data + 15, nalu.data());
  XCTAssertEqual(2u, nalu.size());
  XCTAssertEqual(0u, reader.BytesRemaining());
  XCTAssertFalse(reader.ReadNalu(nalu));
}

- (void)testEmptyOutputBuffer {
  webrtc::AvccBufferWriter writer({});
  XCTAssertEqual(0u, writer.BytesRemaining());
  XCTAssertFalse(writer.WriteNalu(NALU_TEST_DATA_0));
}

- (void)testWriteSingleNalu {
  const uint8_t expected_buffer[] = {0x00, 0x00, 0x00, 0x03, 0xAA, 0xBB, 0xCC};
  constexpr size_t buffer_size = std::size(NALU_TEST_DATA_0) + 4;
  uint8_t buffer[buffer_size];
  webrtc::AvccBufferWriter writer(buffer);
  XCTAssertEqual(buffer_size, writer.BytesRemaining());
  XCTAssertTrue(writer.WriteNalu(NALU_TEST_DATA_0));
  XCTAssertEqual(0u, writer.BytesRemaining());
  XCTAssertFalse(writer.WriteNalu(NALU_TEST_DATA_1));
  XCTAssertEqual(0,
                 memcmp(expected_buffer, buffer, std::size(expected_buffer)));
}

- (void)testWriteMultipleNalus {
  // clang-format off
  const uint8_t expected_buffer[] = {
    0x00, 0x00, 0x00, 0x03, 0xAA, 0xBB, 0xCC,
    0x00, 0x00, 0x00, 0x04, 0xDE, 0xAD, 0xBE, 0xEF
  };
  // clang-format on
  constexpr size_t buffer_size =
      std::size(NALU_TEST_DATA_0) + std::size(NALU_TEST_DATA_1) + 8;
  uint8_t buffer[buffer_size];
  webrtc::AvccBufferWriter writer(buffer);
  XCTAssertEqual(buffer_size, writer.BytesRemaining());
  XCTAssertTrue(writer.WriteNalu(NALU_TEST_DATA_0));
  XCTAssertEqual(buffer_size - (std::size(NALU_TEST_DATA_0) + 4),
                 writer.BytesRemaining());
  XCTAssertTrue(writer.WriteNalu(NALU_TEST_DATA_1));
  XCTAssertEqual(0u, writer.BytesRemaining());
  XCTAssertEqual(0,
                 memcmp(expected_buffer, buffer, std::size(expected_buffer)));
}

- (void)testOverflow {
  const uint8_t expected_buffer[] = {0x00, 0x00, 0x00};
  constexpr size_t buffer_size = std::size(NALU_TEST_DATA_0);
  uint8_t buffer[buffer_size] = {};
  webrtc::AvccBufferWriter writer(buffer);
  XCTAssertEqual(buffer_size, writer.BytesRemaining());
  XCTAssertFalse(writer.WriteNalu(NALU_TEST_DATA_0));
  XCTAssertEqual(buffer_size, writer.BytesRemaining());
  XCTAssertEqual(0,
                 memcmp(expected_buffer, buffer, std::size(expected_buffer)));
}

- (void)testH264AnnexBBufferToCMSampleBuffer {
  // clang-format off
  const uint8_t annex_b_test_data[] = {
    0x00,
    0x00, 0x00, 0x01,
    0x01, 0x00, 0x00, 0xFF, // first chunk, 4 bytes
    0x00, 0x00, 0x01,
    0xAA, 0xFF, // second chunk, 2 bytes
    0x00, 0x00, 0x01,
    0xBB};  // third chunk, 1 byte, will not fit into output array

  const uint8_t expected_cmsample_data[] = {
    0x00, 0x00, 0x00, 0x04,
    0x01, 0x00, 0x00, 0xFF, // first chunk, 4 bytes
    0x00, 0x00, 0x00, 0x02,
    0xAA, 0xFF}; // second chunk, 2 bytes
  // clang-format on

  CMMemoryPoolRef memory_pool = CMMemoryPoolCreate(nil);
  CMSampleBufferRef out_sample_buffer = nil;
  CMVideoFormatDescriptionRef description = [self createDescription];

  Boolean result = webrtc::H264AnnexBBufferToCMSampleBuffer(
      annex_b_test_data, description, &out_sample_buffer, memory_pool);

  XCTAssertTrue(result);

  XCTAssertEqual(description,
                 CMSampleBufferGetFormatDescription(out_sample_buffer));

  char* data_ptr = nullptr;
  CMBlockBufferRef block_buffer =
      CMSampleBufferGetDataBuffer(out_sample_buffer);
  size_t block_buffer_size = CMBlockBufferGetDataLength(block_buffer);
  CMBlockBufferGetDataPointer(block_buffer, 0, nullptr, nullptr, &data_ptr);
  XCTAssertEqual(block_buffer_size, std::size(annex_b_test_data));

  int data_comparison_result = memcmp(
      expected_cmsample_data, data_ptr, std::size(expected_cmsample_data));

  XCTAssertEqual(0, data_comparison_result);

  if (description) {
    CFRelease(description);
    description = nullptr;
  }

  CMMemoryPoolInvalidate(memory_pool);
  CFRelease(memory_pool);
}

- (void)testH264CMSampleBufferToAnnexBBuffer {
  // clang-format off
  const uint8_t cmsample_data[] = {
    0x00, 0x00, 0x00, 0x04,
    0x01, 0x00, 0x00, 0xFF, // first chunk, 4 bytes
    0x00, 0x00, 0x00, 0x02,
    0xAA, 0xFF}; // second chunk, 2 bytes

  const uint8_t expected_annex_b_data[] = {
    0x00, 0x00, 0x00, 0x01,
    0x01, 0x00, 0x00, 0xFF, // first chunk, 4 bytes
    0x00, 0x00, 0x00, 0x01,
    0xAA, 0xFF}; // second chunk, 2 bytes
  // clang-format on

  webrtc::Buffer annexb_buffer =
      webrtc::Buffer::CreateWithCapacity(std::size(cmsample_data));
  CMSampleBufferRef sample_buffer =
      [self createCMSampleBufferRef:(void*)cmsample_data
                       cmsampleSize:std::size(cmsample_data)];

  Boolean result =
      webrtc::H264CMSampleBufferToAnnexBBuffer(sample_buffer,
                                               /* is_keyframe */ false,
                                               &annexb_buffer);

  XCTAssertTrue(result);

  XCTAssertEqual(std::size(expected_annex_b_data), annexb_buffer.size());
  XCTAssertEqual(
      0,
      memcmp(
          expected_annex_b_data, annexb_buffer.data(), annexb_buffer.size()));
}

- (void)testH264CMSampleBufferToAnnexBBufferWithKeyframe {
  // clang-format off
  const uint8_t cmsample_data[] = {
    0x00, 0x00, 0x00, 0x04,
    0x01, 0x00, 0x00, 0xFF, // first chunk, 4 bytes
    0x00, 0x00, 0x00, 0x02,
    0xAA, 0xFF}; // second chunk, 2 bytes

  const uint8_t expected_annex_b_data[] = {
    0x00, 0x00, 0x00, 0x01,
    0x01, 0x00, 0x00, 0xFF, // first chunk, 4 bytes
    0x00, 0x00, 0x00, 0x01,
    0xAA, 0xFF}; // second chunk, 2 bytes
  // clang-format on

  webrtc::Buffer annexb_buffer =
      webrtc::Buffer::CreateUninitializedWithSize(std::size(cmsample_data));
  CMSampleBufferRef sample_buffer =
      [self createCMSampleBufferRef:(void*)cmsample_data
                       cmsampleSize:std::size(cmsample_data)];

  Boolean result =
      webrtc::H264CMSampleBufferToAnnexBBuffer(sample_buffer,
                                               /* is_keyframe */ true,
                                               &annexb_buffer);

  XCTAssertTrue(result);

  XCTAssertEqual(std::size(SPS_PPS_BUFFER) + std::size(expected_annex_b_data),
                 annexb_buffer.size());

  XCTAssertEqual(
      0,
      memcmp(SPS_PPS_BUFFER, annexb_buffer.data(), std::size(SPS_PPS_BUFFER)));

  XCTAssertEqual(0,
                 memcmp(expected_annex_b_data,
                        annexb_buffer.data() + std::size(SPS_PPS_BUFFER),
                        std::size(expected_annex_b_data)));
}

- (CMVideoFormatDescriptionRef)createDescription {
  CMVideoFormatDescriptionRef description =
      webrtc::CreateVideoFormatDescription(SPS_PPS_BUFFER);
  XCTAssertTrue(description);
  return description;
}

- (CMSampleBufferRef)createCMSampleBufferRef:(void*)cmsampleData
                                cmsampleSize:(size_t)cmsampleSize {
  CMSampleBufferRef sample_buffer = nil;
  OSStatus status;

  CMVideoFormatDescriptionRef description = [self createDescription];
  CMBlockBufferRef block_buffer = nullptr;

  status = CMBlockBufferCreateWithMemoryBlock(nullptr,
                                              cmsampleData,
                                              cmsampleSize,
                                              nullptr,
                                              nullptr,
                                              0,
                                              cmsampleSize,
                                              kCMBlockBufferAssureMemoryNowFlag,
                                              &block_buffer);
  XCTAssertEqual(kCMBlockBufferNoErr, status);

  status = CMSampleBufferCreate(nullptr,
                                block_buffer,
                                true,
                                nullptr,
                                nullptr,
                                description,
                                1,
                                0,
                                nullptr,
                                0,
                                nullptr,
                                &sample_buffer);
  XCTAssertEqual(noErr, status);

  return sample_buffer;
}

@end
