/* synthetic kernel-ish source #34 */
#include <stdio.h>
int do_thing_34(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
