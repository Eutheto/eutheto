# SPDX-License-Identifier: Apache-2.0
cmake_minimum_required(VERSION 3.25)

foreach(required_variable REPOSITORY_ROOT NATIVE_ROOT)
  if(NOT DEFINED ${required_variable} OR "${${required_variable}}" STREQUAL "")
    message(FATAL_ERROR "${required_variable} is required.")
  endif()
endforeach()
if(NOT WIN32)
  message(FATAL_ERROR "The native OR-Tools worker build requires Windows x86_64.")
endif()
string(TOLOWER "$ENV{PROCESSOR_ARCHITECTURE}" processor_architecture)
if(NOT processor_architecture STREQUAL "amd64")
  message(FATAL_ERROR
    "The native OR-Tools worker build requires Windows x86_64; PROCESSOR_ARCHITECTURE is '$ENV{PROCESSOR_ARCHITECTURE}'.")
endif()

cmake_path(ABSOLUTE_PATH REPOSITORY_ROOT NORMALIZE)
cmake_path(ABSOLUTE_PATH NATIVE_ROOT NORMALIZE)
set(expected_native_root
  "${REPOSITORY_ROOT}/.cache/ortools-native/windows-x86_64")
cmake_path(NORMAL_PATH expected_native_root)
if(NOT NATIVE_ROOT STREQUAL expected_native_root)
  message(FATAL_ERROR
    "NATIVE_ROOT must be the dedicated repository cache path ${expected_native_root}.")
endif()
# A failed rerun must never leave a previous artifact visible as current.
file(REMOVE_RECURSE
  "${NATIVE_ROOT}/work"
  "${NATIVE_ROOT}/staging"
  "${NATIVE_ROOT}/current")


set(source_contract_path
  "${REPOSITORY_ROOT}/workers/ortools/source-contract.json")
set(dependency_sources_path
  "${REPOSITORY_ROOT}/workers/ortools/dependency-sources.json")
set(protocol_schema_path "${REPOSITORY_ROOT}/protocol/solver-worker.proto")
if(NOT EXISTS "${source_contract_path}")
  message(FATAL_ERROR "The generated approved OR-Tools source contract is missing.")
endif()
if(NOT EXISTS "${dependency_sources_path}")
  message(FATAL_ERROR "The generated OR-Tools dependency source lock is missing.")
endif()

file(READ "${source_contract_path}" source_contract_json)
function(contract_get output_name)
  string(JSON field_value ERROR_VARIABLE field_error
    GET "${source_contract_json}" ${ARGN})
  if(NOT field_error STREQUAL "NOTFOUND")
    string(JOIN "." field_path ${ARGN})
    message(FATAL_ERROR
      "Invalid generated source-contract field '${field_path}': ${field_error}")
  endif()
  set(${output_name} "${field_value}" PARENT_SCOPE)
endfunction()
function(require_sha256 field_name field_value)
  string(LENGTH "${field_value}" digest_length)
  if(NOT digest_length EQUAL 64 OR NOT field_value MATCHES "^[0-9a-f]+$")
    message(FATAL_ERROR
      "${field_name} must be exactly 64 lowercase hexadecimal SHA-256 characters.")
  endif()
endfunction()
function(require_safe_leaf field_name field_value)
  if(field_value STREQUAL "" OR field_value STREQUAL "." OR field_value STREQUAL ".."
     OR field_value MATCHES "[/\\\\]")
    message(FATAL_ERROR "${field_name} must be a non-empty leaf name: '${field_value}'.")
  endif()
endfunction()

contract_get(contract_schema_version schema_version)
contract_get(approval_phase approval phase)
contract_get(approval_record approval record)
contract_get(approval_status approval status)
contract_get(ortools_version ortools version)
contract_get(ortools_source_url ortools source_url)
contract_get(ortools_sha256 ortools sha256)
contract_get(ortools_patch_relative ortools patch_path)
contract_get(ortools_patch_sha256 ortools patch_sha256)
contract_get(protobuf_version protobuf source_version)
contract_get(protobuf_runtime_version protobuf cpp_runtime_version)
contract_get(protobuf_source_url protobuf source_url)
contract_get(protobuf_sha256 protobuf sha256)
contract_get(protocol_schema_sha256 protocol schema_sha256)
contract_get(worker_identity worker identity)
contract_get(worker_version worker version)
string(JSON cache_entries_type ERROR_VARIABLE cache_entries_error
  TYPE "${source_contract_json}" cmake cache_entries)
