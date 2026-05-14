/* synthetic kernel-ish source #26 */
#include <stdio.h>
int do_thing_26(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
