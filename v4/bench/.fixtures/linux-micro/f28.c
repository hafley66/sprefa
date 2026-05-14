/* synthetic kernel-ish source #28 */
#include <stdio.h>
int do_thing_28(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