string(JSON cache_entry_count ERROR_VARIABLE cache_entry_count_error
  LENGTH "${source_contract_json}" cmake cache_entries)
if(NOT contract_schema_version STREQUAL "1"
   OR NOT approval_phase STREQUAL "3"
   OR NOT approval_status STREQUAL "approved"
   OR approval_record STREQUAL ""
   OR approval_record STREQUAL "UNRESOLVED")
  message(FATAL_ERROR "The generated Phase-03 source contract is not approved.")
endif()
if(NOT ortools_version STREQUAL "9.15.6755"
   OR NOT protobuf_version STREQUAL "33.1"
   OR NOT protobuf_runtime_version STREQUAL "33.1.0"
   OR NOT worker_identity STREQUAL "eutheto-ortools-worker"
   OR NOT worker_version STREQUAL "0.1.0")
  message(FATAL_ERROR "The native build supports only the approved OR-Tools 9.15.6755 worker contract.")
endif()
if(NOT ortools_source_url MATCHES "^https://"
   OR NOT protobuf_source_url MATCHES "^https://")
  message(FATAL_ERROR "Approved OR-Tools and protobuf source URLs must use HTTPS.")
endif()
require_sha256("ortools.sha256" "${ortools_sha256}")
require_sha256("ortools.patch_sha256" "${ortools_patch_sha256}")
require_sha256("protobuf.sha256" "${protobuf_sha256}")
require_sha256("protocol.schema_sha256" "${protocol_schema_sha256}")
if(NOT ortools_patch_relative STREQUAL
   "workers/ortools/patches/9.15-candidate-fixes.patch")
  message(FATAL_ERROR "The source contract selects an unexpected repository patch.")
endif()
if(NOT cache_entries_error STREQUAL "NOTFOUND"
   OR NOT cache_entries_type STREQUAL "OBJECT"
   OR NOT cache_entry_count_error STREQUAL "NOTFOUND"
   OR NOT cache_entry_count EQUAL 26)
  message(FATAL_ERROR "The source contract must contain exactly 26 CMake cache entries.")
endif()
set(ortools_patch "${REPOSITORY_ROOT}/${ortools_patch_relative}")
if(NOT EXISTS "${ortools_patch}")
  message(FATAL_ERROR "The approved repository patch is missing: ${ortools_patch}")
endif()
file(SHA256 "${ortools_patch}" actual_ortools_patch_sha256)
if(NOT actual_ortools_patch_sha256 STREQUAL ortools_patch_sha256)
  message(FATAL_ERROR "The repository patch does not match the approved SHA-256.")
endif()
file(SHA256 "${protocol_schema_path}" actual_protocol_schema_sha256)
if(NOT actual_protocol_schema_sha256 STREQUAL protocol_schema_sha256)
  message(FATAL_ERROR "protocol/solver-worker.proto does not match the approved SHA-256.")
endif()

file(READ "${dependency_sources_path}" dependency_sources_json)
function(dependency_get output_name)
  string(JSON field_value ERROR_VARIABLE field_error
    GET "${dependency_sources_json}" ${ARGN})
  if(NOT field_error STREQUAL "NOTFOUND")
    string(JOIN "." field_path ${ARGN})
    message(FATAL_ERROR
      "Invalid generated dependency-source field '${field_path}': ${field_error}")
  endif()
  set(${output_name} "${field_value}" PARENT_SCOPE)
endfunction()
dependency_get(dependency_schema_version schema_version)
dependency_get(dependency_ortools_version ortools version)
dependency_get(dependency_ortools_sha256 ortools sha256)
string(JSON dependency_object_type ERROR_VARIABLE dependency_object_error
  TYPE "${dependency_sources_json}" dependencies)
string(JSON dependency_count ERROR_VARIABLE dependency_count_error
  LENGTH "${dependency_sources_json}" dependencies)
if(NOT dependency_schema_version STREQUAL "1"
   OR NOT dependency_ortools_version STREQUAL ortools_version
   OR NOT dependency_ortools_sha256 STREQUAL ortools_sha256
   OR NOT dependency_object_error STREQUAL "NOTFOUND"
   OR NOT dependency_object_type STREQUAL "OBJECT"
   OR NOT dependency_count_error STREQUAL "NOTFOUND"
   OR NOT dependency_count EQUAL 5)
  message(FATAL_ERROR
    "The generated dependency source lock does not match the approved OR-Tools source.")
endif()

