/* synthetic kernel-ish source #15 */
#include <stdio.h>
int do_thing_15(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
