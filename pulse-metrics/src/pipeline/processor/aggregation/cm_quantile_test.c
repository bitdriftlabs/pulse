#include <stdint.h>
#include <stdlib.h>

// The statsite cm_quantile tests includes a single test that relies on srandom() and random() with
// a well defined seed. It's not immediately obvious how to call these functions from Rust so here
// we are.

void cm_quantile_test_initialize_c() {
  srandom(42);
}

uint64_t cm_quantile_test_random_c() {
  return random();
}