set(expected_dependencies abseil bzip2 eigen re2 zlib)
set(actual_dependencies "")
math(EXPR dependency_last "${dependency_count} - 1")
foreach(dependency_index RANGE 0 ${dependency_last})
  string(JSON dependency_name MEMBER "${dependency_sources_json}"
    dependencies ${dependency_index})
  list(APPEND actual_dependencies "${dependency_name}")
endforeach()
list(SORT actual_dependencies)
if(NOT actual_dependencies STREQUAL expected_dependencies)
  message(FATAL_ERROR
    "The dependency source lock must contain exactly: ${expected_dependencies}.")
endif()
foreach(dependency IN LISTS expected_dependencies)
  foreach(field archive_name archive_root patch sha256 source_url version)
    dependency_get(field_value dependencies "${dependency}" "${field}")
    set("${dependency}_${field}" "${field_value}")
  endforeach()
  require_safe_leaf("dependencies.${dependency}.archive_name"
    "${${dependency}_archive_name}")
  require_safe_leaf("dependencies.${dependency}.archive_root"
    "${${dependency}_archive_root}")
  require_safe_leaf("dependencies.${dependency}.patch"
    "${${dependency}_patch}")
  require_sha256("dependencies.${dependency}.sha256"
    "${${dependency}_sha256}")
  if(NOT "${${dependency}_source_url}" MATCHES "^https://")
    message(FATAL_ERROR "dependencies.${dependency}.source_url must use HTTPS.")
  endif()
  if("${${dependency}_version}" STREQUAL "")
    message(FATAL_ERROR "dependencies.${dependency}.version must not be empty.")
  endif()
endforeach()

find_program(git_executable git REQUIRED)
find_program(ninja_executable ninja REQUIRED)
find_program(c_compiler cl.exe REQUIRED)
find_program(cxx_compiler cl.exe REQUIRED)
find_program(dumpbin_executable dumpbin.exe REQUIRED)

set(work_root "${NATIVE_ROOT}/work")
set(download_root "${work_root}/downloads")
set(source_parent "${work_root}/sources")
set(ortools_source_dir "${source_parent}/or-tools-9.15")
set(protobuf_source_dir "${source_parent}/protobuf-33.1")
set(ortools_build_dir "${work_root}/ortools-build")
set(ortools_install_dir "${work_root}/ortools-install")
set(worker_build_dir "${work_root}/worker-build")
set(staging_root "${NATIVE_ROOT}/staging")
set(final_root "${NATIVE_ROOT}/current")
set(staging_bin "${staging_root}/bin")
set(final_worker "${final_root}/bin/ortools-worker.exe")

file(MAKE_DIRECTORY "${download_root}" "${source_parent}" "${staging_bin}")

function(download_archive label source_url expected_sha256 archive_path)
  file(REMOVE "${archive_path}")
  message(STATUS "Downloading fixed ${label} source archive")
  file(DOWNLOAD
    "${source_url}"
    "${archive_path}"
    EXPECTED_HASH "SHA256=${expected_sha256}"
    STATUS download_status
    LOG download_log
    SHOW_PROGRESS
    TLS_VERIFY ON
    TIMEOUT 900
    INACTIVITY_TIMEOUT 60
  )
  list(GET download_status 0 download_result)
  list(GET download_status 1 download_message)
  if(NOT download_result EQUAL 0)
    file(REMOVE "${archive_path}")
    message(FATAL_ERROR
      "${label} source download or SHA-256 verification failed: ${download_message}\n${download_log}")
  endif()
  file(SHA256 "${archive_path}" actual_sha256)
  if(NOT actual_sha256 STREQUAL expected_sha256)
    file(REMOVE "${archive_path}")
    message(FATAL_ERROR
      "${label} source archive SHA-256 is ${actual_sha256}, expected ${expected_sha256}.")
  endif()
endfunction()

function(run_stage stage working_directory)
  message(STATUS "Running ${stage}")
  execute_process(
    COMMAND ${ARGN}
    WORKING_DIRECTORY "${working_directory}"
    RESULT_VARIABLE stage_result
    COMMAND_ECHO STDOUT
    ENCODING UTF-8
  )
  if(NOT "${stage_result}" STREQUAL "0")
    message(FATAL_ERROR "${stage} failed with exit code ${stage_result}.")
  endif()
endfunction()

