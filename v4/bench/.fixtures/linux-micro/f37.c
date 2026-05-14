/* synthetic kernel-ish source #37 */
#include <stdio.h>
int do_thing_37(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
