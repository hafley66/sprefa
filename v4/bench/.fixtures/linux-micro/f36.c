/* synthetic kernel-ish source #36 */
#include <stdio.h>
int do_thing_36(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