function(extract_archive label archive_path expected_root)
  run_stage("extract-${label}" "${source_parent}"
    "${CMAKE_COMMAND}" -E tar xzf "${archive_path}")
  if(NOT EXISTS "${source_parent}/${expected_root}/CMakeLists.txt")
    message(FATAL_ERROR
      "The verified ${label} archive did not extract expected root ${expected_root}.")
  endif()
endfunction()

set(ortools_archive "${download_root}/or-tools-v9.15.tar.gz")
set(protobuf_archive "${download_root}/protobuf-33.1.tar.gz")
download_archive("OR-Tools" "${ortools_source_url}" "${ortools_sha256}"
  "${ortools_archive}")
download_archive("protobuf" "${protobuf_source_url}" "${protobuf_sha256}"
  "${protobuf_archive}")
foreach(dependency IN LISTS expected_dependencies)
  set(dependency_archive
    "${download_root}/${${dependency}_archive_name}")
  download_archive("${dependency}" "${${dependency}_source_url}"
    "${${dependency}_sha256}" "${dependency_archive}")
endforeach()

extract_archive("ortools" "${ortools_archive}" "or-tools-9.15")
extract_archive("protobuf" "${protobuf_archive}" "protobuf-33.1")
foreach(dependency IN LISTS expected_dependencies)
  extract_archive("${dependency}"
    "${download_root}/${${dependency}_archive_name}"
    "${${dependency}_archive_root}")
endforeach()

run_stage("check-repository-patch" "${ortools_source_dir}"
  "${git_executable}" apply --check "${ortools_patch}")
run_stage("apply-repository-patch" "${ortools_source_dir}"
  "${git_executable}" apply "${ortools_patch}")
foreach(dependency IN LISTS expected_dependencies)
  set(dependency_source_dir
    "${source_parent}/${${dependency}_archive_root}")
  set(dependency_patch
    "${ortools_source_dir}/patches/${${dependency}_patch}")
  if(NOT EXISTS "${dependency_patch}")
    message(FATAL_ERROR
      "The verified OR-Tools source does not contain ${dependency_patch}.")
  endif()
  run_stage("check-${dependency}-patch" "${dependency_source_dir}"
    "${git_executable}" apply --check --ignore-whitespace "${dependency_patch}")
  run_stage("apply-${dependency}-patch" "${dependency_source_dir}"
    "${git_executable}" apply --ignore-whitespace "${dependency_patch}")
endforeach()

set(common_cmake_flags "")
math(EXPR cache_entry_last "${cache_entry_count} - 1")
foreach(cache_entry_index RANGE 0 ${cache_entry_last})
  string(JSON cache_entry_name MEMBER "${source_contract_json}"
    cmake cache_entries ${cache_entry_index})
  contract_get(cache_entry_value cmake cache_entries "${cache_entry_name}")
  list(APPEND common_cmake_flags "-D${cache_entry_name}=${cache_entry_value}")
endforeach()

function(cache_value output_name cache_file entry_name)
  file(STRINGS "${cache_file}" matching_lines REGEX "^${entry_name}:[^=]*=")
  list(LENGTH matching_lines matching_line_count)
  if(NOT matching_line_count EQUAL 1)
    message(FATAL_ERROR
      "Expected exactly one ${entry_name} entry in ${cache_file}; found ${matching_line_count}.")
  endif()
  list(GET matching_lines 0 matching_line)
  string(REGEX REPLACE "^[^=]*=" "" actual_value "${matching_line}")
  set(${output_name} "${actual_value}" PARENT_SCOPE)
endfunction()
function(require_cache_value cache_file entry_name expected_value)
  cache_value(actual_value "${cache_file}" "${entry_name}")
  if(NOT actual_value STREQUAL expected_value)
    message(FATAL_ERROR
      "CMake cache entry ${entry_name} is '${actual_value}', expected '${expected_value}'.")
  endif()
endfunction()

