/* synthetic kernel-ish source #14 */
#include <stdio.h>
int do_thing_14(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
