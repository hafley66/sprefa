/* synthetic kernel-ish source #17 */
#include <stdio.h>
int do_thing_17(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