set(ENV{OR_TOOLS_PATCH} "6755")
set(ortools_configure_command
  "${CMAKE_COMMAND}"
  -S "${ortools_source_dir}"
  -B "${ortools_build_dir}"
  -G Ninja
  ${common_cmake_flags}
  "-DCMAKE_C_COMPILER=${c_compiler}"
  "-DCMAKE_CXX_COMPILER=${cxx_compiler}"
  "-DCMAKE_CXX_FLAGS=/DEIGEN_MPL2_ONLY"
  "-DCMAKE_INSTALL_PREFIX=${ortools_install_dir}"
  "-DFETCHCONTENT_FULLY_DISCONNECTED=ON"
  "-DFETCHCONTENT_SOURCE_DIR_ZLIB=${source_parent}/${zlib_archive_root}"
  "-DFETCHCONTENT_SOURCE_DIR_BZIP2=${source_parent}/${bzip2_archive_root}"
  "-DFETCHCONTENT_SOURCE_DIR_ABSL=${source_parent}/${abseil_archive_root}"
  "-DFETCHCONTENT_SOURCE_DIR_PROTOBUF=${protobuf_source_dir}"
  "-DFETCHCONTENT_SOURCE_DIR_RE2=${source_parent}/${re2_archive_root}"
  "-DFETCHCONTENT_SOURCE_DIR_EIGEN3=${source_parent}/${eigen_archive_root}"
)
run_stage("ortools-configure" "${work_root}" ${ortools_configure_command})

set(ortools_cache "${ortools_build_dir}/CMakeCache.txt")
foreach(cache_entry_index RANGE 0 ${cache_entry_last})
  string(JSON cache_entry_name MEMBER "${source_contract_json}"
    cmake cache_entries ${cache_entry_index})
  contract_get(cache_entry_value cmake cache_entries "${cache_entry_name}")
  require_cache_value("${ortools_cache}" "${cache_entry_name}" "${cache_entry_value}")
endforeach()
foreach(source_override
    "ZLIB|${source_parent}/${zlib_archive_root}"
    "BZIP2|${source_parent}/${bzip2_archive_root}"
    "ABSL|${source_parent}/${abseil_archive_root}"
    "PROTOBUF|${protobuf_source_dir}"
    "RE2|${source_parent}/${re2_archive_root}"
    "EIGEN3|${source_parent}/${eigen_archive_root}")
  string(REPLACE "|" ";" source_override_parts "${source_override}")
  list(GET source_override_parts 0 source_override_name)
  list(GET source_override_parts 1 source_override_path)
  require_cache_value("${ortools_cache}"
    "FETCHCONTENT_SOURCE_DIR_${source_override_name}" "${source_override_path}")
endforeach()
require_cache_value("${ortools_cache}" FETCHCONTENT_FULLY_DISCONNECTED ON)
require_cache_value("${ortools_cache}" CMAKE_GENERATOR Ninja)
require_cache_value("${ortools_cache}" CMAKE_CXX_FLAGS /DEIGEN_MPL2_ONLY)
cache_value(actual_c_compiler "${ortools_cache}" CMAKE_C_COMPILER)
cache_value(actual_cxx_compiler "${ortools_cache}" CMAKE_CXX_COMPILER)
cmake_path(GET actual_c_compiler FILENAME actual_c_compiler_name)
cmake_path(GET actual_cxx_compiler FILENAME actual_cxx_compiler_name)
string(TOLOWER "${actual_c_compiler_name}" actual_c_compiler_name)
string(TOLOWER "${actual_cxx_compiler_name}" actual_cxx_compiler_name)
if(NOT actual_c_compiler_name STREQUAL "cl.exe"
   OR NOT actual_cxx_compiler_name STREQUAL "cl.exe")
  message(FATAL_ERROR "The native worker source build must use cl.exe for C and C++.")
endif()

set(ortools_version_config "${ortools_build_dir}/ortoolsConfigVersion.cmake")
if(NOT EXISTS "${ortools_version_config}")
  message(FATAL_ERROR "OR-Tools configure did not generate ortoolsConfigVersion.cmake.")
endif()
file(STRINGS "${ortools_version_config}" package_version_lines
  REGEX "^set\\(PACKAGE_VERSION \"[^\"]+\"\\)$")
list(LENGTH package_version_lines package_version_line_count)
if(NOT package_version_line_count EQUAL 1)
  message(FATAL_ERROR "Expected exactly one generated OR-Tools package version.")
endif()
list(GET package_version_lines 0 package_version_line)
string(REGEX REPLACE "^set\\(PACKAGE_VERSION \"([^\"]+)\"\\)$" "\\1"
  actual_ortools_version "${package_version_line}")
if(NOT actual_ortools_version STREQUAL ortools_version)
  message(FATAL_ERROR
    "Generated OR-Tools package version is ${actual_ortools_version}, expected ${ortools_version}.")
endif()

run_stage("ortools-build" "${work_root}"
  "${CMAKE_COMMAND}" --build "${ortools_build_dir}" --config Release --parallel 2)
run_stage("ortools-install" "${work_root}"
  "${CMAKE_COMMAND}" --install "${ortools_build_dir}" --config Release)
