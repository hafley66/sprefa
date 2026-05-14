/* synthetic kernel-ish source #24 */
#include <stdio.h>
int do_thing_24(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
