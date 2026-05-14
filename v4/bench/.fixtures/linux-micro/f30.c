/* synthetic kernel-ish source #30 */
#include <stdio.h>
int do_thing_30(int x) {
  printk("KERN_INFO: %d", x);
  printk("ANOTHER: %s %d", "hello", x);
  return 0;
}