set(ortools_package_dir "${ortools_install_dir}/lib/cmake/ortools")
set(protobuf_package_dir "${ortools_install_dir}/lib/cmake/protobuf")
if(NOT EXISTS "${ortools_package_dir}/ortoolsConfig.cmake")
  message(FATAL_ERROR
    "The verified OR-Tools stage is missing its package config.")
endif()
if(NOT EXISTS "${protobuf_package_dir}/protobuf-config.cmake")
  message(FATAL_ERROR
    "The verified OR-Tools stage is missing its protobuf package config.")
endif()


set(worker_configure_command
  "${CMAKE_COMMAND}"
  -S "${REPOSITORY_ROOT}/workers/ortools"
  -B "${worker_build_dir}"
  -G Ninja
  ${common_cmake_flags}
  "-DCMAKE_CXX_COMPILER=${cxx_compiler}"
  "-DCMAKE_PREFIX_PATH=${ortools_install_dir}"
  "-Dortools_DIR=${ortools_package_dir}"
  "-DProtobuf_DIR=${protobuf_package_dir}"
  "-DCMAKE_FIND_USE_CMAKE_ENVIRONMENT_PATH=FALSE"
  "-DCMAKE_FIND_USE_PACKAGE_ROOT_PATH=FALSE"
  "-DCMAKE_FIND_USE_PACKAGE_REGISTRY=FALSE"
  "-DCMAKE_FIND_USE_SYSTEM_PACKAGE_REGISTRY=FALSE"
  "-DEUTHETO_ORTOOLS_DEVELOPMENT_BUILD=OFF"
  "-DEUTHETO_ORTOOLS_BUILD_TESTS=ON"
  "-DEUTHETO_ORTOOLS_BUILD_CANDIDATE_BENCHMARKS=OFF"
  "-DEUTHETO_ORTOOLS_PHASE3_CONTRACT=${source_contract_path}"
)
run_stage("worker-configure" "${work_root}" ${worker_configure_command})
set(worker_cache "${worker_build_dir}/CMakeCache.txt")
foreach(cache_entry_index RANGE 0 ${cache_entry_last})
  string(JSON cache_entry_name MEMBER "${source_contract_json}"
    cmake cache_entries ${cache_entry_index})
  contract_get(cache_entry_value cmake cache_entries "${cache_entry_name}")
  require_cache_value("${worker_cache}" "${cache_entry_name}" "${cache_entry_value}")
endforeach()
require_cache_value("${worker_cache}" EUTHETO_ORTOOLS_DEVELOPMENT_BUILD OFF)
require_cache_value("${worker_cache}" EUTHETO_ORTOOLS_BUILD_TESTS ON)
require_cache_value("${worker_cache}" EUTHETO_ORTOOLS_BUILD_CANDIDATE_BENCHMARKS OFF)
require_cache_value("${worker_cache}" EUTHETO_ORTOOLS_PHASE3_CONTRACT
  "${source_contract_path}")

run_stage("worker-build" "${work_root}"
  "${CMAKE_COMMAND}" --build "${worker_build_dir}" --config Release --parallel 2)
set(saved_path "$ENV{PATH}")
set(ENV{PATH} "${ortools_install_dir}/bin;${saved_path}")
run_stage("worker-ctest" "${work_root}"
  "${CMAKE_CTEST_COMMAND}" --test-dir "${worker_build_dir}" --build-config Release
  --output-on-failure --no-tests=error)
set(ENV{PATH} "${saved_path}")

require_cache_value("${worker_cache}" CMAKE_PREFIX_PATH "${ortools_install_dir}")
require_cache_value("${worker_cache}" ortools_DIR "${ortools_package_dir}")
require_cache_value("${worker_cache}" Protobuf_DIR "${protobuf_package_dir}")
require_cache_value("${worker_cache}" CMAKE_FIND_USE_CMAKE_ENVIRONMENT_PATH FALSE)
require_cache_value("${worker_cache}" CMAKE_FIND_USE_PACKAGE_ROOT_PATH FALSE)
require_cache_value("${worker_cache}" CMAKE_FIND_USE_PACKAGE_REGISTRY FALSE)
require_cache_value("${worker_cache}" CMAKE_FIND_USE_SYSTEM_PACKAGE_REGISTRY FALSE)
run_stage("worker-install" "${work_root}"
  "${CMAKE_COMMAND}" --install "${worker_build_dir}" --config Release
  --prefix "${staging_root}")
