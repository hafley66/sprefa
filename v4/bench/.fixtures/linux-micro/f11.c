/* synthetic kernel-ish source #11 */
#include <stdio.h>
int do_thing_11(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
