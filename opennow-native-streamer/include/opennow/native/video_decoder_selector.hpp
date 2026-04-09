#pragma once

#include <string>
#include <vector>

namespace opennow::native {

struct VideoDecoderCandidate {
  std::string decoder_name;
  bool hardware = false;
  bool use_hw_device = false;
  std::string path_description;
};

bool IsRaspberryPi4Runtime();
std::vector<VideoDecoderCandidate> BuildVideoDecoderCandidates(const std::string& codec, bool force_software_decode, bool prefer_rgba_upload);

}  // namespace opennow::native
