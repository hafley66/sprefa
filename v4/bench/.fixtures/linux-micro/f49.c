/* synthetic kernel-ish source #49 */
#include <stdio.h>
int do_thing_49(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