set(staging_worker "${staging_bin}/ortools-worker.exe")
if(NOT EXISTS "${staging_worker}")
  message(FATAL_ERROR "The production worker install did not create ${staging_worker}.")
endif()

file(GLOB stage_dlls "${ortools_install_dir}/bin/*.dll")
set(stage_dll_names "")
foreach(stage_dll IN LISTS stage_dlls)
  cmake_path(GET stage_dll FILENAME stage_dll_name)
  string(TOLOWER "${stage_dll_name}" stage_dll_name)
  if(stage_dll_name IN_LIST stage_dll_names)
    message(FATAL_ERROR
      "The verified OR-Tools stage contains colliding DLL basenames: ${stage_dll_name}.")
  endif()
  list(APPEND stage_dll_names "${stage_dll_name}")
endforeach()

set(windows_system_dll_allowlist
  advapi32.dll
  bcrypt.dll
  dbghelp.dll
  kernel32.dll
  ntdll.dll
)
set(msvc_runtime_dll_allowlist
  msvcp140.dll
  vcruntime140.dll
  vcruntime140_1.dll
)
function(inspect_pe artifact output_dependencies)
  execute_process(
    COMMAND "${dumpbin_executable}" /DEPENDENTS "${artifact}"
    RESULT_VARIABLE dependency_result
    OUTPUT_VARIABLE dependency_output
    ERROR_VARIABLE dependency_error
    TIMEOUT 60
    ENCODING UTF-8
  )
  if(NOT dependency_result EQUAL 0)
    message(FATAL_ERROR
      "dumpbin dependency inspection failed for ${artifact}: ${dependency_error}")
  endif()
  string(REPLACE "\r\n" "\n" dependency_output "${dependency_output}")
  string(REPLACE "\r" "\n" dependency_output "${dependency_output}")
  string(REPLACE ";" "\\;" dependency_output "${dependency_output}")
  string(REPLACE "\n" ";" dependency_lines "${dependency_output}")
  set(dependencies "")
  set(in_dependency_section FALSE)
  set(saw_dependency_section FALSE)
  foreach(dependency_line IN LISTS dependency_lines)
    string(STRIP "${dependency_line}" dependency_line)
    if(dependency_line MATCHES
       "^Image has the following (delay load )?dependencies:$")
      set(in_dependency_section TRUE)
      set(saw_dependency_section TRUE)
      continue()
    endif()
    if(dependency_line STREQUAL "Summary")
      set(in_dependency_section FALSE)
      continue()
    endif()
    if(NOT in_dependency_section OR dependency_line STREQUAL "")
      continue()
    endif()
    if(NOT dependency_line MATCHES
       "^[A-Za-z0-9_.+-]+\\.[dD][lL][lL]$")
      message(FATAL_ERROR
        "PE import names must be safe DLL basenames: ${artifact} -> ${dependency_line}")
    endif()
    string(TOLOWER "${dependency_line}" dependency_name)
    list(APPEND dependencies "${dependency_name}")
  endforeach()
  if(NOT saw_dependency_section)
    message(FATAL_ERROR
      "dumpbin output had no recognized dependency section for ${artifact}.")
  endif()
  list(REMOVE_DUPLICATES dependencies)
  list(SORT dependencies)

  execute_process(
    COMMAND "${dumpbin_executable}" /HEADERS "${artifact}"
    RESULT_VARIABLE headers_result
    OUTPUT_VARIABLE headers_output
    ERROR_VARIABLE headers_error
    TIMEOUT 60
    ENCODING UTF-8
  )
  string(TOLOWER "${headers_output}" headers_output)
  if(NOT headers_result EQUAL 0
     OR NOT headers_output MATCHES "8664 machine \\(x64\\)")
    message(FATAL_ERROR
      "Packaged PE artifact is not x64: ${artifact}\n${headers_error}")
  endif()
  set(${output_dependencies} "${dependencies}" PARENT_SCOPE)
endfunction()

