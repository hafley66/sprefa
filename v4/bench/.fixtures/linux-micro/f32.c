/* synthetic kernel-ish source #32 */
#include <stdio.h>
int do_thing_32(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
