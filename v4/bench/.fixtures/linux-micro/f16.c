/* synthetic kernel-ish source #16 */
#include <stdio.h>
int do_thing_16(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
