/* synthetic kernel-ish source #29 */
#include <stdio.h>
int do_thing_29(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
