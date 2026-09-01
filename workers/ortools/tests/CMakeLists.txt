# SPDX-License-Identifier: Apache-2.0
add_executable(ortools-worker-native-tests native_tests.cc)
target_compile_features(ortools-worker-native-tests PRIVATE cxx_std_20)
set_target_properties(ortools-worker-native-tests PROPERTIES CXX_EXTENSIONS OFF)
target_link_libraries(ortools-worker-native-tests PRIVATE eutheto-ortools-worker-lib)
if(MSVC)
  target_compile_options(ortools-worker-native-tests PRIVATE /W4 /WX /permissive-)
else()
  target_compile_options(ortools-worker-native-tests PRIVATE
    -Wall -Wextra -Wpedantic -Werror
  )
endif()
add_test(NAME ortools-worker-native COMMAND ortools-worker-native-tests)
set_tests_properties(ortools-worker-native PROPERTIES TIMEOUT 60)
