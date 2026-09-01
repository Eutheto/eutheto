// SPDX-License-Identifier: Apache-2.0
#include <iostream>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

#include "worker.h"

int main(int argc, char** argv) {
  if (argc != 1 || argv == nullptr) return eutheto::ortools_worker::kExitUsage;
#ifdef _WIN32
  if (_setmode(_fileno(stdin), _O_BINARY) == -1 ||
      _setmode(_fileno(stdout), _O_BINARY) == -1) {
    return eutheto::ortools_worker::kExitConfiguration;
  }
#endif
  return eutheto::ortools_worker::RunSession(std::cin, std::cout, std::cerr);
}
