/* synthetic kernel-ish source #1 */
#include <stdio.h>
int do_thing_1(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
