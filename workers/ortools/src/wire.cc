// SPDX-License-Identifier: Apache-2.0
#include "wire.h"

#include <array>
#include <istream>
#include <limits>
#include <ostream>
#include <string_view>
#include <unordered_map>
#include <unordered_set>

#include <google/protobuf/descriptor.h>

#include "protocol-policy.h"

namespace eutheto::ortools_worker {
namespace {

namespace policy = eutheto::worker::v1::policy;
using google::protobuf::Descriptor;
using google::protobuf::FieldDescriptor;

struct Cursor {
  std::span<const std::uint8_t> bytes;
  std::size_t position = 0;
};

bool ReadVarint(Cursor* cursor, std::uint64_t* value) {
  std::uint64_t result = 0;
  const std::size_t start = cursor->position;
  for (unsigned index = 0; index < 10; ++index) {
    if (cursor->position == cursor->bytes.size()) return false;
    const std::uint8_t byte = cursor->bytes[cursor->position++];
    if (index == 9 && byte > 1U) return false;
    result |= static_cast<std::uint64_t>(byte & 0x7fU) << (index * 7U);
    if ((byte & 0x80U) == 0) {
      std::size_t minimum = 1;
      for (std::uint64_t remaining = result; remaining >= 0x80U;
           remaining >>= 7U) {
        ++minimum;
      }
      if (cursor->position - start != minimum) return false;
      *value = result;
      return true;
    }
  }
  return false;
}

bool IsValidUtf8(std::span<const std::uint8_t> bytes) {
  for (std::size_t i = 0; i < bytes.size();) {
    const std::uint8_t first = bytes[i++];
    if (first <= 0x7fU) continue;
    unsigned continuation = 0;
    std::uint32_t codepoint = 0;
    std::uint32_t minimum = 0;
    if ((first & 0xe0U) == 0xc0U) {
      continuation = 1;
      codepoint = first & 0x1fU;
      minimum = 0x80U;
    } else if ((first & 0xf0U) == 0xe0U) {
      continuation = 2;
      codepoint = first & 0x0fU;
      minimum = 0x800U;
    } else if ((first & 0xf8U) == 0xf0U) {
      continuation = 3;
      codepoint = first & 0x07U;
      minimum = 0x10000U;
    } else {
      return false;
    }
    if (i + continuation > bytes.size()) return false;
    for (unsigned j = 0; j < continuation; ++j) {
      const std::uint8_t next = bytes[i++];
      if ((next & 0xc0U) != 0x80U) return false;
      codepoint = (codepoint << 6U) | (next & 0x3fU);
    }
    if (codepoint < minimum || codepoint > 0x10ffffU ||
        (codepoint >= 0xd800U && codepoint <= 0xdfffU)) {
      return false;
    }
  }
  return true;
}

std::size_t BytesCap(std::string_view full_name) {
  if (full_name == "eutheto.worker.v1.HandshakeRequest.core_version")
    return policy::kHandshakeRequestCoreVersionMaxBytes;
  if (full_name == "eutheto.worker.v1.HandshakeRequest.expected_backend_id")
    return policy::kHandshakeRequestExpectedBackendIdMaxBytes;
  if (full_name == "eutheto.worker.v1.HandshakeRequest.expected_manifest_sha256")
    return policy::kHandshakeRequestExpectedManifestSha256MaxBytes;
  if (full_name == "eutheto.worker.v1.SolveRequest.request_id")
    return policy::kSolveRequestRequestIdMaxBytes;
  if (full_name == "eutheto.worker.v1.SolveRequest.cp_model_proto")
    return policy::kSolveRequestCpModelProtoMaxBytes;
  if (full_name == "eutheto.worker.v1.SolveRequest.model_fingerprint")
    return policy::kSolveRequestModelFingerprintMaxBytes;
  return policy::kMaxStringBytes;
}

std::size_t RepeatedCap(std::string_view full_name) {
  if (full_name == "eutheto.worker.v1.HandshakeRequest.required_capabilities")
    return policy::kHandshakeRequestRequiredCapabilitiesMaxCount;
  if (full_name == "eutheto.worker.v1.SolveRequest.projections")
    return policy::kSolveRequestProjectionsMaxCount;
  return policy::kMaxRepeatedFieldItems;
}

int ExpectedWireType(const FieldDescriptor& field) {
  using Type = FieldDescriptor::Type;
  switch (field.type()) {
    case Type::TYPE_DOUBLE:
    case Type::TYPE_FIXED64:
    case Type::TYPE_SFIXED64:
      return 1;
    case Type::TYPE_STRING:
    case Type::TYPE_BYTES:
    case Type::TYPE_MESSAGE:
      return 2;
    case Type::TYPE_FLOAT:
    case Type::TYPE_FIXED32:
    case Type::TYPE_SFIXED32:
      return 5;
    default:
      return 0;
  }
}

bool ValidateVarintValue(const FieldDescriptor& field, std::uint64_t value) {
  using Type = FieldDescriptor::Type;
  switch (field.type()) {
    case Type::TYPE_BOOL:
      return value <= 1;
    case Type::TYPE_UINT32:
      return value <= std::numeric_limits<std::uint32_t>::max();
    case Type::TYPE_INT32:
      return value <=
                 static_cast<std::uint64_t>(
                     std::numeric_limits<std::int32_t>::max()) ||
             value >= 0xffffffff80000000ULL;
    case Type::TYPE_ENUM:
      return value <=
                 static_cast<std::uint64_t>(std::numeric_limits<int>::max()) &&
             field.enum_type()->FindValueByNumber(static_cast<int>(value)) !=
                 nullptr;
    default:
      return true;
  }
}

bool SkipUnknown(Cursor* cursor, int wire_type) {
  std::uint64_t value = 0;
  switch (wire_type) {
    case 0:
      return ReadVarint(cursor, &value);
    case 1:
      if (cursor->bytes.size() - cursor->position < 8) return false;
      cursor->position += 8;
      return true;
    case 2:
      if (!ReadVarint(cursor, &value) ||
          value > cursor->bytes.size() - cursor->position) {
        return false;
      }
      cursor->position += static_cast<std::size_t>(value);
      return true;
    case 5:
      if (cursor->bytes.size() - cursor->position < 4) return false;
      cursor->position += 4;
      return true;
    default:
      return false;
  }
}

bool PreflightMessage(std::span<const std::uint8_t> bytes,
                      const Descriptor& descriptor, std::size_t depth,
                      std::string* reason) {
  if (depth > policy::kMaxNestingDepth) {
    *reason = "nesting depth exceeds policy";
    return false;
  }
  Cursor cursor{bytes};
  std::unordered_set<int> singular_fields;
  std::unordered_set<int> packed_fields;
  std::unordered_set<int> oneofs;
  std::unordered_map<int, std::size_t> repeated_counts;
  std::size_t unknown_fields = 0;
  int last_field_number = 0;
  while (cursor.position != cursor.bytes.size()) {
    std::uint64_t tag = 0;
    if (!ReadVarint(&cursor, &tag) || tag == 0 || (tag >> 3U) > 536870911U) {
      *reason = "invalid or noncanonical field tag";
      return false;
    }
    const int number = static_cast<int>(tag >> 3U);
    const int wire_type = static_cast<int>(tag & 7U);
    if (number >= 19000 && number <= 19999) {
      *reason = "globally reserved protobuf field tag is present";
      return false;
    }
    if (number < last_field_number) {
      *reason = "field order is noncanonical";
      return false;
    }
    last_field_number = number;
    const FieldDescriptor* field = descriptor.FindFieldByNumber(number);
    if (field == nullptr) {
      if (descriptor.IsReservedNumber(number)) {
        *reason = "reserved field tag is present";
        return false;
      }
      if (++unknown_fields > policy::kMaxRepeatedFieldItems) {
        *reason = "unknown field count exceeds policy";
        return false;
      }
      if (!SkipUnknown(&cursor, wire_type)) {
        *reason = "invalid unknown field";
        return false;
      }
      continue;
    }

    const bool packed = field->is_repeated() && field->is_packable() &&
                        wire_type == 2;
    if ((!packed && wire_type != ExpectedWireType(*field)) ||
        (field->is_packed() && field->is_repeated() && field->is_packable() &&
         !packed)) {
      *reason = "known field has a noncanonical wire type";
      return false;
    }
    if (!field->is_repeated()) {
      if (!singular_fields.insert(number).second) {
        *reason = "duplicate singular field";
        return false;
      }
      if (field->containing_oneof() != nullptr &&
          !oneofs.insert(field->containing_oneof()->index()).second) {
        *reason = "duplicate oneof member";
        return false;
      }
    }

    if (packed) {
      if (!packed_fields.insert(number).second) {
        *reason = "packed field is split across multiple segments";
        return false;
      }
      std::uint64_t length = 0;
      if (!ReadVarint(&cursor, &length) || length == 0 ||
          length > cursor.bytes.size() - cursor.position) {
        *reason = "invalid packed field";
        return false;
      }
      Cursor packed_cursor{cursor.bytes.subspan(
          cursor.position, static_cast<std::size_t>(length))};
      cursor.position += static_cast<std::size_t>(length);
      while (packed_cursor.position != packed_cursor.bytes.size()) {
        std::uint64_t value = 0;
        const int element_wire = ExpectedWireType(*field);
        bool valid = false;
        if (element_wire == 0) {
          valid = ReadVarint(&packed_cursor, &value) &&
                  ValidateVarintValue(*field, value);
        } else if (element_wire == 1 &&
                   packed_cursor.bytes.size() - packed_cursor.position >= 8) {
          packed_cursor.position += 8;
          valid = true;
        } else if (element_wire == 5 &&
                   packed_cursor.bytes.size() - packed_cursor.position >= 4) {
          packed_cursor.position += 4;
          valid = true;
        }
        if (!valid) {
          *reason = "invalid packed scalar";
          return false;
        }
        if (++repeated_counts[number] > RepeatedCap(field->full_name())) {
          *reason = "repeated field count exceeds policy";
          return false;
        }
      }
      continue;
    }

    if (field->is_repeated() &&
        ++repeated_counts[number] > RepeatedCap(field->full_name())) {
      *reason = "repeated field count exceeds policy";
      return false;
    }
    if (wire_type == 0) {
      std::uint64_t value = 0;
      if (!ReadVarint(&cursor, &value) || !ValidateVarintValue(*field, value) ||
          (!field->has_presence() && !field->is_repeated() && value == 0)) {
        *reason = "invalid or noncanonical scalar";
        return false;
      }
    } else if (wire_type == 1) {
      if (cursor.bytes.size() - cursor.position < 8) {
        *reason = "truncated fixed64 field";
        return false;
      }
      cursor.position += 8;
    } else if (wire_type == 5) {
      if (cursor.bytes.size() - cursor.position < 4) {
        *reason = "truncated fixed32 field";
        return false;
      }
      cursor.position += 4;
    } else {
      std::uint64_t length = 0;
      if (!ReadVarint(&cursor, &length) ||
          length > cursor.bytes.size() - cursor.position) {
        *reason = "truncated length-delimited field";
        return false;
      }
      const auto value = cursor.bytes.subspan(
          cursor.position, static_cast<std::size_t>(length));
      cursor.position += static_cast<std::size_t>(length);
      if (value.empty() && !field->has_presence() && !field->is_repeated()) {
        *reason = "explicit empty scalar is noncanonical";
        return false;
      }
      if (field->type() == FieldDescriptor::TYPE_MESSAGE) {
        if (!PreflightMessage(value, *field->message_type(), depth + 1, reason))
          return false;
      } else {
        if (length > BytesCap(field->full_name())) {
          *reason = "string or bytes field exceeds policy";
          return false;
        }
        if (field->type() == FieldDescriptor::TYPE_STRING &&
            !IsValidUtf8(value)) {
          *reason = "string field is not valid UTF-8";
          return false;
        }
      }
    }
  }
  return true;
}

}  // namespace

FrameReadResult FrameReader::Read(std::size_t maximum_payload) {
  std::array<std::uint8_t, policy::kLengthPrefixBytes> prefix{};
  input_.read(reinterpret_cast<char*>(prefix.data()), prefix.size());
  const std::streamsize prefix_bytes = input_.gcount();
  if (prefix_bytes == 0 && input_.eof()) return {FrameReadStatus::kEof, {}};
  if (prefix_bytes != static_cast<std::streamsize>(prefix.size()))
    return {input_.bad() ? FrameReadStatus::kIoError
                         : FrameReadStatus::kTruncated,
            {}};
  const std::uint32_t length =
      (static_cast<std::uint32_t>(prefix[0]) << 24U) |
      (static_cast<std::uint32_t>(prefix[1]) << 16U) |
      (static_cast<std::uint32_t>(prefix[2]) << 8U) |
      static_cast<std::uint32_t>(prefix[3]);
  if (length < policy::kMinPayloadBytes) return {FrameReadStatus::kEmpty, {}};
  if (length > maximum_payload) return {FrameReadStatus::kTooLarge, {}};
  if (session_bytes_ + prefix.size() + length > policy::kTotalSessionBytes)
    return {FrameReadStatus::kSessionTooLarge, {}};
  std::vector<std::uint8_t> payload(length);
  input_.read(reinterpret_cast<char*>(payload.data()),
              static_cast<std::streamsize>(payload.size()));
  if (input_.gcount() != static_cast<std::streamsize>(payload.size()))
    return {input_.bad() ? FrameReadStatus::kIoError
                         : FrameReadStatus::kTruncated,
            {}};
  session_bytes_ += prefix.size() + payload.size();
  return {FrameReadStatus::kOk, std::move(payload)};
}

FrameReadStatus FrameReader::CheckEof() {
  const int next = input_.get();
  if (next != std::char_traits<char>::eof()) return FrameReadStatus::kOk;
  return input_.bad() ? FrameReadStatus::kIoError : FrameReadStatus::kEof;
}

bool WriteFrame(std::ostream& output,
                const eutheto::worker::v1::WorkerFrame& frame) {
  const std::size_t payload_size = frame.ByteSizeLong();
  if (payload_size < policy::kMinPayloadBytes ||
      payload_size > policy::kWorkerEventMaxPayloadBytes ||
      payload_size > std::numeric_limits<std::uint32_t>::max()) {
    return false;
  }
  std::string payload;
  payload.reserve(payload_size);
  if (!frame.SerializeToString(&payload) || payload.size() != payload_size)
    return false;
  const std::uint32_t length = static_cast<std::uint32_t>(payload.size());
  const std::array<char, 4> prefix = {
      static_cast<char>(length >> 24U), static_cast<char>(length >> 16U),
      static_cast<char>(length >> 8U), static_cast<char>(length)};
  output.write(prefix.data(), static_cast<std::streamsize>(prefix.size()));
  output.write(payload.data(), static_cast<std::streamsize>(payload.size()));
  output.flush();
  return output.good();
}

bool PreflightParentFrame(std::span<const std::uint8_t> payload,
                          std::string* static_reason) {
  if (payload.empty()) {
    *static_reason = "empty protobuf payload";
    return false;
  }
  return PreflightMessage(payload,
                          *eutheto::worker::v1::ParentFrame::descriptor(), 1,
                          static_reason);
}

}  // namespace eutheto::ortools_worker
