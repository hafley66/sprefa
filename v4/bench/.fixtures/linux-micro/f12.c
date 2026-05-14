/* synthetic kernel-ish source #12 */
#include <stdio.h>
int do_thing_12(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
