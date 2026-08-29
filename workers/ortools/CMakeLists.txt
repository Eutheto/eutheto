# SPDX-License-Identifier: Apache-2.0
cmake_minimum_required(VERSION 3.25)
project(eutheto_ortools_worker LANGUAGES NONE)

set(
  EUTHETO_ORTOOLS_PHASE3_CONTRACT
  ""
  CACHE FILEPATH
  "Approved Phase-03 OR-Tools/protobuf source contract"
)

if(EUTHETO_ORTOOLS_PHASE3_CONTRACT STREQUAL "")
  message(FATAL_ERROR
    "The OR-Tools worker is not configured in Phase 00. "
    "Supply -DEUTHETO_ORTOOLS_PHASE3_CONTRACT=/absolute/path/to/an/approved-contract.json "
    "only after the Phase-03 source, SHA-256, matched protobuf, protocol, flags, and license gates are approved. "
    "workers/ortools/source-contract.example.json is intentionally unapproved and cannot build a worker."
  )
endif()

if(NOT IS_ABSOLUTE "${EUTHETO_ORTOOLS_PHASE3_CONTRACT}")
  message(FATAL_ERROR
    "EUTHETO_ORTOOLS_PHASE3_CONTRACT must be an absolute path to reviewed Phase-03 build input."
  )
endif()

if(NOT EXISTS "${EUTHETO_ORTOOLS_PHASE3_CONTRACT}")
  message(FATAL_ERROR
    "EUTHETO_ORTOOLS_PHASE3_CONTRACT does not exist: ${EUTHETO_ORTOOLS_PHASE3_CONTRACT}"
  )
endif()

file(READ "${EUTHETO_ORTOOLS_PHASE3_CONTRACT}" phase3_contract_json)

function(contract_get output_name)
  string(JSON field_value ERROR_VARIABLE field_error GET "${phase3_contract_json}" ${ARGN})
  if(NOT field_error STREQUAL "NOTFOUND")
    string(JOIN "." field_path ${ARGN})
    message(FATAL_ERROR
      "Invalid Phase-03 source contract field '${field_path}': ${field_error}"
    )
  endif()
  set(${output_name} "${field_value}" PARENT_SCOPE)
endfunction()

function(require_sha256 field_name field_value)
  string(LENGTH "${field_value}" digest_length)
  if(NOT digest_length EQUAL 64 OR NOT "${field_value}" MATCHES "^[0-9a-f]+$")
    message(FATAL_ERROR
      "Approved Phase-03 field '${field_name}' must be exactly 64 lowercase hexadecimal SHA-256 characters."
    )
  endif()
endfunction()

contract_get(contract_schema_version schema_version)
contract_get(approval_phase approval phase)
contract_get(approval_record approval record)
contract_get(approval_status approval status)
contract_get(ortools_version ortools version)
contract_get(ortools_source_url ortools source_url)
contract_get(ortools_sha256 ortools sha256)
contract_get(protobuf_source_version protobuf source_version)
contract_get(protobuf_source_url protobuf source_url)
contract_get(protobuf_sha256 protobuf sha256)
contract_get(protobuf_protoc_version protobuf protoc_version)
contract_get(protobuf_runtime_version protobuf cpp_runtime_version)
contract_get(protocol_wire_version protocol wire_version)
contract_get(protocol_schema_sha256 protocol schema_sha256)
contract_get(worker_identity worker identity)
contract_get(worker_version worker version)
string(JSON cmake_cache_type ERROR_VARIABLE cmake_cache_error TYPE "${phase3_contract_json}" cmake cache_entries)

if(NOT contract_schema_version STREQUAL "1")
  message(FATAL_ERROR "Unsupported source-contract schema_version: ${contract_schema_version}")
endif()
if(NOT approval_phase STREQUAL "3" OR NOT approval_status STREQUAL "approved")
  message(FATAL_ERROR
    "The source contract is not approved for Phase 03 (phase=${approval_phase}, status=${approval_status})."
  )
endif()
if(approval_record STREQUAL "" OR approval_record STREQUAL "UNRESOLVED")
  message(FATAL_ERROR "An approved Phase-03 contract requires a non-placeholder approval.record.")
endif()
if(NOT ortools_source_url MATCHES "^https://" OR ortools_version STREQUAL "UNRESOLVED")
  message(FATAL_ERROR "Approved OR-Tools version and HTTPS source URL are required.")
endif()
if(NOT protobuf_source_url MATCHES "^https://"
   OR protobuf_source_version STREQUAL "UNRESOLVED"
   OR protobuf_protoc_version STREQUAL "UNRESOLVED"
   OR protobuf_runtime_version STREQUAL "UNRESOLVED")
  message(FATAL_ERROR "Approved matched protobuf source, protoc, and C++ runtime versions are required.")
endif()
if(NOT protocol_wire_version STREQUAL "1")
  message(FATAL_ERROR "This Phase-00 contract accepts only solver-worker wire version 1.")
endif()
if(NOT worker_identity STREQUAL "eutheto-ortools-worker" OR worker_version STREQUAL "UNRESOLVED")
  message(FATAL_ERROR "Approved worker identity and version are required.")
endif()
if(NOT cmake_cache_error STREQUAL "NOTFOUND" OR NOT cmake_cache_type STREQUAL "OBJECT")
  message(FATAL_ERROR
    "Approved Phase-03 contract field 'cmake.cache_entries' must contain the exact reviewed CMake cache object."
  )
endif()

require_sha256("ortools.sha256" "${ortools_sha256}")
require_sha256("protobuf.sha256" "${protobuf_sha256}")
require_sha256("protocol.schema_sha256" "${protocol_schema_sha256}")

message(FATAL_ERROR
  "The approved Phase-03 input contract passed the Phase-00 gate, but no worker target exists yet. "
  "Phase 03 must add the reviewed real source build and executable checks; Phase 00 never emits a dummy worker."
)