set(runtime_queue "${staging_worker}")
set(inspected_runtime "")
while(runtime_queue)
  list(POP_FRONT runtime_queue artifact)
  string(TOLOWER "${artifact}" artifact_identity)
  if(artifact_identity IN_LIST inspected_runtime)
    continue()
  endif()
  list(APPEND inspected_runtime "${artifact_identity}")
  inspect_pe("${artifact}" runtime_dependencies)

  foreach(runtime_dependency IN LISTS runtime_dependencies)
    if(runtime_dependency MATCHES
       "(glpk|gurobi|cplex|xpress|python|libjvm|java|coreclr|hostfxr|dotnet)")
      message(FATAL_ERROR
        "Forbidden solver or language runtime dependency: ${artifact} -> ${runtime_dependency}")
    endif()
    if(runtime_dependency MATCHES
       "^(msvcp[0-9]+d|vcruntime[0-9_]+d|concrt[0-9]+d|ucrtbased)\\.dll$")
      message(FATAL_ERROR
        "Debug MSVC runtime dependency is forbidden: ${artifact} -> ${runtime_dependency}")
    endif()
    if(runtime_dependency MATCHES "^(api|ext)-ms-win-.*\\.dll$"
       OR runtime_dependency IN_LIST windows_system_dll_allowlist
       OR runtime_dependency IN_LIST msvc_runtime_dll_allowlist)
      continue()
    endif()

    set(runtime_source "")
    foreach(stage_dll IN LISTS stage_dlls)
      cmake_path(GET stage_dll FILENAME stage_dll_name)
      string(TOLOWER "${stage_dll_name}" stage_dll_name)
      if(stage_dll_name STREQUAL runtime_dependency)
        if(NOT runtime_source STREQUAL "")
          message(FATAL_ERROR
            "Runtime dependency has more than one staged source: ${runtime_dependency}")
        endif()
        set(runtime_source "${stage_dll}")
      endif()
    endforeach()
    if(runtime_source STREQUAL "")
      message(FATAL_ERROR
        "Runtime dependency is neither explicitly allowed nor in the verified stage: ${artifact} -> ${runtime_dependency}")
    endif()

    cmake_path(GET runtime_source FILENAME runtime_source_name)
    set(runtime_destination "${staging_bin}/${runtime_source_name}")
    file(COPY_FILE "${runtime_source}" "${runtime_destination}" ONLY_IF_DIFFERENT)
    list(APPEND runtime_queue "${runtime_destination}")
  endforeach()
endwhile()

file(GLOB staging_entries RELATIVE "${staging_root}" "${staging_root}/*")
if(NOT staging_entries STREQUAL "bin")
  message(FATAL_ERROR "The native worker staging root contains unexpected entries: ${staging_entries}")
endif()
file(GLOB final_bin_entries RELATIVE "${staging_bin}" "${staging_bin}/*")
foreach(final_bin_entry IN LISTS final_bin_entries)
  if(NOT final_bin_entry STREQUAL "ortools-worker.exe"
     AND NOT final_bin_entry MATCHES "^[A-Za-z0-9_.+-]+\\.[dD][lL][lL]$")
    message(FATAL_ERROR "The native worker bin directory contains an unclassified file: ${final_bin_entry}")
  endif()
endforeach()

# Remove every alternative loader source before the installed startup smoke.
file(REMOVE_RECURSE "${ortools_install_dir}" "${worker_build_dir}")
string(FIND "$ENV{PATH}" "${ortools_install_dir}" stage_path_index)
if(NOT stage_path_index EQUAL -1)
  message(FATAL_ERROR "The OR-Tools staging directory remains on PATH before final smoke.")
endif()
set(empty_input "${work_root}/empty-input")
file(WRITE "${empty_input}" "")
execute_process(
  COMMAND "${staging_worker}"
  WORKING_DIRECTORY "${staging_bin}"
  INPUT_FILE "${empty_input}"
  RESULT_VARIABLE worker_status
  OUTPUT_VARIABLE worker_stdout
  ERROR_VARIABLE worker_stderr
  TIMEOUT 30
  ENCODING UTF-8
)
if(NOT worker_status STREQUAL "64")
  message(FATAL_ERROR
    "Installed worker returned ${worker_status} for empty stdin, expected 64. stderr: ${worker_stderr}")
endif()
if(NOT worker_stdout STREQUAL "")
  message(FATAL_ERROR "Installed worker wrote protocol output for empty stdin.")
endif()

file(RENAME "${staging_root}" "${final_root}")
file(REMOVE_RECURSE "${work_root}")
if(NOT EXISTS "${final_worker}")
  message(FATAL_ERROR "Final native worker publication is incomplete: ${final_worker}")
endif()
message(STATUS
  "Built and verified the Windows x86_64 worker at ${final_worker}. Solver manifest, license/SBOM payload, sidecar packaging, backend registration, and release readiness remain deferred.")
