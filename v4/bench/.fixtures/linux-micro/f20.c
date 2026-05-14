/* synthetic kernel-ish source #20 */
#include <stdio.h>
int do_thing_20(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
